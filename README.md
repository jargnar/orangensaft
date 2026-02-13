# 🍊Orangensaft

Orangensaft is an experimantal, new age, post-AI programming language. _This is a hobby project. It's a toy language. I'm just starting this thing. Do not use it. It's only for me._

Ok. This is like a mini python where prompts are deeply part of the language. You can fully intermix deterministic scripting with probabilistic LLM calls. LLM calls are a fundamental part of the language runtime.

This is a valid code:

```
// example.saft

people = ["alice", "bob", "charlie", "mr. karabalabaloofal"]

longest_name: string = $
  who has the longest name in {people}
$

assert longest_name == people[3] // yup true
```

When anything is enclosed between `$ .. $`, then the language runtime:

- calls an LLM
- enforces the declared type
- stores the result into the specified variable

Below is the same code, but written normally and deterministically:

```
// old.saft

people = ["alice", "bob", "charlie", "mr. karabalabaloofal"]

longest_name = people[0]
for p in people:
  if len(p) > len(longest_name):
    longest_name = p

print(longest_name)
```

You can choose what level of probabilistic or deterministic code you write. WCGW?.

To run a `saft` program, clone the repo and run it like this:

```sh
% cargo run -- run examples/11_simple_array_op_2.saft \
  --provider openrouter \
  --api-key-env OPENROUTER_API_KEY \
  --model openai/gpt-4o-mini \
  --temperature 0 \
  --max-tool-rounds 8 \
  --max-tool-calls 32
```

### A few more examples

```
verbs = ["build", "test", "ship"]

upper: [string] = $
    convert each item in {verbs} to uppercase
$

assert upper[0] == "BUILD"
assert upper[1] == "TEST"
assert upper[2] == "SHIP"
```

or even sth like this

```
people = ["alice", "bob", "charlie"]

f greet(x, y):
    ret x + " says hi to " + y

z: string = $
    hey it seems among {people}, bob
    wants to talk to alice
    can you {greet} them
$

assert z == "bob says hi to alice"
```

This is true func calling at runtime.

You can also load CSVs with Polars-backed dataframes:

```saft
df = read("examples/data/team_stats.csv")

winner = $
    which column from {df} has highest average
$
```

When a dataframe is interpolated in a prompt (`{df}`), runtime injects a structured JSON context block instead of raw full-table dumps. That block includes:

- `shape` (`rows`, `columns`)
- column names + dtypes
- `sample_rows` (bounded sample)
- `numeric_profile` (`mean`, `min`, `max` per numeric column, bounded)
- truncation metadata so models know context was summarized

This keeps prompts token-efficient while still giving the model enough tabular signal for questions like "highest average column". For exact numeric answers, deterministic stdlib functions (`mean`, `sum`, etc.) are still available.


See all other examples in the examples folder.

## Tiny stdlib

Current builtin functions:

- `upper(string) -> string`
- `print(any) -> nil`
- `len(string|list|tuple|object|dataframe) -> int`
- `type(any) -> string`
- `read(path: string) -> dataframe` (CSV)
- `shape(df: dataframe) -> (int, int)` (`rows, columns`)
- `columns(df: dataframe) -> [string]`
- `head(df: dataframe) -> [object]` (first 5 rows)
- `select(df: dataframe, cols: [string]) -> dataframe`
- `mean(df: dataframe, column: string) -> float`
- `sum(df: dataframe, column: string) -> float`
- `min(df: dataframe, column: string) -> float`
- `max(df: dataframe, column: string) -> float`


## Build notes

You can also use shorthand (no `run` subcommand):

```sh
% cargo run -- examples/11_simple_array_op_2.saft --provider openrouter
```

You can auto-format in-memory before running/checking:

```sh
% cargo run -- run examples/14_polars_agentic_scouting_report.saft --autofmt
% cargo run -- check examples/14_polars_agentic_scouting_report.saft --autofmt
```

And format files directly:

```sh
% cargo run -- fmt examples/14_polars_agentic_scouting_report.saft --check
% cargo run -- fmt examples/14_polars_agentic_scouting_report.saft --write
```

Note: formatter output is AST-based and can rewrite layout aggressively.

If you want plain `orangensaft ...` commands:

```sh
% cargo install --path .
```

Then set defaults once in your shell profile:

```sh
export ORANGENSAFT_PROVIDER=openrouter
export ORANGENSAFT_API_KEY_ENV=OPENROUTER_API_KEY
export ORANGENSAFT_MODEL=openai/gpt-4o-mini
export ORANGENSAFT_TEMPERATURE=0
export ORANGENSAFT_MAX_TOOL_ROUNDS=8
export ORANGENSAFT_MAX_TOOL_CALLS=32
```

After that, this works:

```sh
% orangensaft examples/11_simple_array_op_2.saft
```

## AI Agent entrypoint

For AI-assisted maintenance and development in this repo:

- `AGENTS.md` is the canonical, complete agent guide.
