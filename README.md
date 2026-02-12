# 🍊Orangensaft

Orangensaft is a new age, post AI programming language.

This is a hobby project.

This is like a mini python, where prompts are deeply part of the language. You can fully intermix deterministic scripting with probabilistic LLM calls.

LLM calls are a deep part of the language runtime.

This is a valid code:

```
people = ["alice", "bob", "charlie", "mr. karabalabaloofal"]

z: string = $
    who has the longest name in {people}
$

assert z == people[3]
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
