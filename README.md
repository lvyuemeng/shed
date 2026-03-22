# shed

**Sh**ell **E**nvironment **D**eclaration.

Write your environment once. Compile to bash, zsh, fish, or powershell.

```sh
shed bash  ~/.config/shed/env.shed   # → POSIX sh
shed zsh   ~/.config/shed/env.shed   # → zsh
shed fish  ~/.config/shed/env.shed   # → fish
shed pwsh  ~/.config/shed/env.shed   # → powershell
shed check ~/.config/shed/env.shed   # → syntax check
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

```sh
# ── env vars ──────────────────────────
set KEY value

# ── path ──────────────────────────────
path+ dir          # prepend
path- dir          # append

# ── eval-init (starship, zoxide…) ─────
inject cmd [args]  # {shell} expands to the target shell name

# ── conditions ────────────────────────
if have <cmd>             # command exists on PATH
if os   darwin|linux|windows
if shell bash|zsh|fish|pwsh
elif …
else
end

# comment
```

## example

```sh
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
```

Compiles to bash:
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
if command -v starship >/dev/null 2>&1; then
  eval "$(starship init bash)"
fi
```

Compiles to fish:
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
if type -q starship
  starship init fish | source
end
```

Compiles to powershell:
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
if (Get-Command starship -ErrorAction SilentlyContinue) {
  Invoke-Expression (& starship init powershell)
}
```

## path resolution

`path+` and `path-` directories are resolved at compile time using these rules,
applied in order:

1. **`~` prefix** — expanded to `$HOME` (Unix) or `%USERPROFILE%` (Windows).
   If neither variable is set the `~` is kept as-is.
2. **Relative path** — joined onto the directory that contains the `.shed` file,
   so the file is portable regardless of where you run `shed` from.
   When reading from stdin there is no anchor; relative paths are emitted unchanged.
3. **Absolute path** — passed through unchanged.

No filesystem access is performed during resolution; the path does not need to exist.

```sh
# given: shed bash ~/dot/env.shed

path+ ~/.cargo/bin          # → $HOME/.cargo/bin  (rule 1)
path+ bin                   # → /home/you/dot/bin  (rule 2, relative to env.shed)
path+ /usr/local/bin        # → /usr/local/bin      (rule 3, absolute)
```

## chezmoi

chezmoi manages exactly two things — `env.shed` and the `shed` binary:

```
dot_config/shed/env.shed          # your one source of truth
```

No templates needed. No loaders. No per-shell files.

## extending

Adding a new shell = one struct implementing `Emitter` in `src/emit/`.  
Adding a new keyword = one variant in `ast.rs`, one `match` arm in `parser.rs`,
one `match` arm per emitter.