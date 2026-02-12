pub mod ast;
pub mod cli;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod provider;
pub mod resolver;
pub mod runtime;
pub mod schema;
pub mod token;
pub mod value;

use ast::Program;
use error::SaftResult;

pub fn check_source(source: &str) -> SaftResult<Program> {
    let tokens = lexer::lex(source)?;
    let program = parser::parse(tokens)?;
    resolver::resolve(&program, runtime::BUILTIN_NAMES)?;
    Ok(program)
}

pub fn run_source(source: &str) -> SaftResult<()> {
    run_source_with_provider_and_options(
        source,
        Box::new(provider::HeuristicMockProvider::new()),
        runtime::RuntimeOptions::default(),
    )
}

pub fn run_source_with_provider(
    source: &str,
    provider: Box<dyn provider::PromptProvider>,
) -> SaftResult<()> {
    run_source_with_provider_and_options(source, provider, runtime::RuntimeOptions::default())
}

pub fn run_source_with_provider_and_options(
    source: &str,
    provider: Box<dyn provider::PromptProvider>,
    options: runtime::RuntimeOptions,
) -> SaftResult<()> {
    let program = check_source(source)?;
    let mut runtime = runtime::Runtime::with_provider_and_options(provider, options);
    runtime.run_program(&program)
}
