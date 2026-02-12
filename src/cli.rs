use std::env;
use std::fs;

use crate::error::SaftError;
use crate::provider::{HeuristicMockProvider, NoopProvider, OpenRouterProvider, PromptProvider};
use crate::runtime::RuntimeOptions;

pub fn run(args: Vec<String>) -> i32 {
    match parse_args(&args) {
        Ok(command) => {
            if let Err(err) = execute(command) {
                eprintln!("{err}");
                return 1;
            }
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

#[derive(Debug, Clone)]
enum Command {
    Check {
        file: String,
        autofmt: bool,
    },
    Run {
        file: String,
        provider: ProviderKind,
        api_key_env: String,
        model: Option<String>,
        temperature: Option<f32>,
        max_tool_rounds: usize,
        max_tool_calls: usize,
        autofmt: bool,
    },
    Fmt {
        file: String,
        write: bool,
        check: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderKind {
    Mock,
    OpenRouter,
    None,
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    if args.len() < 2 {
        return Err(usage(
            args.first().map(String::as_str).unwrap_or("orangensaft"),
        ));
    }

    match args[1].as_str() {
        "check" => parse_check_command(args),
        "fmt" => parse_fmt_command(args),
        "run" => parse_run_command(args, 2, 3),
        _ => parse_run_command(args, 1, 2),
    }
}

fn parse_check_command(args: &[String]) -> Result<Command, String> {
    let bin_name = args.first().map(String::as_str).unwrap_or("orangensaft");
    if args.len() < 3 {
        return Err(format!("missing file path\n{}", usage(bin_name)));
    }
    let file = args[2].clone();
    let mut autofmt = false;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--autofmt" => {
                autofmt = true;
                i += 1;
            }
            other => return Err(format!("unknown option '{other}'\n{}", usage(bin_name))),
        }
    }

    Ok(Command::Check { file, autofmt })
}

fn parse_fmt_command(args: &[String]) -> Result<Command, String> {
    let bin_name = args.first().map(String::as_str).unwrap_or("orangensaft");
    if args.len() < 3 {
        return Err(format!("missing file path\n{}", usage(bin_name)));
    }

    let file = args[2].clone();
    let mut write = false;
    let mut check = false;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--write" => {
                write = true;
                i += 1;
            }
            "--check" => {
                check = true;
                i += 1;
            }
            other => return Err(format!("unknown option '{other}'\n{}", usage(bin_name))),
        }
    }

    if write && check {
        return Err("fmt options --write and --check are mutually exclusive".to_string());
    }

    Ok(Command::Fmt { file, write, check })
}

#[derive(Debug, Clone)]
struct RunDefaults {
    provider: ProviderKind,
    api_key_env: String,
    model: Option<String>,
    temperature: Option<f32>,
    max_tool_rounds: usize,
    max_tool_calls: usize,
}

fn parse_run_command(
    args: &[String],
    file_index: usize,
    option_start: usize,
) -> Result<Command, String> {
    let bin_name = args.first().map(String::as_str).unwrap_or("orangensaft");
    if file_index >= args.len() {
        return Err(format!("missing file path\n{}", usage(bin_name)));
    }

    let file = args[file_index].clone();
    let defaults = run_defaults()?;
    let mut provider = defaults.provider;
    let mut api_key_env = defaults.api_key_env;
    let mut model = defaults.model;
    let mut temperature = defaults.temperature;
    let mut max_tool_rounds = defaults.max_tool_rounds;
    let mut max_tool_calls = defaults.max_tool_calls;
    let mut autofmt = false;
    let mut i = option_start;

    while i < args.len() {
        match args[i].as_str() {
            "--api-key-env" => {
                if i + 1 >= args.len() {
                    return Err(format!("missing value for option '{}'", args[i]));
                }
                api_key_env = args[i + 1].clone();
                i += 2;
            }
            "--model" => {
                if i + 1 >= args.len() {
                    return Err(format!("missing value for option '{}'", args[i]));
                }
                model = Some(args[i + 1].clone());
                i += 2;
            }
            "--temperature" => {
                if i + 1 >= args.len() {
                    return Err(format!("missing value for option '{}'", args[i]));
                }
                temperature = Some(parse_f32_option("--temperature", &args[i + 1])?);
                i += 2;
            }
            "--max-tool-rounds" => {
                if i + 1 >= args.len() {
                    return Err("missing value for option '--max-tool-rounds'".to_string());
                }
                max_tool_rounds = parse_usize_option("--max-tool-rounds", &args[i + 1])?;
                i += 2;
            }
            "--max-tool-calls" => {
                if i + 1 >= args.len() {
                    return Err("missing value for option '--max-tool-calls'".to_string());
                }
                max_tool_calls = parse_usize_option("--max-tool-calls", &args[i + 1])?;
                i += 2;
            }
            "--provider" => {
                if i + 1 >= args.len() {
                    return Err("missing value for option '--provider'".to_string());
                }
                provider = parse_provider_kind(&args[i + 1])?;
                i += 2;
            }
            "--autofmt" => {
                autofmt = true;
                i += 1;
            }
            other => {
                return Err(format!("unknown option '{other}'\n{}", usage(bin_name)));
            }
        }
    }

    Ok(Command::Run {
        file,
        provider,
        api_key_env,
        model,
        temperature,
        max_tool_rounds,
        max_tool_calls,
        autofmt,
    })
}

fn parse_provider_kind(raw: &str) -> Result<ProviderKind, String> {
    match raw {
        "mock" => Ok(ProviderKind::Mock),
        "openrouter" => Ok(ProviderKind::OpenRouter),
        "none" => Ok(ProviderKind::None),
        other => Err(format!(
            "invalid provider '{other}' (expected 'mock', 'openrouter', or 'none')"
        )),
    }
}

fn run_defaults() -> Result<RunDefaults, String> {
    let runtime_defaults = RuntimeOptions::default();
    let provider = match env::var("ORANGENSAFT_PROVIDER") {
        Ok(value) => parse_provider_kind(&value)
            .map_err(|err| format!("invalid ORANGENSAFT_PROVIDER value: {err}"))?,
        Err(_) => ProviderKind::Mock,
    };

    let api_key_env =
        env::var("ORANGENSAFT_API_KEY_ENV").unwrap_or_else(|_| "OPENROUTER_API_KEY".to_string());
    let model = env::var("ORANGENSAFT_MODEL").ok();
    let temperature = match env::var("ORANGENSAFT_TEMPERATURE") {
        Ok(raw) => Some(parse_f32_option("ORANGENSAFT_TEMPERATURE", &raw)?),
        Err(_) => None,
    };
    let max_tool_rounds = match env::var("ORANGENSAFT_MAX_TOOL_ROUNDS") {
        Ok(raw) => parse_usize_option("ORANGENSAFT_MAX_TOOL_ROUNDS", &raw)?,
        Err(_) => runtime_defaults.max_tool_rounds,
    };
    let max_tool_calls = match env::var("ORANGENSAFT_MAX_TOOL_CALLS") {
        Ok(raw) => parse_usize_option("ORANGENSAFT_MAX_TOOL_CALLS", &raw)?,
        Err(_) => runtime_defaults.max_tool_calls,
    };

    Ok(RunDefaults {
        provider,
        api_key_env,
        model,
        temperature,
        max_tool_rounds,
        max_tool_calls,
    })
}

fn execute(command: Command) -> Result<(), String> {
    match command {
        Command::Check { file, autofmt } => {
            let source = read_file(&file)?;
            let source_to_check = if autofmt {
                crate::format_source(&source).map_err(|err| render_error(err, &file, &source))?
            } else {
                source.clone()
            };

            match crate::check_source(&source_to_check) {
                Ok(_) => {
                    println!("OK: {file}");
                    Ok(())
                }
                Err(err) => Err(render_error(err, &file, &source_to_check)),
            }
        }
        Command::Run {
            file,
            provider,
            api_key_env,
            model,
            temperature,
            max_tool_rounds,
            max_tool_calls,
            autofmt,
        } => {
            let source = read_file(&file)?;
            let source_to_run = if autofmt {
                crate::format_source(&source).map_err(|err| render_error(err, &file, &source))?
            } else {
                source.clone()
            };
            let provider: Box<dyn PromptProvider> = match provider {
                ProviderKind::Mock => Box::new(HeuristicMockProvider::new()),
                ProviderKind::OpenRouter => {
                    let provider = OpenRouterProvider::from_env(&api_key_env, model, temperature)
                        .map_err(|err| err.message)?;
                    Box::new(provider)
                }
                ProviderKind::None => Box::new(NoopProvider),
            };

            let options = RuntimeOptions {
                max_tool_rounds,
                max_tool_calls,
            };

            match crate::run_source_with_provider_and_options(&source_to_run, provider, options) {
                Ok(_) => Ok(()),
                Err(err) => Err(render_error(err, &file, &source_to_run)),
            }
        }
        Command::Fmt { file, write, check } => {
            let source = read_file(&file)?;
            let formatted =
                crate::format_source(&source).map_err(|err| render_error(err, &file, &source))?;

            if check {
                if source == formatted {
                    println!("OK: {file}");
                    Ok(())
                } else {
                    Err(format!("not formatted: {file}"))
                }
            } else if write {
                fs::write(&file, formatted)
                    .map_err(|err| format!("failed to write '{file}': {err}"))
            } else {
                print!("{formatted}");
                Ok(())
            }
        }
    }
}

fn read_file(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("failed to read '{path}': {err}"))
}

fn render_error(err: SaftError, file: &str, source: &str) -> String {
    err.render(file, source)
}

fn usage(bin_name: &str) -> String {
    format!(
        "Usage:\n  {bin_name} check <file.saft> [--autofmt]\n  {bin_name} run <file.saft> [options]\n  {bin_name} fmt <file.saft> [--write|--check]\n  {bin_name} <file.saft> [options]\n\nOptions (run/check):\n  --provider mock|openrouter|none\n  --api-key-env ENV\n  --model NAME\n  --temperature N\n  --max-tool-rounds N\n  --max-tool-calls N\n  --autofmt\n\nOptions (fmt):\n  --write   write formatted output back to file\n  --check   fail if file is not already formatted\n\nDefault values can be set once with env vars:\n  ORANGENSAFT_PROVIDER=mock|openrouter|none\n  ORANGENSAFT_API_KEY_ENV=OPENROUTER_API_KEY\n  ORANGENSAFT_MODEL=openai/gpt-4o-mini\n  ORANGENSAFT_TEMPERATURE=0\n  ORANGENSAFT_MAX_TOOL_ROUNDS=8\n  ORANGENSAFT_MAX_TOOL_CALLS=32"
    )
}

fn parse_usize_option(name: &str, raw: &str) -> Result<usize, String> {
    raw.parse::<usize>()
        .map_err(|_| format!("invalid value for {name}: '{raw}'"))
}

fn parse_f32_option(name: &str, raw: &str) -> Result<f32, String> {
    raw.parse::<f32>()
        .map_err(|_| format!("invalid value for {name}: '{raw}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_run_subcommand_with_options() {
        let args = vec![
            "orangensaft".to_string(),
            "run".to_string(),
            "examples/11_simple_array_op_2.saft".to_string(),
            "--provider".to_string(),
            "none".to_string(),
        ];

        let command = parse_args(&args).expect("expected run command to parse");
        match command {
            Command::Run {
                file,
                provider,
                autofmt,
                ..
            } => {
                assert_eq!(file, "examples/11_simple_array_op_2.saft");
                assert_eq!(provider, ProviderKind::None);
                assert!(!autofmt);
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn parses_shorthand_file_invocation() {
        let args = vec![
            "orangensaft".to_string(),
            "examples/11_simple_array_op_2.saft".to_string(),
            "--provider".to_string(),
            "none".to_string(),
        ];

        let command = parse_args(&args).expect("expected shorthand command to parse");
        match command {
            Command::Run {
                file,
                provider,
                autofmt,
                ..
            } => {
                assert_eq!(file, "examples/11_simple_array_op_2.saft");
                assert_eq!(provider, ProviderKind::None);
                assert!(!autofmt);
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn parses_fmt_subcommand() {
        let args = vec![
            "orangensaft".to_string(),
            "fmt".to_string(),
            "examples/11_simple_array_op_2.saft".to_string(),
            "--check".to_string(),
        ];

        let command = parse_args(&args).expect("expected fmt command to parse");
        match command {
            Command::Fmt { file, check, write } => {
                assert_eq!(file, "examples/11_simple_array_op_2.saft");
                assert!(check);
                assert!(!write);
            }
            other => panic!("expected fmt command, got {other:?}"),
        }
    }

    #[test]
    fn parses_autofmt_flag_for_run() {
        let args = vec![
            "orangensaft".to_string(),
            "run".to_string(),
            "examples/11_simple_array_op_2.saft".to_string(),
            "--autofmt".to_string(),
        ];

        let command = parse_args(&args).expect("expected run command to parse");
        match command {
            Command::Run { autofmt, .. } => assert!(autofmt),
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn parses_autofmt_flag_for_check() {
        let args = vec![
            "orangensaft".to_string(),
            "check".to_string(),
            "examples/11_simple_array_op_2.saft".to_string(),
            "--autofmt".to_string(),
        ];

        let command = parse_args(&args).expect("expected check command to parse");
        match command {
            Command::Check { file, autofmt } => {
                assert_eq!(file, "examples/11_simple_array_op_2.saft");
                assert!(autofmt);
            }
            other => panic!("expected check command, got {other:?}"),
        }
    }
}
