# Orangensaft Design Goals (v0)

## Core Principles

1. Ergonomics first
   - Common workflows should read like plain intent, not configuration.
   - Reuse one syntax where possible (for example, prompt interpolation handles both values and function tools).

2. Minimal syntax noise
   - Prefer fewer brackets, keywords, and mode switches.
   - New syntax must justify itself by removing repeated boilerplate.

3. Readability over cleverness
   - A script should be understandable top-to-bottom by a new reader.
   - Prompt blocks should look like natural language with lightweight interpolation.

4. Deterministic core, probabilistic edge
   - Core language semantics are deterministic and testable.
   - LLM behavior is isolated to explicit `$ ... $` expressions.

5. Progressive structure, not strict typing
   - Keep the language dynamic by default.
   - Add optional schema annotations for runtime checks and safer LLM outputs.

6. Built-in true function calling
   - Interpolating a function (`{my_func}`) should expose it as a callable tool.
   - Tool execution must run through the interpreter, not simulated by model text.

7. Great failures
   - Parse/runtime/schema errors must include precise spans and clear messages.
   - LLM parse/validation failures should explain what failed and where.

8. Small, evolvable surface
   - v0 should stay small enough to implement and reason about quickly.
   - Add features only when they compose cleanly with existing semantics.

## Non-Goals for v0

- Full static type system
- Security-hardening for adversarial multi-tenant execution
- Macro/meta-programming systems
- Provider-agnostic perfection on day one

## Feature Acceptance Rubric

Before adding syntax or runtime behavior, ask:

1. Does this reduce user ceremony in common scripts?
2. Does this keep scripts readable at a glance?
3. Can this produce clear errors when it fails?
4. Does this compose with existing grammar and runtime rules?
5. Can we explain it in two to three sentences in the docs?

If most answers are "no", do not add the feature to v0.
