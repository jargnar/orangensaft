# 🍊Orangensaft

Orangensaft is a new age, post AI programming language.

This is a hobby project.

This is like a mini python, where prompts are deeply part of the language. You can fully intermix deterministic scripting with probabilistic LLM calls. LLM calls are a deep part of the language runtime.

This is a valid code:

```
// example.saft

people = ["alice", "bob", "charlie", "mr. karabalabaloofal"]

z: string = $
    who has the longest name in {people}
$

// guess what happened above?
// when you enclosed something in $ .. $ and assign it to a var
// the language runtime actually evaluates it by calling an LLM
// and stores response in the assigned variable

// so the below will actually mostly be true
// (unless you're calling like a stupid model)

assert z == people[3]
```

To run a `saft` program, for now clone the repo and run it like this

```sh
% cargo run -- run examples/11_simple_array_op_2.saft \
  --provider openrouter \
  --api-key-env OPENROUTER_API_KEY \
  --model openai/gpt-4o-mini \
  --temperature 0 \
  --max-tool-rounds 8 \
  --max-tool-calls 32
```

A few more quick examples below.

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


See all other examples in the examples folder.
