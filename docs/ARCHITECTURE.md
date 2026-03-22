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
      Inject { cmd, args }    -- eval-style initialiser
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

1. Add src/emit/<shell>.rs implementing Emitter.
2. Declare `pub mod <shell>;` in src/emit.rs.
3. Add a match arm in src/main.rs.

### New statement keyword

1. Add a Node variant in src/ast.rs.
2. Add a parse arm in Parser::block() in src/parser.rs.
3. Add an emit arm in every backend node() method.

The compiler enforces exhaustiveness: no variant can be silently skipped.

### New condition type

1. Add a Cond variant in src/ast.rs.
2. Add a parse arm in Parser::parse_cond() in src/parser.rs.
3. Add an emit arm in every backend cond() method.

---

## What This Architecture Deliberately Omits

  Symbol table / var resolution  -- DSL has no user-defined variables
  Type system                    -- all values are strings; no type errors
  Optimisation pass              -- output correctness matters more than brevity
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
  Users currently need inject or separate per-shell files.

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
