# Architecture -- shed

This document describes the conceptual architecture of shed at the component
and data-flow level. It does not prescribe concrete code. See the source
files for implementation detail and docs/AGENT.md for coding rules.

---

## Purpose Recap

shed is a source-to-source compiler: it reads one .shed file and writes
valid shell script for a chosen target dialect. The input language is a
small, line-oriented DSL; the output is idiomatic shell code meant to be
eval-ed or source-d at shell startup.

Design principle: maximum simplicity at every layer.

  - one input format, many output dialects
  - one binary, no runtime, no installer, no config files
  - one straight pipe: source text -> tokens -> AST -> emitted text

---
## High-Level Data Flow

    stdin / file
         |
         v
    [ Reader ] --> [ Parser ] --> [ Emitter ] --> stdout
     raw String      Vec      String
                    (the AST)

Reader  -- trivial file or stdin read. No buffering; .shed files are tiny.
Parser  -- converts raw text into a typed AST. Pure: no I/O, no global state.
Emitter -- converts the AST into a target-language string. Pure: no I/O.

Three stages. No optimisation pass, no symbol table, no linker.

---

## Component Map

    src/
      main.rs      CLI: parse args, call reader, call parser,
                   dispatch to emitter, print result.
      ast.rs       Shared data types: Node, IfNode, Cond.
      parser.rs    Converts text -> Vec.
                   Line-oriented tokenisation then recursive-descent.
      emit.rs      Emitter trait + sub-module declarations.
      emit/
        bash.rs    bash + zsh backend (POSIX-compatible)
        fish.rs    fish backend
        pwsh.rs    PowerShell backend

---
## The AST

The AST is deliberately flat and concrete. There is no generic expression
node, no precedence hierarchy, no optional field hiding ambiguity.

    Node
      Set    { key, val }     -- export an environment variable
      Path   { dir, prepend } -- prepend or append to PATH
      call { cmd, args }    -- eval-style initialiser
      If(IfNode)
        cond  : Cond
        body  : Vec     -- then-branch
        elifs : Vec -- elif branches
        else_ : Vec     -- empty = absent

    Cond
      Have(cmd)    -- command must exist on PATH
      Os(name)     -- darwin | linux | windows
      Shell(name)  -- bash | zsh | fish | pwsh

Nesting is supported through IfNode body / elifs / else_.
The recursive block() call handles arbitrary depth naturally.

---

## The Parser

Two micro-phases:

1. Pre-tokenisation -- split source into lines, strip comments, trim, split
   on whitespace into Vec. Drop blank lines. No raw bytes after this.

2. Recursive descent -- block(stops) consumes token-lines until a stop
   keyword or EOF. parse_if handles branching. parse_cond maps two-token
   syntax to a Cond variant.

Error messages carry enough context for the user to self-correct without
reading source code.

---

## The Emitter Trait

    trait Emitter
      name()        -> &str       -- shell name; used for {shell} substitution
      emit_nodes()  -> Vec -- only required implementation
      render()      -> String     -- joins lines with newlines (default)
      indent()      -> String     -- prepends N*2 spaces (default)

Each backend implements emit_nodes as a flat match over Node variants.
Conditional code generation lives in a private emit_if method on the struct.
Shell-specific idioms are encapsulated entirely inside the backend.

No associated types, no generics, no lifetime parameters on the trait.

---
## Extension Points

### New shell dialect

1. Create `src/emit/<shell>.rs`.
2. Declare `pub mod <shell>;` in `src/emit.rs`.
3. Implement `Emitter` for the new struct.
4. Add a match arm in `main.rs`.

No other files need to change.

### New statement keyword

1. Add a variant to `Node` (and `Cond` if it is a condition) in `src/ast.rs`.
2. Add a parse arm in `Parser::block()` (or `parse_cond`) in `src/parser.rs`.
3. Add an emit arm in every emitter's `node()` / `cond()` method.

The compiler enforces exhaustiveness: no variant can be silently skipped.

### New condition type

1. Add a `Cond` variant in `src/ast.rs`.
2. Add a parse arm in `Parser::parse_cond()` in `src/parser.rs`.
3. Add an emit arm in every backend `cond()` method.

---

## Module Layout

```
src/
  main.rs       — CLI entry point only; no business logic
  ast.rs        — data types (Node, IfNode, Cond, ParseError)
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

## Compound Conditions (`and`, `or`, `not`) -- Operator Precedence

This section documents the implemented syntax and precedence rules for
compound conditions in the shed DSL.

### Syntax

```sh
# not (prefix) -- negates one leaf; highest precedence
if not have cargo
  set CARGO_ABSENT 1
end

# and (infix) -- medium precedence
if have cargo and os linux
  path+ ~/.cargo/bin
end

# or (infix) -- lowest precedence
if os darwin or os linux
  set POSIX 1
end

# mixed: not binds tighter than and, and tighter than or
if not have cargo and os linux
  # parsed as: (not have cargo) and (os linux)
end

if have cargo or not shell fish
  # parsed as: (have cargo) or (not (shell fish))
end
```

### Precedence table (highest to lowest)

| Level | Operator | Arity | Associates |
|-------|----------|-------|------------|
| 1 | `not` | prefix | right |
| 2 | `and` | infix | left |
| 3 | `or` | infix | left |

There are **no parentheses** in the DSL. Conditions requiring grouping beyond
what precedence provides must use nested `if` blocks.

### Grammar (EBNF)

```
cond     = or_expr
or_expr  = and_expr ( "or"  and_expr )*
and_expr = not_expr ( "and" not_expr )*
not_expr = "not" not_expr | leaf
leaf     = ( "have" | "os" | "shell" ) value
```

Because the DSL is line/token-split, each leaf occupies exactly two tokens.
The parser locates `or` / `and` by scanning the flat token slice for those
keywords at positions where a boundary between two leaves can exist (i.e.
positions 2, 5, 8... for a sequence of two-token leaves, but since `not`
consumes a prefix the scan uses a right-to-left search for the lowest-
precedence operator).

### Parser implementation (`src/parser.rs`)

`parse_cond(ln, toks)` is the entry point. It calls three helpers in order
of descending precedence:

```
parse_or(ln, toks)
  scan RIGHT-TO-LEFT for the last "or" where index >= 2 and index+1 < len.
  split at that position:
    left  = parse_or(toks[..pos])    -- recurse left for left-associativity
    right = parse_and(toks[pos+1..]) -- right operand is and-level
  if none found: delegate to parse_and.

parse_and(ln, toks)
  scan RIGHT-TO-LEFT for the last "and" where index >= 2 and index+1 < len.
  split:
    left  = parse_and(toks[..pos])   -- recurse left for left-associativity
    right = parse_not(toks[pos+1..]) -- right operand is not-level
  if none found: delegate to parse_not.

parse_not(ln, toks)
  if toks[0] == "not": Not(Box::new(parse_not(toks[1..])))
  else: parse_leaf(toks)

parse_leaf(ln, toks)
  expects exactly [type, value]; type in {have, os, shell}
```

Right-to-left scanning for the LAST operator, with left-recursive descent,
achieves left-associativity:
  `a and b and c` -- last `and` splits right side off
  → And(parse_and("a and b"), parse_not("c"))
  → And(And(a,b), c)     -- left-associative

### Emitter output

| Shell | `Not(c)` | `And(a, b)` | `Or(a, b)` |
|-------|----------|-------------|------------|
| bash/zsh | `! <c>` | `<a> && <b>` | `<a> \|\| <b>` |
| fish | `not <c>` | `<a>; and <b>` | `<a>; or <b>` |
| pwsh | `(-not (<c>))` | `(<a>) -and (<b>)` | `(<a>) -or (<b>)` |

### What to avoid

- **Parentheses in the DSL** — the parser is line/token-split; a paren grammar
  would need a character scanner.
- **Implicit precedence surprises** -- always document that `not` binds tighter
  than `and`, which binds tighter than `or`. Add a comment in source if unclear.

### Implementation files

All changes are confined to `src/parser.rs`. The `Cond` AST in `src/ast.rs`
and all emitters are unchanged.

---

## Semantic Pruning Pass

The pruning pass runs after parsing and before emitting. It is pure: no I/O,
no global state; the input AST and target shell name are the only inputs.

### Purpose

Conditional blocks guarded by `Cond::Shell` or `Cond::Os` are statically
known to be always-true or always-false for a specific compilation target.
Emitting dead branches wastes output lines and can confuse shell linters.

Examples (target = bash):

```
if shell fish          →  entire block unreachable; prune to nothing
  ...
end

if shell bash          →  guard always true; inline the body directly
  set EDITOR nvim
end

if os darwin           →  condition unknown at compile time; kept as-is
  ...
end
```

`Cond::Have` is a runtime check (command existence may differ per machine)
and is never pruned.

`Cond::Os` is known only when the shed file is compiled for a specific
machine. Because `shed` is a source-to-source compiler targeting multiple
hosts from one file, `os` conditions are left untouched.

### Pruning rules for `Cond::Shell(name)`

| Condition | Target shell | Result |
|-----------|-------------|--------|
| `Shell(s)` where `s == target` | any | always-true → inline body |
| `Shell(s)` where `s != target` | any | always-false → drop branch |

### Pruning rules for compound conditions

The pass reduces compound `Cond` nodes before deciding branch fate:

- `Not(always-true)`  → always-false
- `Not(always-false)` → always-true
- `And(always-false, _)` or `And(_, always-false)` → always-false (short-circuit)
- `And(always-true,  c)` → reduce to `c`
- `Or(always-true,  _)` or `Or(_, always-true)`  → always-true (short-circuit)
- `Or(always-false, c)` → reduce to `c`

A `Cond` that cannot be fully resolved stays as a `Cond` node and is emitted
normally.

### Branch outcome

After condition evaluation:

- **Always-true body** — replace `Node::If(inode)` with the body nodes
  inlined into the parent list. `elif` / `else` branches are dropped.
- **Always-false body** — check `elifs` in order; the first elif whose
  condition is also always-true is inlined. If no elif matches, the `else_`
  block (if present) is inlined. If nothing matches the node is dropped.
- **Unknown** — the `IfNode` is kept but its `body`, `elifs`, and `else_`
  are recursively pruned.

### Implementation location

```
src/
  prune.rs    — prune_nodes(nodes, shell) -> Vec<Node>
                prune_cond(cond, shell)   -> CondResult
```

`CondResult` is a local enum:

```rust
enum CondResult {
    AlwaysTrue,
    AlwaysFalse,
    Unknown(Cond),
}
```

The pass is wired into `main.rs` between `resolve_paths` and `emit`:

```
read → parse → resolve_paths → prune_nodes → emit
```

`prune_nodes` takes the target shell name as a `&str` so it remains pure
and testable without constructing an `Emitter`.

No other files need to change.

---

## What This Architecture Deliberately Omits

  Symbol table / var resolution  -- DSL has no user-defined variables
  Type system                    -- all values are strings; no type errors
  General optimisation pass      -- output correctness matters more than brevity
  Plugin / dynamic loading       -- shell set is closed; static dispatch wins
  Runtime configuration file     -- all behaviour driven by the .shed source
  IR between parser and emitter  -- the AST is the IR; no lowering needed

---

## Proposed Improvement Plan

Ordered highest to lowest impact. All items are evolutionary; none requires
changing the three-stage architecture.

---

### P1 -- Line-number tracking in error messages

Problem.
  Errors say what went wrong but not where. A 100-line env.shed cannot be
  searched without counting lines manually.

Approach.
  Retain the original 1-based line number alongside each token-line during
  pre-tokenisation, as (usize, Vec). Thread it into every error
  string from block(), parse_cond(), and parse_if().

Change surface.  parser.rs only. The AST and all emitters are unaffected.

Result.  shed: line 42: unknown keyword "sett"

---

### P2 -- Variable interpolation in values

Problem.
  `set FOO $HOME/tool` emits the literal string $HOME/tool. Bash expands it
  at eval time; fish and pwsh do not. Behaviour is silent and shell-dependent.

Approach.
  Represent a value as Vec where ValuePart is Literal(String) or
  Var(String). Parser splits $VAR tokens in value positions. Each emitter
  renders Var(HOME) as $HOME (bash/zsh/fish) or $env:HOME (pwsh).

Change surface.  ast.rs (new ValuePart type), parser.rs, all four emitters.

---

### P3 -- alias keyword

Problem.
  Shell aliases are the second most common env-file entry after exports.
  Users currently need call or separate per-shell files.

Approach.
  Add Node::Alias { name: String, body: String }. Emitters render:
    bash / zsh   alias name='body'
    fish         alias name body
    pwsh         Set-Alias name body

Change surface.  ast.rs, parser.rs, all four emitters. Follows the existing
                 extension pattern exactly.

---

### P4 -- Structured error type

Problem.
  All errors are String. Testing requires fragile string matching.
  A future library surface cannot expose typed errors to callers.

Approach.
  Hand-write a minimal Error enum (no external crates) with variants such as
  UnknownKeyword, UnterminatedBlock, BadUsage. Implement std::fmt::Display.
  Change Result to Result in the parser.

Change surface.  New src/error.rs; parser.rs; main.rs. Emitters unaffected.

---

### P5 -- zsh as a first-class backend

Problem.
  zsh is emitted by BashEmitter with shell_name set to "zsh". This works via
  POSIX compatibility but precludes zsh-specific output and makes
  Cond::Shell("zsh") indistinguishable from Cond::Shell("bash") inside the emitter.

Approach.
  Extract a ZshEmitter that composes BashEmitter for shared logic and overrides
  only the parts that diverge (shell variable detection, typeset idioms).
  Alternatively, add a PosixDialect enum parameter to BashEmitter.

Change surface.  src/emit/bash.rs (refactor), optional new src/emit/zsh.rs,
                 src/emit.rs, src/main.rs (match arm already present).
