mod ast;
mod emit;
mod parser;
mod prune;
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
use prune::prune_nodes;

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
  call cmd [args]             eval-init (starship, zoxide, …)
                              use {shell} as a placeholder for the target shell name

  alias name body              define a shell alias

  if    have  <cmd>           guard: command must exist on PATH
  if    os    darwin|linux|windows
  if    shell bash|zsh|fish|pwsh
  if    not   <cond>          negate a condition
  if    <cond> and <cond>     both conditions must hold
  if    <cond> or  <cond>     either condition must hold
  elif  …
  else
  end

  # comment (inline or full-line)

  COMPOUND CONDITIONS (precedence: not > and > or)
  if not have cargo              negate a single condition
  if have cargo and os linux     both must hold
  if os darwin or os linux       either holds
  if not have cargo and os linux parsed as: (not have cargo) and (os linux)
";

/// Read source from `path` (a file) or from stdin when `path` is `None`.
/// Errors include the path in the message so the caller can surface it directly.
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

/// Return the directory of the shed source file as an absolute path,
/// resolving it against the current working directory when necessary.
/// Returns `None` when reading from stdin (no anchor directory).
fn base_dir(file: Option<&str>) -> Option<PathBuf> {
    let parent = Path::new(file?).parent()?;
    Some(
        env::current_dir()
            .map(|cwd| cwd.join(parent))
            .unwrap_or_else(|_| parent.to_path_buf()),
    )
}

/// Render `ast` to a shell-specific string for the named `shell`.
/// Returns an error for unknown shell names.
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

/// Top-level entry point: parses `args`, reads the source, and either
/// runs the `check` subcommand or compiles and prints the target shell output.
fn run(args: &[String]) -> Result<(), String> {
    // --help / -h anywhere in the args, or bare invocation with no subcommand,
    // prints usage to stdout and exits cleanly (exit 0).
    if args.iter().any(|a| a == "--help" || a == "-h") || args.len() < 2 {
        print!("{}", USAGE);
        return Ok(());
    }

    let shell = args.get(1).map(String::as_str).ok_or(USAGE)?;
    let file = args.get(2).map(String::as_str);
    let base = base_dir(file);

    // Parse and resolve paths in one step: Parser::new takes the base dir so
    // `path+` / `path-` tokens are normalised during parsing — no separate pass.
    let parsed = read(file)
        .and_then(|src| Parser::new(&src, base).parse().map_err(|e| e.to_string()))?;

    if shell == "check" {
        println!("ok ({} top-level nodes)", parsed.len());
        return Ok(());
    }

    let ast = prune_nodes(parsed, shell);

    emit(shell, &ast).map(|out| println!("{}", out))
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Err(e) = run(&args) {
        eprintln!("shed: {}", e);
        process::exit(1);
    }
}
