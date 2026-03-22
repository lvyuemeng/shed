mod ast;
mod emit;
mod parser;
#[cfg(test)]
mod tests;

use std::{
    env,
    fs,
    io::{self, Read},
    process,
};

use emit::{bash::BashEmitter, fish::FishEmitter, pwsh::PwshEmitter, Emitter};
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

fn read(path: Option<&String>) -> Result<String, String> {
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

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("{}", USAGE);
        process::exit(1);
    }

    let shell   = &args[1];
    let file    = args.get(2);

    let src = read(file).unwrap_or_else(|e| {
        eprintln!("shed: {}", e);
        process::exit(1);
    });

    let ast = Parser::new(&src).parse().unwrap_or_else(|e| {
        eprintln!("shed: {}", e);
        process::exit(1);
    });

    if shell == "check" {
        println!("ok ({} top-level nodes)", ast.len());
        return;
    }

    let out: String = match shell.as_str() {
        "bash" => BashEmitter::new("bash").render(&ast),
        "zsh"  => BashEmitter::new("zsh").render(&ast),
        "fish" => FishEmitter.render(&ast),
        "pwsh" => PwshEmitter.render(&ast),
        other  => {
            eprintln!("shed: unknown shell {:?} — choose: bash, zsh, fish, pwsh", other);
            process::exit(1);
        }
    };

    println!("{}", out);
}