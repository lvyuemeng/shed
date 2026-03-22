# AGENT — Rust Coding Spec & Philosophy for `shed`

This document is written for coding agents (and humans) contributing to `shed`.
It describes the Rust idioms, constraints, and design philosophy that govern
every change to this codebase. Read it before touching a single line.

---

## Core Philosophy

> **Native. Simple. Honest.**

`shed` is a single-binary, zero-dependency compiler. It should compile fast,
run fast, and carry no weight the user did not ask for. Every decision is
measured against these three words.

- **Native** — use only `std`. No `clap`, no `serde`, no `anyhow`, no `thiserror`.
  If the standard library can do it, the standard library does it.
- **Simple** — if a data structure, a function, or a module can be removed
  without losing correctness, remove it.
- **Honest** — the code should say exactly what it means. No clever macros
  wrapping simple logic. No abstractions hiding a three-line match arm.

---

## Dependency Policy

`Cargo.toml` has **no `[dependencies]`** section. This is not an oversight.

| Situation | Rule |
|-----------|------|
| Need argument parsing | Write it by hand — `std::env::args()` is enough |
| Need error handling | Use `Result<T, String>` and `format!` |
| Need file I/O | `std::fs` and `std::io` |
| Need a serialisation format | Reconsider the design first |
| Genuinely need an external crate | Open a discussion; the bar is very high |

---

## Error Handling

- Return `Result<T, String>` for fallible operations.
- Errors are plain English sentences, lowercase, no trailing period.
  Example: `"unterminated if-block (missing 'end')"` — not `"Error: Unterminated."`
- Errors are reported with `eprintln!("shed: {}", e)` and `process::exit(1)` in
  `main`. No panics outside of internal logic bugs.
- Do **not** introduce `anyhow`, `thiserror`, or custom error enums unless the
  error type must carry structured data that a `String` genuinely cannot express.

---

## Types & Data

- Keep the AST **flat**. `Node` is an enum of concrete statements; there are no
  expression trees, no operator precedence, no ambiguous grammar.
- Prefer plain `struct` and `enum` over trait objects wherever the set of
  variants is closed and known at compile time.
- Do **not** box nodes unless recursion demands it.
- `String` owns data in the AST. `&str` is fine for transient slices during
  parsing. Do not over-engineer lifetimes.

---

## Parser Style

- The parser is a **line-oriented, token-split** recursive-descent parser.
  Each line is split on whitespace before parsing begins; there is no character-
  level scanner.
- `Parser::block()` is the central loop. It consumes lines until it hits a stop
  keyword or EOF.
- Error messages must include enough context for the user to fix the problem
  without reading source code.
- Do **not** add backtracking, lookahead beyond `peek()`, or any form of
  speculative parse.

---

## Emitter Style

- Each shell backend is a struct in `src/emit/<shell>.rs` that implements the
  `Emitter` trait.
- The trait surface is intentionally small: `name()`, `emit_nodes()`. The
  default `render()` and `indent()` helpers live on the trait and should not be
  overridden without good reason.
- Every emitter method is **pure** — no side-effects, no global state, no I/O.
- Shell-specific quirks belong inside the emitter, not in the AST.

---

## Module Layout

```
src/
  main.rs       — CLI entry point only; no business logic
  ast.rs        — data types (Node, IfNode, Cond)
  parser.rs     — source → AST
  emit.rs       — Emitter trait + sub-module declarations
  emit/
    bash.rs     — bash / zsh backend
    fish.rs     — fish backend
    pwsh.rs     — PowerShell backend
```

Do not let `main.rs` grow. If logic is needed beyond dispatching to an emitter,
it belongs in a dedicated module.

---

## Naming Conventions

| What | Convention | Example |
|------|------------|---------|
| Types / traits | `UpperCamelCase` | `BashEmitter`, `IfNode` |
| Functions / methods | `snake_case` | `emit_nodes`, `parse_cond` |
| Local variables | `snake_case`, short | `d` for depth, `t` for token-line |
| Constants | `SCREAMING_SNAKE` | `USAGE` |
| Module files | `snake_case` | `bash.rs`, `emit.rs` |

Single-letter variables are acceptable **only** inside tight, obvious loops
(`d` for indent depth, `n` for node, `c` for condition, `b` for body).

---

## Formatting & Style

- Run `cargo fmt` before every commit.
- Run `cargo clippy -- -D warnings` and fix every diagnostic.
- Align struct fields and match arms with spaces when it improves readability
  (the existing code does this; follow the pattern).
- Prefer `vec![…]` literals over repeated `push` calls when the contents are
  known up front.
- Prefer iterator chains (`flat_map`, `map`, `collect`) over explicit `for`
  loops when the transform is simple and the intent is clear.

---

## Adding a Shell

1. Create `src/emit/<shell>.rs`.
2. Declare `pub mod <shell>;` in `src/emit.rs`.
3. Implement `Emitter` for the new struct.
4. Add a match arm in `main.rs`.

No other files need to change.

## Adding a Keyword

1. Add a variant to `Node` (and `Cond` if it is a condition) in `src/ast.rs`.
2. Add a parse arm in `Parser::block()` (or `parse_cond`) in `src/parser.rs`.
3. Add an emit arm in every emitter's `node()` / `cond()` method.

The compiler will point out every missing arm.

---

## What to Avoid

- **Macros** for things that a function or a match arm handles cleanly.
- **Trait objects** (`Box<dyn Emitter>`) when a concrete enum dispatch suffices.
- **Lifetimes** on public API types — the AST owns its strings.
- **Premature abstraction** — do not create a helper until it is used in three
  or more places.
- **Configuration structs** for single-value options — pass the value directly.
- **`unwrap()` / `expect()`** in paths that can be reached by bad user input.
