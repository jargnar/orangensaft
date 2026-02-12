# AGENTS.md

This is the single LLM/agent entrypoint for Orangensaft.

## 1. Repo Snapshot

- Language/runtime: Rust (`edition = 2024`)
- Crate: `orangensaft`
- Dependency footprint: `serde_json`
- Binary entrypoint: `src/main.rs` -> `orangensaft::cli::run`

Top-level directories:
- `src/`: lexer, parser, resolver, runtime, providers, stdlib
- `examples/`: runnable `.saft` samples
- `tests/`: integration test suites
- `docs/`: design notes and grammar draft

## 2. What Orangensaft Is

Orangensaft is a small interpreted language with:
- Deterministic scripting (variables, functions, loops, assertions, arithmetic).
- Explicit prompt expressions (`$ ... $`) for LLM interaction.
- Runtime schema annotations for assignments and function params/returns.
- True tool-calling: interpolating function values in prompts exposes callable tools.

Design boundary:
- Deterministic core semantics live in interpreter logic.
- Probabilistic behavior is explicit and isolated to prompt expressions.

## 3. Quick Commands

- Run all tests:
  - `cargo test`
- Parse + resolve only:
  - `cargo run -- check examples/01_vanilla_assignments.saft`
- Run with mock provider:
  - `cargo run -- run examples/06_function_map.saft --provider mock`
- Run shorthand (no `run` subcommand):
  - `cargo run -- examples/06_function_map.saft --provider mock`
- Run with OpenRouter:
  - `cargo run -- run examples/11_simple_array_op_2.saft --provider openrouter --api-key-env OPENROUTER_API_KEY --model openai/gpt-4o-mini --temperature 0 --max-tool-rounds 8 --max-tool-calls 32`
- Install binary and run directly:
  - `cargo install --path .`
  - `orangensaft examples/11_simple_array_op_2.saft`
- Set persistent CLI defaults via env vars (optional):
  - `ORANGENSAFT_PROVIDER`
  - `ORANGENSAFT_API_KEY_ENV`
  - `ORANGENSAFT_MODEL`
  - `ORANGENSAFT_TEMPERATURE`
  - `ORANGENSAFT_MAX_TOOL_ROUNDS`
  - `ORANGENSAFT_MAX_TOOL_CALLS`

## 4. Language Surface (Current Implementation)

Statements:
- function definition: `f name(params): ...`
- assignment: `name = expr`
- typed assignment: `name: schema = expr`
- conditionals: `if ...: ... else: ...`
- loop: `for x in iterable: ...`
- return: `ret expr`
- assertion: `assert expr`
- expression statement: `expr`

Expressions:
- literals: int/float/string/bool/nil
- list/tuple/object literals
- calls, indexing, object member access
- tuple index sugar (`value.0`)
- unary: `-`, `not`
- binary: arithmetic/comparison/logical
- prompt expression: `$ ... {interpolation_expr} ... $`

Schema annotations:
- primitives: `int`, `float`, `bool`, `string`, `any`
- list: `[schema]`
- tuple: `(schema, schema, ...)`
- object: `{field: schema, ...}`
- union: `a | b`
- optional: `schema?`

## 5. Standard Library (`src/stdlib.rs`)

Builtin functions currently installed by runtime:
- `upper(string) -> string`
- `print(any) -> nil`
  - prints to stdout with newline
  - string arguments print as raw text (without surrounding quotes)
- `len(string|list|tuple|object) -> int`
- `type(any) -> string`
  - returns runtime kind names (`int`, `float`, `bool`, `string`, `list`, `tuple`, `object`, `function`, `nil`)

Where builtins are wired:
- declarations: `src/stdlib.rs`
- resolver-visible builtin names: `stdlib::BUILTIN_NAMES`
- runtime registration: `Runtime::install_builtins` in `src/runtime.rs`

## 6. Execution Pipeline

1. `src/lexer.rs`: source -> tokens (`NEWLINE/INDENT/DEDENT`, prompt tokenization).
2. `src/parser.rs`: tokens -> AST.
3. `src/resolver.rs`: lightweight name checks.
4. `src/runtime.rs`: interpreter execution.
5. `src/provider.rs`: prompt provider backend.

Public orchestration API:
- `check_source` in `src/lib.rs`: lex + parse + resolve
- `run_source*` in `src/lib.rs`: check + runtime

## 7. Module Responsibilities

- `src/token.rs`: token kinds and token struct
- `src/error.rs`: span-aware errors and renderer
- `src/ast.rs`: AST and schema AST nodes
- `src/lexer.rs`: lexing, indentation handling, prompt block lexing
- `src/parser.rs`: recursive-descent parsing, prompt interpolation parsing, schema parsing
- `src/resolver.rs`: undefined-name and duplicate checks
- `src/value.rs`: runtime value model and truthiness
- `src/schema.rs`: schema validation + JSON Schema conversion
- `src/provider.rs`: `PromptProvider` protocol + mock/openrouter providers
- `src/stdlib.rs`: builtin function definitions
- `src/runtime.rs`: interpreter, prompt rendering/tool loop, typed prompt repair
- `src/cli.rs`: CLI parsing/execution

## 8. Runtime Semantics That Matter

Assignments:
- untyped assignment stores evaluated value directly
- typed non-prompt assignment validates evaluated value against schema
- typed prompt assignment:
  - appends strict JSON output contract to prompt
  - requires JSON parse + schema validation
  - retries once with repair prompt on failure

Prompt interpolation:
- non-function interpolation serializes value as JSON text into prompt
- function interpolation:
  - exposes function as callable tool
  - inserts tool name into rendered prompt
  - bare variable interpolation uses variable name as tool name
  - non-variable function expression gets generated tool name (`tool_1`, ...)

Tool-call loop:
- provider receives prompt + exposed tools + prior tool results
- model may return tool calls or final text
- runtime executes tool calls through interpreter
- loop guarded by `max_tool_rounds` and `max_tool_calls`

Function semantics:
- closures are captured
- parameter schema validated at call time
- return schema validated before returning to caller
- missing `ret` => `nil`
- top-level `ret` is runtime error

Operator semantics:
- `+` supports numeric addition and string concatenation
- `-`, `*`, `/` numeric only (`%` int-only)
- `and`/`or` return booleans based on truthiness
- truthiness: only `false` and `nil` are falsey

## 9. Providers

`src/provider.rs` defines:
- `PromptRequest { prompt, tools, tool_results }`
- `PromptResponse::{FinalText, ToolCalls}`
- trait: `PromptProvider`

Implementations:
- `HeuristicMockProvider`: deterministic heuristics for local tests/examples
- `SequenceProvider`: deterministic queued responses for tests
- `OpenRouterProvider`: `curl` call to OpenRouter chat completions API
- `NoopProvider`: explicit error when prompts are attempted

## 10. Tests and Coverage

Readable integration test suites:
- `tests/core_language.rs`
  - baseline deterministic execution
  - resolver undefined-name behavior
  - assignment schema enforcement
- `tests/prompt_assignments.rs`
  - prompt assignment behavior
  - typed prompt JSON validation + repair path
- `tests/tool_calling.rs`
  - function tool-calling scenarios
  - alias/capability behavior
- `tests/stdlib.rs`
  - stdlib builtins (`upper`, `print`, `len`, `type`)
  - CLI-level stdout assertion for `print`

Examples:
- `examples/01`..`11` for baseline/prompt/tool-call behavior
- `examples/12_stdlib_basics.saft` for stdlib usage

## 11. Invariants to Preserve

- `$ ... $` stays the explicit LLM boundary.
- Untyped prompt assignment yields string output.
- Typed prompt assignment must be JSON + schema-valid, with at most one repair retry.
- Function interpolation must expose real interpreter-callable tools.
- Errors should keep precise spans for diagnostics.
- Tests should remain green with `cargo test`.

## 12. Known Sharp Edges

- Resolver is intentionally lightweight (not full flow analysis).
- Forward-reference patterns can pass resolver but still fail at runtime order-of-execution.
- Mock provider is heuristic, not a general model substitute.
- OpenRouter integration shells out to `curl`.

## 13. Change Playbooks

Add a builtin function:
1. Add function in `src/stdlib.rs`.
2. Register it in `BUILTINS` and `BUILTIN_NAMES`.
3. Add coverage in `tests/stdlib.rs` (and examples if user-facing).

Add syntax/operator:
1. `src/token.rs`
2. `src/lexer.rs`
3. `src/parser.rs`
4. `src/ast.rs` (if node shape changes)
5. `src/runtime.rs`
6. `src/resolver.rs` / `src/schema.rs` if needed
7. tests + docs updates

Add schema feature:
1. Extend `SchemaExpr` (`src/ast.rs`)
2. Parse in `src/parser.rs`
3. Validate in `src/schema.rs`
4. JSON Schema conversion in `src/schema.rs`
5. typed-prompt contract behavior checks in runtime/tests

Add provider:
1. Implement `PromptProvider`
2. Wire CLI option in `src/cli.rs`
3. Add deterministic tests via `SequenceProvider` where possible

## 14. Update Discipline

For behavior changes:
1. Prefer adding/updating tests first.
2. Keep lexer/parser/resolver/runtime semantics aligned.
3. Run `cargo test`.
4. Update:
   - `AGENTS.md` (this file)
   - `README.md` if user-facing behavior changes
   - `docs/v0-grammar-ast.md` when grammar or semantic contracts shift
