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
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn bash(src: &str) -> String {
    BashEmitter::new("bash").render(&Parser::new(src).parse().unwrap())
}
fn zsh(src: &str) -> String {
    BashEmitter::new("zsh").render(&Parser::new(src).parse().unwrap())
}
fn fish(src: &str) -> String {
    FishEmitter.render(&Parser::new(src).parse().unwrap())
}
fn pwsh(src: &str) -> String {
    PwshEmitter.render(&Parser::new(src).parse().unwrap())
}

// ── set / path / inject across all shells ────────────────────────────────────

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

/// inject replaces {shell} with the actual shell name.
#[test]
fn inject_shell_placeholder() {
    assert_eq!(
        bash("inject starship init {shell}"),
        "eval \"$(starship init bash)\""
    );
    assert_eq!(
        zsh("inject starship init {shell}"),
        "eval \"$(starship init zsh)\""
    );
    assert_eq!(
        fish("inject starship init {shell}"),
        "starship init fish | source"
    );
    assert_eq!(
        pwsh("inject starship init {shell}"),
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

/// `if shell` emits a self-true / cross-false detection per backend.
#[test]
fn if_shell_self_and_cross() {
    // each shell is true for itself
    assert!(bash("if shell bash\nset X 1\nend").contains("$BASH_VERSION"));
    assert!(fish("if shell fish\nset X 1\nend").contains("true"));
    assert!(pwsh("if shell pwsh\nset X 1\nend").contains("$true"));
    // bash emitting fish-shell guard gives "false"
    assert!(bash("if shell fish\nset X 1\nend").contains("false"));
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

/// Reflects the README example: set + os guard + have guards + inject.
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
  inject starship init {shell}
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
