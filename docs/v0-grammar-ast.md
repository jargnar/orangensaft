# Orangensaft v0: Grammar and AST Draft

This draft is intentionally small. It is designed to get to a first milestone quickly:

- Parse and run a mini Python-like language
- Add one explicit LLM expression form (`$ ... $`)
- Optionally validate values with assignment annotations (`name: <schema> = expr`)
- Support true model tool-calling through normal interpolation

## 1. v0 Surface Goals

- Python-like blocks (`:` + indentation)
- Dynamic runtime values (no full static type system)
- Deterministic core language semantics
- LLM calls modeled as explicit runtime effects via `PromptExpr`
- Runtime schema checks on annotated assignments
- Ergonomic prompt authoring: function interpolation (`{my_func}`) auto-exposes tools

## 2. Concrete Grammar (EBNF-style)

This grammar assumes lexer support for `NEWLINE`, `INDENT`, and `DEDENT`.

```ebnf
program         ::= stmt* EOF ;

stmt            ::= fn_def
                  | if_stmt
                  | for_stmt
                  | return_stmt
                  | assert_stmt
                  | assign_stmt
                  | expr_stmt ;

fn_def          ::= "f" IDENT "(" param_list? ")" return_annot? ":" NEWLINE INDENT stmt+ DEDENT ;
param_list      ::= param ("," param)* ;
param           ::= IDENT (":" schema_expr)? ;
return_annot    ::= "->" schema_expr ;

if_stmt         ::= "if" expr ":" NEWLINE INDENT stmt+ DEDENT ("else" ":" NEWLINE INDENT stmt+ DEDENT)? ;

for_stmt        ::= "for" pattern "in" expr ":" NEWLINE INDENT stmt+ DEDENT ;
pattern         ::= IDENT
                  | IDENT ("," IDENT)+ ;   // tuple destructure in loops only (v0)

return_stmt     ::= "ret" expr? NEWLINE ;
assert_stmt     ::= "assert" expr NEWLINE ;
assign_stmt     ::= IDENT (":" schema_expr)? "=" expr NEWLINE ;
expr_stmt       ::= expr NEWLINE ;

expr            ::= logic_or ;
logic_or        ::= logic_and ("or" logic_and)* ;
logic_and       ::= equality ("and" equality)* ;
equality        ::= comparison (("==" | "!=") comparison)* ;
comparison      ::= term ((">" | ">=" | "<" | "<=") term)* ;
term            ::= factor (("+" | "-") factor)* ;
factor          ::= unary (("*" | "/" | "%") unary)* ;
unary           ::= ("-" | "not") unary
                  | postfix ;

postfix         ::= primary postfix_op* ;
postfix_op      ::= "(" arg_list? ")"
                  | "[" expr "]"
                  | "." IDENT
                  | "." INT ;               // tuple index sugar (e.g., p.0)
arg_list        ::= expr ("," expr)* ;

primary         ::= INT
                  | FLOAT
                  | STRING
                  | "true"
                  | "false"
                  | "nil"
                  | IDENT
                  | list_lit
                  | tuple_lit
                  | object_lit
                  | prompt_expr
                  | "(" expr ")" ;

list_lit        ::= "[" (expr ("," expr)*)? "]" ;
tuple_lit       ::= "(" expr "," expr ("," expr)* ")" ;    // at least 2
object_lit      ::= "{" (IDENT ":" expr ("," IDENT ":" expr)*)? "}" ;

prompt_expr     ::= "$" prompt_part* "$" ;
prompt_part     ::= PROMPT_TEXT
                  | "{" expr "}" ;
```

### Prompt Lexing Mode

Inside `$ ... $`, the lexer switches to prompt mode:

- Anything is `PROMPT_TEXT` until:
  - `{` starts interpolation (parse normal `expr` until matching `}`)
  - `$` closes the prompt block
- Newlines are preserved as text in prompt parts
- `//` line comments are supported in normal mode (outside prompt mode)

## 3. Schema Grammar (zod-lite runtime checker)

```ebnf
schema_expr      ::= union_schema ;
union_schema     ::= schema_primary ("|" schema_primary)* optional_suffix? ;
optional_suffix  ::= "?" ;

schema_primary   ::= primitive_schema
                   | list_schema
                   | tuple_schema
                   | object_schema
                   | "(" schema_expr ")" ;

primitive_schema ::= "int" | "float" | "bool" | "string" | "any" ;
list_schema      ::= "[" schema_expr "]" ;
tuple_schema     ::= "(" schema_expr "," schema_expr ("," schema_expr)* ")" ;
object_schema    ::= "{" schema_field ("," schema_field)* "}" ;
schema_field     ::= IDENT ":" schema_expr ;
```

## 4. AST Shape (Rust-friendly)

Use spans on every node for quality diagnostics.

```rust
#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    FnDef(FnDef),
    Assign { name: String, annotation: Option<SchemaExpr>, value: Expr, span: Span },
    If { cond: Expr, then_block: Vec<Stmt>, else_block: Option<Vec<Stmt>>, span: Span },
    For { pattern: Pattern, iter: Expr, body: Vec<Stmt>, span: Span },
    Return { value: Option<Expr>, span: Span },
    Assert { expr: Expr, span: Span },
    Expr { expr: Expr, span: Span },
}

#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub params: Vec<FnParam>,
    pub return_schema: Option<SchemaExpr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FnParam {
    pub name: String,
    pub schema: Option<SchemaExpr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Name(String),
    Tuple(Vec<String>),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    Str(String, Span),
    Nil(Span),
    Var(String, Span),
    List(Vec<Expr>, Span),
    Tuple(Vec<Expr>, Span),
    Object(Vec<(String, Expr)>, Span),

    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Member {
        target: Box<Expr>,
        name: String,
        span: Span,
    },
    TupleIndex {
        target: Box<Expr>,
        index: usize,
        span: Span,
    },
    Prompt(PromptExpr),
}

#[derive(Debug, Clone)]
pub enum UnaryOp { Neg, Not }

#[derive(Debug, Clone)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
}

#[derive(Debug, Clone)]
pub struct PromptExpr {
    pub parts: Vec<PromptPart>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum PromptPart {
    Text(String),
    Interpolation(Expr),
}

#[derive(Debug, Clone)]
pub enum SchemaExpr {
    Any,
    Int,
    Float,
    Bool,
    String,
    List(Box<SchemaExpr>),
    Tuple(Vec<SchemaExpr>),
    Object(Vec<SchemaField>),
    Union(Vec<SchemaExpr>),
    Optional(Box<SchemaExpr>),
}

#[derive(Debug, Clone)]
pub struct SchemaField {
    pub name: String,
    pub schema: SchemaExpr,
}
```

## 5. Runtime Semantics for Prompt Interpolation, Tool Calls, and Annotations

`expr = $ ... $`:

- Evaluate interpolation expressions to runtime `Value`
- For non-function values, serialize interpolation to canonical JSON text
- For dataframe values, serialize interpolation to a bounded dataframe context JSON block (`shape`, `columns`, sampled rows, numeric profile, truncation metadata)
- For function values, auto-register a model tool and insert its callable name into prompt text
  - Bare identifier interpolation (`{my_func}`) uses `my_func` as tool name
  - Other function-valued expressions use generated names (`tool_1`, `tool_2`, ...)
- Build provider tool definitions from function signatures:
  - Parameter schema from function parameter annotations when present
  - Missing parameter annotations default to `any`
  - Return schema is advisory; assignment annotation remains authoritative
- Call provider with prompt + discovered tools
- Run tool-call loop:
  1. Model emits tool call(s)
  2. Runtime validates tool args against parameter schema
  3. Runtime executes the user function in interpreter
  4. Runtime appends tool result message and continues
  5. Stop when model returns final non-tool output or max rounds exceeded

`name: schema = expr`:

- Evaluate `expr` to a runtime value
- Validate/coerce that value against `schema`
- On mismatch, raise runtime error with assignment span + schema mismatch details

When combining both forms:

- Prompt result text is parsed as JSON only when assignment schema exists
- Parsed value is validated against assignment schema
- On parse/validation failure, runtime can retry with one repair prompt

## 6. v0 Value Model

```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Object(std::collections::BTreeMap<String, Value>),
    DataFrame(PolarsDataFrameHandle),
    Function(FunctionValue),
    Nil,
}
```

Notes:

- `+` allowed for numeric add and string concat only
- Truthiness: `false` and `nil` are falsey; everything else truthy

## 7. v0 Standard Library (Implemented)

Current builtins:

- `upper(string) -> string`
- `print(any) -> nil` (prints to stdout)
- `len(string|list|tuple|object|dataframe) -> int`
- `type(any) -> string`
- `read(path: string) -> dataframe` (CSV)
- `shape(df: dataframe) -> (int, int)`
- `columns(df: dataframe) -> [string]`
- `head(df: dataframe) -> [object]`
- `select(df: dataframe, cols: [string]) -> dataframe`
- `mean(df: dataframe, column: string) -> float`
- `sum(df: dataframe, column: string) -> float`
- `min(df: dataframe, column: string) -> float`
- `max(df: dataframe, column: string) -> float`

Builtins are normal function values at runtime, so they can be called directly and can be interpolated in prompts as tools.

## 8. CLI Contract for Milestone 1

Keep secrets out of compile artifacts:

- `orangensaft check file.saft`
- `orangensaft check file.saft --autofmt`
- `orangensaft run file.saft --api-key-env OPENAI_API_KEY --model gpt-4.1-mini --temperature 0`
- `orangensaft run file.saft --max-tool-rounds 8 --max-tool-calls 32`
- `orangensaft run file.saft --autofmt`
- `orangensaft fmt file.saft --check`
- `orangensaft fmt file.saft --write`

If you still want compile-time key injection, treat it as a temporary dev flag only.

## 9. Suggested Parser/Checker Phases

1. Lex (`NEWLINE/INDENT/DEDENT`, prompt mode, comments)
2. Parse to AST
3. Name resolution (undefined variable/function checks)
4. Lightweight semantic checks
   - valid tuple index usage
   - prompt interpolation expressions are syntactically valid
   - assignment annotation syntax validity
5. Interpret runtime

## 10. Example of True Function Calling

```saft
verbs = ["build", "test", "ship"]

f my_func(verb: string) -> string:
    ret upper(verb) + "_random_suffix"

result: [string] = $
    Call {my_func} once for each verb in {verbs}.
    Return JSON array of strings only.
$
```

In this form, interpolating `{my_func}` auto-exposes `my_func` as a callable tool.
