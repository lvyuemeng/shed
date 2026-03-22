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
| Need error handling | Use `Result<T, ParseError>` or `Result<T, String>` as appropriate |
| Need file I/O | `std::fs` and `std::io` |
| Need a serialisation format | Reconsider the design first |
| Genuinely need an external crate | Open a discussion; the bar is very high |

---

## Error Handling

### Error type

Parser errors carry structured data. Use `ParseError` — a plain struct, no
external crate, no trait magic:

```rust
// in src/ast.rs (or a separate src/error.rs if the type list grows)
pub struct ParseError {
    pub line: usize,   // 1-based source line; 0 means EOF / not applicable
    pub msg:  String,  // plain English, lowercase, no trailing period
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.msg)
        } else {
            write!(f, "line {}: {}", self.line, self.msg)
        }
    }
}
```

- `ParseError` is the only custom error struct in the codebase. Everything else
  uses `Result<T, String>` with `format!`.
- The `msg` field is plain English, lowercase, no trailing period.
  Example: `"unterminated if-block (missing 'end')"` — not `"Error: Unterminated."`
- Errors are reported in `main` with `eprintln!("shed: {}", e)` and
  `process::exit(1)`. No panics outside of internal logic bugs.
- Do **not** introduce `anyhow`, `thiserror`, or additional error enums unless
  the type must carry structured data that `ParseError` genuinely cannot express.

---

## Safety Rules

These rules prevent panics on bad user input.

### No direct indexing without a proven bounds check

```rust
// Bad — panics on an empty token list
let kw = toks[0].as_str();

// Good — propagate gracefully
let kw = toks.first()
    .ok_or_else(|| ParseError { line, msg: "empty line".into() })?;
```

Use `.get(i)`, `.first()`, `.last()`, or a preceding `match` / `if let`
instead of `collection[i]` when the index is not provably in-bounds from the
same expression's context. If a check was done one or two lines above, add a
short comment explaining why the index is safe.

### No `unwrap()` or `expect()` on user-input paths

`unwrap()` / `expect()` are acceptable **only** in:

- test code,
- provably infallible paths — add a comment explaining why.

Everywhere else, propagate with `?` or convert with `.ok_or_else(...)`.

### Prefer `Cow<'_, str>` for zero-copy strings

When a function either returns a borrowed slice unchanged **or** allocates a
modified copy, prefer `std::borrow::Cow<'_, str>` over always allocating a new
`String`. The `indent()` helper in `Emitter` is the canonical example: at
depth 0 it returns the input untouched; only at depth > 0 does it allocate.

---

## Code Style

### Prefer shallow nesting — two levels maximum per function

Deep nesting hides control flow and makes errors hard to locate. Target at most
two levels of indented blocks per function body.

```rust
// Bad — three levels before the real work
loop {
    match peek() {
        Some(kw) => {
            match kw {
                "elif" => { ... }
            }
        }
    }
}

// Good — flat while-let, single match level
while let Some((ln, kw)) = self.peek_kw() {
    match kw.as_str() {
        "elif" => { ... }
        "end"  => { self.pos += 1; break; }
        kw     => return Err(ParseError { line: ln, msg: format!("unexpected {:?}", kw) }),
    }
}
```

Extract sub-logic into a named helper rather than adding a third nesting level.

### Prefer data-pipeline / iterator style over imperative loops

When a transformation maps a collection to another collection, prefer an
iterator chain. Reserve `for` loops for cases where mutation or early-exit
cannot be expressed clearly as a chain.

```rust
// Acceptable but verbose
let mut out = Vec::new();
for n in nodes {
    out.push(transform(n));
}

// Preferred
let out: Vec<_> = nodes.into_iter().map(transform).collect();
```

Iterator chains compose naturally with `flat_map`, `filter_map`, and `chain`.

### Match exhaustively; avoid silent catch-all arms

A `_ => {}` arm on a closed enum silences compiler warnings when a new variant
is added. Match every known variant explicitly, or use
`_ => unreachable!("...")` with a comment if an arm is structurally impossible.

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
- `Parser` stores `Vec<(usize, Vec<String>)>` — the `usize` is the 1-based
  original source line number, preserved through blank-line and comment
  filtering so every error message is actionable.
- `Parser::block()` is the central loop. It consumes lines until it hits a stop
  keyword or EOF.
- Error messages must include a line number and enough context for the user to
  fix the problem without reading source code. Return `ParseError`, not a bare
  `String`.
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

## Naming Conventions

| What | Convention | Example |
|------|------------|---------|
| Types / traits | `UpperCamelCase` | `BashEmitter`, `IfNode`, `ParseError` |
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

## What to Avoid

- **Macros** for things that a function or a match arm handles cleanly.
- **Trait objects** (`Box<dyn Emitter>`) when a concrete enum dispatch suffices.
- **Lifetimes** on public API types — the AST owns its strings.
- **Premature abstraction** — do not create a helper until it is used in three
  or more places.
- **Configuration structs** for single-value options — pass the value directly.
- **`unwrap()` / `expect()`** in paths that can be reached by bad user input.
- **Direct indexing** (`slice[i]`) without a preceding bounds check or a
  comment proving the index is safe.
- **Bare `String` errors from the parser** — use `ParseError` so callers have
  the line number without re-parsing the message.
- **Deep nesting** — more than two levels of indented blocks in a single
  function body. Extract a helper instead.
