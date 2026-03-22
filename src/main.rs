mod ast;
mod emit;
mod parser;
#[cfg(test)]
mod tests;

use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process,
};

use ast::Node;
use emit::{Emitter, bash::BashEmitter, fish::FishEmitter, pwsh::PwshEmitter};
use parser::Parser;

const USAGE: &str = "\
shed — Shell Environment Declaration
compile a single env.shed to any shell dialect

USAGE
  shed <shell> [file]    compile (reads stdin when file is omitted)
  shed check  [file]     parse only — reports errors or 'ok'

SHELLS
  bash   zsh   fish   pwsh

SHELL RC  (write once, never touch again)
  bash / zsh   eval \"$(shed bash ~/.config/shed/env.shed)\"
  fish         shed fish  ~/.config/shed/env.shed | source
  pwsh         shed pwsh  ~/.config/shed/env.shed | Invoke-Expression

DSL REFERENCE
  set   KEY value             export an env var
  path+ dir                   prepend dir to PATH
  path- dir                   append  dir to PATH
  inject cmd [args]           eval-init (starship, zoxide, …)
                              use {shell} as a placeholder for the target shell name

  if    have  <cmd>           guard: command must exist on PATH
  if    os    darwin|linux|windows
  if    shell bash|zsh|fish|pwsh
  elif  …
  else
  end

  # comment (inline or full-line)
";

fn read(path: Option<&str>) -> Result<String, String> {
    match path {
        Some(p) => fs::read_to_string(p).map_err(|e| format!("{}: {}", p, e)),
        None => {
            let mut s = String::new();
            io::stdin()
                .read_to_string(&mut s)
                .map_err(|e| e.to_string())?;
            Ok(s)
        }
    }
}

/// Resolve a path token from a shed source file.
///
/// Rules (applied in order):
/// 1. `~` prefix  → replace with `$HOME` (Unix) / `$USERPROFILE` (Windows).
///    If neither var is set the `~` is left as-is rather than silently
///    producing a wrong path.
/// 2. Relative path → join onto `base` (the directory of the shed file).
///    When reading from stdin `base` is `None`; relative paths are kept as-is
///    because there is no meaningful anchor.
/// 3. Absolute path → returned unchanged.
///
/// No I/O is performed; the path need not exist.
fn resolve_path(dir: &str, base: Option<&Path>) -> String {
    // Step 1 — home-dir expansion.
    let expanded: PathBuf = if let Some(rest) = dir.strip_prefix('~') {
        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .unwrap_or_default();
        if home.is_empty() {
            // Cannot expand; return as-is.
            return dir.to_owned();
        }
        // rest starts with '/' on Unix or is empty for bare '~'.
        PathBuf::from(home).join(rest.trim_start_matches('/'))
    } else {
        PathBuf::from(dir)
    };

    // Step 2 — resolve relative paths against the shed file's directory.
    if expanded.is_relative() {
        base.map(|b| b.join(&expanded))
            .unwrap_or(expanded)
            .to_string_lossy()
            .into_owned()
    } else {
        expanded.to_string_lossy().into_owned()
    }
}

/// Walk the AST and resolve every `Node::Path` directory in place.
/// All other node types are passed through unchanged.
fn resolve_paths(nodes: Vec<Node>, base: Option<&Path>) -> Vec<Node> {
    nodes
        .into_iter()
        .map(|n| match n {
            Node::Path { dir, prepend } => Node::Path {
                dir: resolve_path(&dir, base),
                prepend,
            },
            Node::If(mut inode) => {
                inode.body = resolve_paths(inode.body, base);
                inode.elifs = inode
                    .elifs
                    .into_iter()
                    .map(|(c, b)| (c, resolve_paths(b, base)))
                    .collect();
                inode.else_ = resolve_paths(inode.else_, base);
                Node::If(inode)
            }
            other => other,
        })
        .collect()
}

fn base_dir(file: Option<&str>) -> Option<PathBuf> {
    let parent = Path::new(file?).parent()?;
    Some(
        env::current_dir()
            .map(|cwd| cwd.join(parent))
            .unwrap_or_else(|_| parent.to_path_buf()),
    )
}

fn emit(shell: &str, ast: &[Node]) -> Result<String, String> {
    match shell {
        "bash" => Ok(BashEmitter::new("bash").render(ast)),
        "zsh" => Ok(BashEmitter::new("zsh").render(ast)),
        "fish" => Ok(FishEmitter.render(ast)),
        "pwsh" => Ok(PwshEmitter.render(ast)),
        other => Err(format!(
            "unknown shell {:?} — choose: bash, zsh, fish, pwsh",
            other
        )),
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let shell = args.get(1).map(String::as_str).ok_or(USAGE)?;
    let file = args.get(2).map(String::as_str);
    let base = base_dir(file);

    let ast = read(file)
        .and_then(|src| Parser::new(&src).parse().map_err(|e| e.to_string()))
        .map(|nodes| resolve_paths(nodes, base.as_deref()))?;

    if shell == "check" {
        println!("ok ({} top-level nodes)", ast.len());
        return Ok(());
    }

    emit(shell, &ast).map(|out| println!("{}", out))
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Err(e) = run(&args) {
        eprintln!("shed: {}", e);
        process::exit(1);
    }
}
