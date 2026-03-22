//! Integration tests: full source -> emitted text round-trips.
//! Each test parses a .shed snippet and asserts on the exact output
//! of one or more emitters.

use crate::{
    emit::{bash::BashEmitter, fish::FishEmitter, pwsh::PwshEmitter, Emitter},
    parser::Parser,
};

fn bash(src: &str) -> String {
    let ast = Parser::new(src).parse().unwrap();
    BashEmitter::new("bash").render(&ast)
}

fn zsh(src: &str) -> String {
    let ast = Parser::new(src).parse().unwrap();
    BashEmitter::new("zsh").render(&ast)
}

fn fish(src: &str) -> String {
    let ast = Parser::new(src).parse().unwrap();
    FishEmitter.render(&ast)
}

fn pwsh(src: &str) -> String {
    let ast = Parser::new(src).parse().unwrap();
    PwshEmitter.render(&ast)
}

// ── set ───────────────────────────────────────────────────────────────────────

#[test]
fn set_bash() {
    assert_eq!(bash("set EDITOR nvim"), "export EDITOR=\"nvim\"");
}

#[test]
fn set_fish() {
    assert_eq!(fish("set EDITOR nvim"), "set -gx EDITOR \"nvim\"");
}

#[test]
fn set_pwsh() {
    assert_eq!(pwsh("set EDITOR nvim"), "$env:EDITOR = \"nvim\"");
}

// ── path ──────────────────────────────────────────────────────────────────────

#[test]
fn path_prepend_bash() {
    assert_eq!(bash("path+ /usr/local/bin"), "export PATH=\"/usr/local/bin:$PATH\"");
}

#[test]
fn path_append_bash() {
    assert_eq!(bash("path- /opt/bin"), "export PATH=\"$PATH:/opt/bin\"");
}

#[test]
fn path_prepend_fish() {
    assert_eq!(fish("path+ /usr/local/bin"), "fish_add_path -gP \"/usr/local/bin\"");
}

#[test]
fn path_append_fish() {
    assert_eq!(fish("path- /opt/bin"), "fish_add_path -gaP \"/opt/bin\"");
}

#[test]
fn path_prepend_pwsh() {
    assert_eq!(pwsh("path+ C:\\tools"), "$env:PATH = \"C:\\tools;$env:PATH\"");
}

// ── inject ────────────────────────────────────────────────────────────────────

#[test]
fn inject_starship_bash() {
    assert_eq!(
        bash("inject starship init {shell}"),
        "eval \"$(starship init bash)\""
    );
}

#[test]
fn inject_starship_zsh() {
    assert_eq!(
        zsh("inject starship init {shell}"),
        "eval \"$(starship init zsh)\""
    );
}

#[test]
fn inject_starship_fish() {
    assert_eq!(
        fish("inject starship init {shell}"),
        "starship init fish | source"
    );
}

#[test]
fn inject_starship_pwsh() {
    assert_eq!(
        pwsh("inject starship init {shell}"),
        "Invoke-Expression (& starship init powershell)"
    );
}

// ── if / have ─────────────────────────────────────────────────────────────────

#[test]
fn if_have_bash() {
    let src = "if have cargo\npath+ $HOME/.cargo/bin\nend";
    let out = bash(src);
    assert!(out.contains("command -v cargo"),        "missing have check: {}", out);
    assert!(out.contains("export PATH"),             "missing path export: {}", out);
    assert!(out.contains("fi"),                      "missing fi: {}", out);
}

#[test]
fn if_have_fish() {
    let src = "if have cargo\npath+ $HOME/.cargo/bin\nend";
    let out = fish(src);
    assert!(out.contains("type -q cargo"),           "missing have check: {}", out);
    assert!(out.contains("fish_add_path"),           "missing path: {}", out);
    assert!(out.contains("end"),                     "missing end: {}", out);
}

#[test]
fn if_have_pwsh() {
    let src = "if have cargo\npath+ C:\\cargo\\bin\nend";
    let out = pwsh(src);
    assert!(out.contains("Get-Command cargo"),       "missing have check: {}", out);
    assert!(out.contains("$env:PATH"),               "missing path: {}", out);
}

// ── if / os ───────────────────────────────────────────────────────────────────

#[test]
fn if_os_bash() {
    let src = "if os darwin\nset BROWSER open\nelif os linux\nset BROWSER xdg-open\nend";
    let out = bash(src);
    assert!(out.contains("Darwin"),    "missing Darwin: {}",   out);
    assert!(out.contains("Linux"),     "missing Linux: {}",    out);
    assert!(out.contains("elif"),      "missing elif: {}",     out);
    assert!(out.contains("fi"),        "missing fi: {}",       out);
}

#[test]
fn if_os_fish() {
    let src = "if os darwin\nset BROWSER open\nend";
    let out = fish(src);
    assert!(out.contains("test (uname -s) = \"Darwin\""), "missing cond: {}", out);
    assert!(out.contains("end"),                          "missing end: {}", out);
}

#[test]
fn if_os_pwsh() {
    let src = "if os windows\nset SHELL pwsh\nend";
    let out = pwsh(src);
    assert!(out.contains("$IsWindows"), "missing cond: {}", out);
}

// ── if / shell ────────────────────────────────────────────────────────────────

#[test]
fn if_shell_bash_self_true() {
    let src = "if shell bash\nset IN_BASH yes\nend";
    let out = bash(src);
    assert!(out.contains("$BASH_VERSION"), "missing bash version check: {}", out);
}

#[test]
fn if_shell_fish_self_true() {
    let src = "if shell fish\nset IN_FISH yes\nend";
    let out = fish(src);
    assert!(out.contains("true"), "fish should emit true for own shell: {}", out);
}

#[test]
fn if_shell_pwsh_self_true() {
    let src = "if shell pwsh\nset IN_PWSH yes\nend";
    let out = pwsh(src);
    assert!(out.contains("$true"), "pwsh should emit $true for own shell: {}", out);
}

#[test]
fn if_shell_cross_is_false_in_bash() {
    let src = "if shell fish\nset X 1\nend";
    let out = bash(src);
    assert!(out.contains("false"), "bash should emit false for fish shell: {}", out);
}

// ── if / else ─────────────────────────────────────────────────────────────────

#[test]
fn if_else_bash() {
    let src = "if os darwin\nset A mac\nelse\nset A other\nend";
    let out = bash(src);
    assert!(out.contains("else"), "missing else: {}", out);
    assert!(out.contains("fi"),   "missing fi: {}",   out);
}

#[test]
fn if_else_fish() {
    let src = "if os darwin\nset A mac\nelse\nset A other\nend";
    let out = fish(src);
    assert!(out.contains("else"), "missing else: {}", out);
    assert!(out.contains("end"),  "missing end: {}",  out);
}

// ── multi-node ────────────────────────────────────────────────────────────────

#[test]
fn readme_example_bash() {
    let src = r#"
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
end
"#;
    let out = bash(src);
    assert!(out.contains("export EDITOR=\"nvim\""),          "EDITOR: {}",    out);
    assert!(out.contains("Darwin"),                          "Darwin: {}",    out);
    assert!(out.contains("Linux"),                           "Linux: {}",     out);
    assert!(out.contains("command -v cargo"),                "cargo: {}",     out);
    assert!(out.contains("eval \"$(starship init bash)\""),  "starship: {}",  out);
}

#[test]
fn readme_example_fish() {
    let src = r#"
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
end
"#;
    let out = fish(src);
    assert!(out.contains("set -gx EDITOR"),           "EDITOR: {}",   out);
    assert!(out.contains("Darwin"),                   "Darwin: {}",   out);
    assert!(out.contains("type -q cargo"),             "cargo: {}",    out);
    assert!(out.contains("starship init fish"),        "starship: {}", out);
}
