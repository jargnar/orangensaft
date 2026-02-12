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
    },
    Run {
        file: String,
        provider: ProviderKind,
        api_key_env: String,
        model: Option<String>,
        temperature: Option<f32>,
        max_tool_rounds: usize,
        max_tool_calls: usize,
    },
}

#[derive(Debug, Clone, Copy)]
enum ProviderKind {
    Mock,
    OpenRouter,
    None,
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    if args.len() < 3 {
        return Err(usage(
            args.first().map(String::as_str).unwrap_or("orangensaft"),
        ));
    }

    match args[1].as_str() {
        "check" => {
            if args.len() != 3 {
                return Err(format!(
                    "'check' expects exactly one file path\n{}",
                    usage(&args[0])
                ));
            }
            Ok(Command::Check {
                file: args[2].clone(),
            })
        }
        "run" => {
            let file = args[2].clone();
            let mut provider = ProviderKind::Mock;
            let mut api_key_env = "OPENROUTER_API_KEY".to_string();
            let mut model: Option<String> = None;
            let mut temperature: Option<f32> = None;
            let mut max_tool_rounds = RuntimeOptions::default().max_tool_rounds;
            let mut max_tool_calls = RuntimeOptions::default().max_tool_calls;
            let mut i = 3usize;
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
                        provider = match args[i + 1].as_str() {
                            "mock" => ProviderKind::Mock,
                            "openrouter" => ProviderKind::OpenRouter,
                            "none" => ProviderKind::None,
                            other => {
                                return Err(format!(
                                    "invalid provider '{other}' (expected 'mock', 'openrouter', or 'none')"
                                ));
                            }
                        };
                        i += 2;
                    }
                    other => {
                        return Err(format!("unknown option '{other}'\n{}", usage(&args[0])));
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
            })
        }
        _ => Err(usage(&args[0])),
    }
}

fn execute(command: Command) -> Result<(), String> {
    match command {
        Command::Check { file } => {
            let source = read_file(&file)?;
            match crate::check_source(&source) {
                Ok(_) => {
                    println!("OK: {file}");
                    Ok(())
                }
                Err(err) => Err(render_error(err, &file, &source)),
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
        } => {
            let source = read_file(&file)?;
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

            match crate::run_source_with_provider_and_options(&source, provider, options) {
                Ok(_) => Ok(()),
                Err(err) => Err(render_error(err, &file, &source)),
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
        "Usage:\n  {bin_name} check <file.saft>\n  {bin_name} run <file.saft> [--provider mock|openrouter|none] [--api-key-env ENV] [--model NAME] [--temperature N] [--max-tool-rounds N] [--max-tool-calls N]"
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
