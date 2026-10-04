# Run `nix develop` (or use direnv) first so every tool is on PATH.

set shell := ["bash", "-euo", "pipefail", "-c"]

# List the recipes.
default:
    @just --list

# Format every source.
fmt:
    cargo fmt --all
    nix fmt flake.nix

# Check formatting without changing files.
fmt-check:
    cargo fmt --all --check

# Lint, and build the core for WebAssembly to prove it needs nothing from its host.
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo check -p kanaemi-core --target wasm32-unknown-unknown

# Run the Rust tests.
test:
    cargo test --workspace

# Build the macOS input method and install it into ~/Library/Input Methods.
install-macos:
    apps/macos/install.sh

# Build the IBus input method and install it under /usr/local/lib/kanaemi; asks for sudo.
install-ibus:
    apps/ibus/install.sh

# Build the Windows input method and install it into Program Files; run from an elevated shell.
install-windows:
    powershell -NoProfile -ExecutionPolicy Bypass -File apps/windows/install.ps1

# Everything CI runs.
ci: fmt-check lint test
