# Release & Publish Workflow

This document covers how to test, build, and distribute `shed` binaries across
platforms. Everything here runs with tools that are standard in the Rust and
open-source ecosystem — no proprietary CI vendor lock-in required.

---

## 1. Local quality gate (run before every release)

```sh
cargo fmt --check          # formatting
cargo clippy -- -D warnings # lints
cargo test                 # all 94 unit + integration tests
```

All three must pass with exit code 0.

---

## 2. Version bump

Edit the `version` field in [`Cargo.toml`](../Cargo.toml):

```toml
[package]
version = "0.2.0"
```

Commit and tag:

```sh
git commit -am "chore: release v0.2.0"
git tag v0.2.0
git push origin main --tags
```

The tag push is the trigger for the GitHub Actions release workflow described
below.

---

## 3. Cross-platform binary builds (GitHub Actions)

Create `.github/workflows/release.yml`. The workflow:

1. Triggers on `push` to a `v*` tag.
2. Builds a static native binary for each target.
3. Attaches all binaries to the GitHub Release.

```yaml
name: Release

on:
  push:
    tags: ["v*"]

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-musl
            artifact: shed-linux-x86_64
          - os: ubuntu-latest
            target: aarch64-unknown-linux-musl
            artifact: shed-linux-aarch64
          - os: macos-latest
            target: x86_64-apple-darwin
            artifact: shed-macos-x86_64
          - os: macos-latest
            target: aarch64-apple-darwin
            artifact: shed-macos-aarch64
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            artifact: shed-windows-x86_64.exe

    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Install musl tools (Linux)
        if: contains(matrix.target, 'musl')
        run: sudo apt-get install -y musl-tools

      - name: Build
        run: cargo build --release --target ${{ matrix.target }}

      - name: Rename binary
        shell: bash
        run: |
          src=target/${{ matrix.target }}/release/shed
          [ -f "${src}.exe" ] && src="${src}.exe"
          cp "$src" ${{ matrix.artifact }}

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.artifact }}
          path: ${{ matrix.artifact }}

  release:
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/download-artifact@v4
        with:
          merge-multiple: true

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: shed-*
          generate_release_notes: true
```

### Why musl for Linux?

Musl-linked binaries are fully static: they carry no dependency on the host
`glibc` version. A binary built with `x86_64-unknown-linux-musl` runs on any
x86-64 Linux system from Alpine to RHEL without extra installation steps.

---

## 4. Windows — Scoop

[Scoop](https://scoop.sh) installs CLI tools from JSON manifests without
requiring administrator rights.

### One-time setup

Create a public GitHub repository named e.g. `scoop-shed` (a "bucket").

Add a manifest file `bucket/shed.json`:

```json
{
  "version": "0.2.0",
  "description": "Shell Environment Declaration — compile env.shed to bash, zsh, fish, or pwsh",
  "license": "MIT",
  "homepage": "https://github.com/lvyuemeng/shed",
  "architecture": {
    "64bit": {
      "url": "https://github.com/lvyuemeng/shed/releases/download/v0.2.0/shed-windows-x86_64.exe",
      "bin": "shed-windows-x86_64.exe",
      "aliases": ["shed"]
    }
  },
  "checkver": {
    "github": "https://github.com/lvyuemeng/shed"
  },
  "autoupdate": {
    "architecture": {
      "64bit": {
        "url": "https://github.com/lvyuemeng/shed/releases/download/v$version/shed-windows-x86_64.exe"
      }
    }
  }
}
```

Update the `url` and `version` fields on every release. Scoop's `checkver` /
`autoupdate` fields can automate this via `scoop-autoupdate`.

### User installation

```powershell
scoop bucket add shed https://github.com/lvyuemeng/scoop-shed
scoop install shed
```

---

## 5. macOS & Linux — shell one-liner installer

For users who do not use a package manager, provide a curl-pipe-sh installer
script at `install.sh` in the repo root. It detects the OS and architecture,
downloads the correct binary from the GitHub release, and places it in
`~/.local/bin` (Linux) or `/usr/local/bin` (macOS).

```sh
#!/bin/sh
set -e

VERSION=${1:-latest}
REPO="lvyuemeng/shed"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "${OS}-${ARCH}" in
  linux-x86_64)   FILE="shed-linux-x86_64"   ;;
  linux-aarch64)  FILE="shed-linux-aarch64"  ;;
  darwin-x86_64)  FILE="shed-macos-x86_64"   ;;
  darwin-arm64)   FILE="shed-macos-aarch64"  ;;
  *) echo "Unsupported platform: ${OS}-${ARCH}"; exit 1 ;;
esac

if [ "$VERSION" = "latest" ]; then
  URL="https://github.com/${REPO}/releases/latest/download/${FILE}"
else
  URL="https://github.com/${REPO}/releases/download/${VERSION}/${FILE}"
fi

DEST="$HOME/.local/bin"
mkdir -p "$DEST"
curl -fsSL "$URL" -o "${DEST}/shed"
chmod +x "${DEST}/shed"
echo "shed installed to ${DEST}/shed"
echo "Make sure ${DEST} is on your PATH."
```

User invocation:

```sh
curl -fsSL https://raw.githubusercontent.com/lvyuemeng/shed/main/install.sh | sh
```

---

## 6. Linux package managers (future)

| Manager | Path |
|---------|------|
| **Homebrew** (also macOS) | Submit a formula to `homebrew-core` or maintain a tap at `lvyuemeng/homebrew-shed` |
| **AUR** (Arch Linux) | Publish a `PKGBUILD` that downloads the musl binary or builds from source via `cargo` |
| **Nix / nixpkgs** | Add a derivation using `rustPlatform.buildRustPackage`; the zero-dependency binary makes this straightforward |
| **Debian / Ubuntu .deb** | Use `cargo deb` (a zero-configuration crate) to produce a `.deb` in CI and attach it to the release |

All of these are optional extras. The curl installer and Scoop cover the most
common cases without requiring a package maintainer relationship.

---

## 7. crates.io publish (optional library use)

If the parser or AST are ever exposed as a library:

```sh
cargo publish --dry-run   # verify the package before upload
cargo publish             # upload to crates.io
```

Requires a crates.io API token set as the `CARGO_REGISTRY_TOKEN` secret in
GitHub Actions.

---

## 8. Release checklist

```
[ ] cargo fmt --check passes
[ ] cargo clippy -- -D warnings passes
[ ] cargo test passes (all 94 tests)
[ ] Cargo.toml version bumped
[ ] CHANGELOG updated (optional)
[ ] git tag vX.Y.Z pushed
[ ] GitHub Actions release workflow completes
[ ] Scoop manifest version updated in scoop-shed repo
[ ] install.sh default version updated (if hard-coded)
```
