//! Integration tests — full source → emitted-text round-trips.
//!
//! Philosophy (per AGENT.md):
//!   - Test important, integrated behaviour: parse + emit together.
//!   - One test per interesting scenario, not one test per function.
//!   - Inline emitter unit-tests (cond, node, indent) live in the emitter
//!     modules themselves and are not duplicated here.

use crate::{
    emit::{Emitter, bash::BashEmitter, fish::FishEmitter, pwsh::PwshEmitter},
    parser::Parser,
    prune::prune_nodes,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn bash(src: &str) -> String {
    let ast = prune_nodes(Parser::new(src).parse().unwrap(), "bash");
    BashEmitter::new("bash").render(&ast)
}
fn zsh(src: &str) -> String {
    let ast = prune_nodes(Parser::new(src).parse().unwrap(), "zsh");
    BashEmitter::new("zsh").render(&ast)
}
fn fish(src: &str) -> String {
    let ast = prune_nodes(Parser::new(src).parse().unwrap(), "fish");
    FishEmitter.render(&ast)
}
fn pwsh(src: &str) -> String {
    let ast = prune_nodes(Parser::new(src).parse().unwrap(), "pwsh");
    PwshEmitter.render(&ast)
}

// ── set / path / call across all shells ────────────────────────────────────

/// set emits the correct shell-specific export for each target.
#[test]
fn set_all_shells() {
    assert_eq!(bash("set EDITOR nvim"), "export EDITOR=\"nvim\"");
    assert_eq!(fish("set EDITOR nvim"), "set -gx EDITOR \"nvim\"");
    assert_eq!(pwsh("set EDITOR nvim"), "$env:EDITOR = \"nvim\"");
}

/// path+ / path- generate the correct PATH mutation per shell.
#[test]
fn path_prepend_and_append() {
    assert_eq!(
        bash("path+ /usr/local/bin"),
        "export PATH=\"/usr/local/bin:$PATH\""
    );
    assert_eq!(bash("path- /opt/bin"), "export PATH=\"$PATH:/opt/bin\"");
    assert_eq!(
        fish("path+ /usr/local/bin"),
        "fish_add_path -gP \"/usr/local/bin\""
    );
    assert_eq!(fish("path- /opt/bin"), "fish_add_path -gaP \"/opt/bin\"");
    assert_eq!(
        pwsh("path+ C:\\tools"),
        "$env:PATH = \"C:\\tools;$env:PATH\""
    );
}

/// call replaces {shell} with the actual shell name.
#[test]
fn call_shell_placeholder() {
    assert_eq!(
        bash("call starship init {shell}"),
        "eval \"$(starship init bash)\""
    );
    assert_eq!(
        zsh("call starship init {shell}"),
        "eval \"$(starship init zsh)\""
    );
    assert_eq!(
        fish("call starship init {shell}"),
        "starship init fish | source"
    );
    assert_eq!(
        pwsh("call starship init {shell}"),
        "Invoke-Expression (& starship init powershell)"
    );
}

// ── conditional guards ────────────────────────────────────────────────────────

/// `if have` emits a command-existence check and the correct block structure.
#[test]
fn if_have_all_shells() {
    let src = "if have cargo\npath+ $HOME/.cargo/bin\nend";

    let b = bash(src);
    assert!(b.contains("command -v cargo"), "bash: {}", b);
    assert!(b.contains("export PATH"), "bash: {}", b);
    assert!(b.contains("fi"), "bash: {}", b);

    let f = fish(src);
    assert!(f.contains("type -q cargo"), "fish: {}", f);
    assert!(f.contains("fish_add_path"), "fish: {}", f);
    assert!(f.contains("end"), "fish: {}", f);

    let p = pwsh("if have cargo\npath+ C:\\cargo\\bin\nend");
    assert!(p.contains("Get-Command cargo"), "pwsh: {}", p);
    assert!(p.contains("$env:PATH"), "pwsh: {}", p);
}

/// `if os` with elif emits the right uname / platform checks.
#[test]
fn if_os_with_elif() {
    let src = "if os darwin\nset BROWSER open\nelif os linux\nset BROWSER xdg-open\nend";

    let b = bash(src);
    assert!(b.contains("Darwin"), "bash: {}", b);
    assert!(b.contains("Linux"), "bash: {}", b);
    assert!(b.contains("elif"), "bash: {}", b);
    assert!(b.contains("fi"), "bash: {}", b);

    let f = fish("if os darwin\nset BROWSER open\nend");
    assert!(f.contains("test (uname -s) = \"Darwin\""), "fish: {}", f);

    let p = pwsh("if os windows\nset SHELL pwsh\nend");
    assert!(p.contains("$IsWindows"), "pwsh: {}", p);
}

/// `if shell` — compile-time pruning applies for self-shell.
/// When compiled for the matching shell the prune pass inlines the body;
/// when compiled for a different shell the block is dropped entirely.
#[test]
fn if_shell_self_and_cross() {
    // each shell compiled for itself: body is inlined, no if-wrapper remains
    let b = bash("if shell bash\nset X 1\nend");
    assert!(
        b.contains("export X=\""),
        "bash self: body not inlined: {}",
        b
    );
    assert!(
        !b.contains("if "),
        "bash self: unexpected if-wrapper: {}",
        b
    );

    let f = fish("if shell fish\nset X 1\nend");
    assert!(
        f.contains("set -gx X"),
        "fish self: body not inlined: {}",
        f
    );
    assert!(
        !f.contains("if "),
        "fish self: unexpected if-wrapper: {}",
        f
    );

    let p = pwsh("if shell pwsh\nset X 1\nend");
    assert!(p.contains("$env:X"), "pwsh self: body not inlined: {}", p);
    assert!(
        !p.contains("if ("),
        "pwsh self: unexpected if-wrapper: {}",
        p
    );

    // bash compiled for fish-shell: block is dropped (empty output)
    let cross = bash("if shell fish\nset X 1\nend");
    assert!(
        cross.is_empty(),
        "bash cross: expected empty, got: {}",
        cross
    );
}

/// else block is emitted correctly.
#[test]
fn if_else_structure() {
    let src = "if os darwin\nset A mac\nelse\nset A other\nend";
    let b = bash(src);
    assert!(b.contains("else"), "bash: {}", b);
    assert!(b.contains("fi"), "bash: {}", b);
    let f = fish(src);
    assert!(f.contains("else"), "fish: {}", f);
    assert!(f.contains("end"), "fish: {}", f);
}

// ── multi-node realistic source ───────────────────────────────────────────────

/// Reflects the README example: set + os guard + have guards + call.
/// Validates that all emitters produce the key fragments from a real-world input.
#[test]
fn readme_example() {
    let src = "\
set EDITOR nvim
if os darwin
  set BROWSER open
elif os linux
  set BROWSER xdg-open
end
if have cargo
  path+ $HOME/.cargo/bin
end
if have starship
  call starship init {shell}
end";

    let b = bash(src);
    assert!(b.contains("export EDITOR=\"nvim\""), "EDITOR: {}", b);
    assert!(b.contains("Darwin"), "Darwin: {}", b);
    assert!(b.contains("Linux"), "Linux: {}", b);
    assert!(b.contains("command -v cargo"), "cargo: {}", b);
    assert!(
        b.contains("eval \"$(starship init bash)\""),
        "starship: {}",
        b
    );

    let f = fish(src);
    assert!(f.contains("set -gx EDITOR"), "EDITOR: {}", f);
    assert!(f.contains("Darwin"), "Darwin: {}", f);
    assert!(f.contains("type -q cargo"), "cargo: {}", f);
    assert!(f.contains("starship init fish"), "starship: {}", f);

    let p = pwsh(src);
    assert!(p.contains("$env:EDITOR = \"nvim\""), "EDITOR: {}", p);
    assert!(p.contains("$IsMacOS"), "darwin: {}", p);
    assert!(p.contains("$IsLinux"), "linux: {}", p);
    assert!(p.contains("Get-Command cargo"), "cargo: {}", p);
    assert!(
        p.contains("Invoke-Expression (& starship init powershell)"),
        "starship: {}",
        p
    );
}

// ── parse-error quality ───────────────────────────────────────────────────────

/// Errors carry the 1-based source line and the offending token.
#[test]
fn parse_errors_carry_line_and_context() {
    let err = Parser::new("set A 1\nsett FOO bar").parse().unwrap_err();
    assert_eq!(err.line, 2, "wrong line: {}", err);
    assert!(err.msg.contains("sett"), "wrong msg: {}", err);

    // Missing set value — points to the correct line
    let err = Parser::new("set A 1\nset B 2\nset C").parse().unwrap_err();
    assert_eq!(err.line, 3, "wrong line: {}", err);

    // Unterminated if — points to the opening `if` line
    let err = Parser::new("set A 1\nif have git\nset B 2")
        .parse()
        .unwrap_err();
    assert_eq!(err.line, 2, "should point to if line: {}", err);
}

// ── compound conditions (not / and / or) ─────────────────────────────────────

/// `if not have` — prefix negation across all shells.
#[test]
fn if_not_have_all_shells() {
    let src = "if not have cargo\nset CARGO_ABSENT 1\nend";

    let b = bash(src);
    assert!(b.contains("! command -v cargo"), "bash: {}", b);
    assert!(b.contains("fi"), "bash: {}", b);

    let f = fish(src);
    assert!(f.contains("not type -q cargo"), "fish: {}", f);
    assert!(f.contains("end"), "fish: {}", f);

    let p = pwsh(src);
    assert!(p.contains("-not (Get-Command cargo"), "pwsh: {}", p);
}

/// `if have X and os Y` — infix `and` across all shells.
#[test]
fn if_and_condition_all_shells() {
    let src = "if have cargo and os linux\npath+ $HOME/.cargo/bin\nend";

    let b = bash(src);
    assert!(b.contains("command -v cargo"), "bash and lhs: {}", b);
    assert!(b.contains("&&"), "bash and op: {}", b);
    assert!(b.contains("Linux"), "bash and rhs: {}", b);

    let f = fish(src);
    assert!(f.contains("type -q cargo"), "fish and lhs: {}", f);
    assert!(f.contains("; and "), "fish and op: {}", f);
    assert!(f.contains("Linux"), "fish and rhs: {}", f);

    let p = pwsh(src);
    assert!(p.contains("Get-Command cargo"), "pwsh and lhs: {}", p);
    assert!(p.contains("-and"), "pwsh and op: {}", p);
    assert!(p.contains("$IsLinux"), "pwsh and rhs: {}", p);
}

/// `if or` — infix `or` across all shells.
#[test]
fn if_or_condition_all_shells() {
    let src = "if os darwin or os linux\nset POSIX 1\nend";

    let b = bash(src);
    assert!(b.contains("Darwin"), "bash or lhs: {}", b);
    assert!(b.contains("||"), "bash or op: {}", b);
    assert!(b.contains("Linux"), "bash or rhs: {}", b);

    let f = fish(src);
    assert!(f.contains("Darwin"), "fish or lhs: {}", f);
    assert!(f.contains("; or "), "fish or op: {}", f);
    assert!(f.contains("Linux"), "fish or rhs: {}", f);

    let p = pwsh(src);
    assert!(p.contains("$IsMacOS"), "pwsh or lhs: {}", p);
    assert!(p.contains("-or"), "pwsh or op: {}", p);
    assert!(p.contains("$IsLinux"), "pwsh or rhs: {}", p);
}

// -- semantic pruning (shell-condition folding) ----------------------------------

/// Comprehensive single-shell pruning integration test (bash).
///
/// Covers in one source blob: self-shell inline, foreign-shell drop,
/// dead-head+else, dead-head+matching-elif, not-fold, and+true, and+false,
/// or+true, and unknown guard kept as-is.
#[test]
fn prune_comprehensive_bash() {
    let src = "\
if shell bash
  set NATIVE 1
end
if shell fish
  set FISH_ONLY 1
end
if shell fish
  set A fish
else
  set A other
end
if shell fish
  set B fish
elif shell bash
  set B bash
end
if not shell fish
  set C 1
end
if shell bash and have cargo
  set D 1
end
if shell fish and have cargo
  set E 1
end
if shell bash or have cargo
  set F 1
end
if have git
  set G 1
end";
    let b = bash(src);

    assert!(
        b.contains("export NATIVE=\""),
        "(1) self-shell not inlined: {}",
        b
    );
    assert!(
        !b.contains("FISH_ONLY"),
        "(2) dead fish block leaked: {}",
        b
    );
    assert!(
        b.contains("export A=\"other\""),
        "(3) else not inlined: {}",
        b
    );
    assert!(
        b.contains("export B=\"bash\""),
        "(4) matching elif not inlined: {}",
        b
    );
    assert!(
        b.contains("export C=\""),
        "(5) not-fold: body not inlined: {}",
        b
    );
    assert!(
        b.contains("command -v cargo"),
        "(6) and+true: have-guard missing: {}",
        b
    );
    assert!(
        b.contains("export D=\""),
        "(6) and+true: D body missing: {}",
        b
    );
    assert!(
        !b.contains("export E"),
        "(7) and+false: dead block leaked: {}",
        b
    );
    assert!(
        b.contains("export F=\""),
        "(8) or+true: body not inlined: {}",
        b
    );
    assert!(
        b.contains("command -v git"),
        "(9) unknown have-guard removed: {}",
        b
    );
    assert!(
        b.contains("export G=\""),
        "(9) unknown: G body missing: {}",
        b
    );
}

/// Multi-elif chain compiled for every shell:
/// fish/zsh/bash heads fold to the matching shell's branch;
/// for pwsh all three fold to false and the unknown `os linux` guard
/// is promoted to the new if-head with the original else preserved.
#[test]
fn prune_multi_elif_chain_all_shells() {
    let src = "\
if shell fish
  set S fish
elif shell zsh
  set S zsh
elif shell bash
  set S bash
elif os linux
  set S linux
else
  set S other
end";

    let b = bash(src);
    assert!(b.contains("export S=\"bash\""), "bash branch: {}", b);
    assert!(!b.contains("if "), "bash: unexpected if-wrapper: {}", b);

    let z = zsh(src);
    assert!(z.contains("export S=\"zsh\""), "zsh branch: {}", z);
    assert!(!z.contains("if "), "zsh: unexpected if-wrapper: {}", z);

    let f = fish(src);
    assert!(f.contains("set -gx S \"fish\""), "fish branch: {}", f);
    assert!(!f.contains("if "), "fish: unexpected if-wrapper: {}", f);

    // pwsh: fish/zsh/bash all dead -> os linux becomes new head
    let p = pwsh(src);
    assert!(p.contains("$IsLinux"), "pwsh: os guard missing: {}", p);
    assert!(
        p.contains("$env:S = \"linux\""),
        "pwsh: linux body missing: {}",
        p
    );
    assert!(
        p.contains("$env:S = \"other\""),
        "pwsh: else preserved: {}",
        p
    );
}
