# shed

**Sh**ell **E**nvironment **D**eclaration.

Write your environment once. Compile to bash, zsh, fish, or powershell.

```sh
shed bash  ~/.config/shed/env.shed   # → bash / POSIX sh
shed zsh   ~/.config/shed/env.shed   # → zsh
shed fish  ~/.config/shed/env.shed   # → fish
shed pwsh  ~/.config/shed/env.shed   # → PowerShell
shed check ~/.config/shed/env.shed   # → parse & validate only
```

## install

```sh
cargo install --path .
```

Cross-compile with [`cross`](https://github.com/cross-rs/cross):

```sh
cross build --release --target x86_64-pc-windows-gnu
cross build --release --target aarch64-apple-darwin
```

Or grab a binary from releases — no runtime required.

## shell rc  (write once, forget forever)

```sh
# ~/.bashrc or ~/.zshrc
eval "$(shed bash ~/.config/shed/env.shed)"
```

```fish
# ~/.config/fish/config.fish
shed fish ~/.config/shed/env.shed | source
```

```powershell
# $PROFILE
shed pwsh ~/.config/shed/env.shed | Invoke-Expression
```

## dsl

### statements

| Keyword | Syntax | Effect |
|---------|--------|--------|
| `set` | `set KEY value` | Export an environment variable |
| `path+` | `path+ dir` | Prepend `dir` to `PATH` |
| `path-` | `path- dir` | Append `dir` to `PATH` |
| `call` | `call cmd [args…]` | Run `cmd --init {shell}` style initialisers; `{shell}` expands to the target shell name |

### conditions

```sh
if have <cmd>              # command exists on PATH
if os   darwin|linux|windows
if shell bash|zsh|fish|pwsh
elif <cond>
else
end
```

### compound conditions

Conditions can be combined with `not`, `and`, and `or` on a single line.

```sh
if not have cargo                        # negate
if have cargo and os linux               # both must hold
if os darwin or os linux                 # either holds
if not have nvim and shell bash          # not binds tighter than and
if have cargo or os linux and shell bash # and binds tighter than or
```

**Precedence (high → low):** `not` > `and` > `or`  
Use nested `if` blocks when you need explicit grouping.

### compile-time pruning

`if shell <name>` branches that cannot match the compile target are removed
from the output entirely. Dead `elif` / `else` chains are collapsed too.
The emitted script contains no unreachable code.

### comments

```sh
# full-line comment
set KEY value  # inline comment
```

## example

```sh
set EDITOR nvim

# per-OS browser
if os darwin
  set BROWSER open
elif os linux
  set BROWSER xdg-open
end

# cargo tools — only when cargo is present
if have cargo
  path+ ~/.cargo/bin
end

# compound: zoxide only on Linux when it is installed
if have zoxide and os linux
  call zoxide init {shell}
end

# starship for every shell
if have starship
  call starship init {shell}
end
```

<details>
<summary>Compiles to bash</summary>

```sh
export EDITOR="nvim"
if [ "$(uname -s)" = "Darwin" ]; then
  export BROWSER="open"
elif [ "$(uname -s)" = "Linux" ]; then
  export BROWSER="xdg-open"
fi
if command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
if command -v zoxide >/dev/null 2>&1 && [ "$(uname -s)" = "Linux" ]; then
  eval "$(zoxide init bash)"
fi
if command -v starship >/dev/null 2>&1; then
  eval "$(starship init bash)"
fi
```

</details>

<details>
<summary>Compiles to fish</summary>

```fish
set -gx EDITOR "nvim"
if test (uname -s) = "Darwin"
  set -gx BROWSER "open"
else if test (uname -s) = "Linux"
  set -gx BROWSER "xdg-open"
end
if type -q cargo
  fish_add_path -gP "$HOME/.cargo/bin"
end
if type -q zoxide; and test (uname -s) = "Linux"
  zoxide init fish | source
end
if type -q starship
  starship init fish | source
end
```

</details>

<details>
<summary>Compiles to PowerShell</summary>

```powershell
$env:EDITOR = "nvim"
if ($IsMacOS) {
  $env:BROWSER = "open"
} elseif ($IsLinux) {
  $env:BROWSER = "xdg-open"
}
if (Get-Command cargo -ErrorAction SilentlyContinue) {
  $env:PATH = "$HOME/.cargo/bin;$env:PATH"
}
if ((Get-Command zoxide -ErrorAction SilentlyContinue) -and ($IsLinux)) {
  Invoke-Expression (& zoxide init powershell)
}
if (Get-Command starship -ErrorAction SilentlyContinue) {
  Invoke-Expression (& starship init powershell)
}
```

</details>

## path resolution

`path+` and `path-` directories are resolved at compile time, applied in order:

1. **`~` prefix** — expanded to `$HOME` (Unix) or `%USERPROFILE%` (Windows).
   If neither variable is set the `~` is kept as-is.
2. **Relative path** — joined onto the directory that contains the `.shed` file,
   so the file is portable regardless of where you run `shed` from.
   When reading from stdin there is no anchor; relative paths are emitted unchanged.
3. **Absolute path** — passed through unchanged.

No filesystem access is performed; the path does not need to exist.

```sh
# given: shed bash ~/dot/env.shed
path+ ~/.cargo/bin      # → $HOME/.cargo/bin        (rule 1)
path+ bin               # → /home/you/dot/bin        (rule 2, relative to env.shed)
path+ /usr/local/bin    # → /usr/local/bin            (rule 3, absolute)
```

## chezmoi

chezmoi manages exactly two things — `env.shed` and the `shed` binary:

```
dot_config/shed/env.shed    # your one source of truth
```

No templates needed. No loaders. No per-shell files.

## contributing

**Adding a new shell backend**
1. Create `src/emit/<shell>.rs` and implement the `Emitter` trait.
2. Declare `pub mod <shell>;` in `src/emit.rs`.
3. Add a match arm in `src/main.rs`.

**Adding a new statement keyword**
1. Add a variant to `Node` in `src/ast.rs`.
2. Add a parse arm in `Parser::block()` in `src/parser.rs`.
3. Add an emit arm in every backend — the compiler enforces exhaustiveness.

**Adding a new condition type**
1. Add a `Cond` variant in `src/ast.rs`.
2. Add a parse arm in `Parser::parse_cond()` in `src/parser.rs`.
3. Add an emit arm in every backend's `cond()` method.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full design and
[`docs/AGENT.md`](docs/AGENT.md) for coding conventions.

## license

MIT — see [`LICENSE`](LICENSE).