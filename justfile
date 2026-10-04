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

# Everything CI runs.
ci: fmt-check lint test
