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

# Run the Rust tests with `cases` generated inputs for each property test instead of proptest's default.
test-thorough cases="20000":
    PROPTEST_CASES={{cases}} cargo test --workspace

# Measure how often conversion puts first what is meant; also with the dictionaries in the folder KANAEMI_ACCURACY_DICTIONARIES when set, ranked with the model KANAEMI_ACCURACY_MODEL when set.
accuracy:
    cargo test -p kanaemi-engine --test integration accuracy -- --nocapture

# Build the macOS input method and install it into ~/Library/Input Methods.
install-macos:
    apps/macos/install.sh

# Import the identity releases sign with, given as MACOS_SIGNING_P12 and MACOS_SIGNING_PASSWORD, into the login keychain, so install-macos signs with it too.
macos-import-identity:
    apps/macos/import-identity.sh

# Make the identity releases sign the macOS app with, into a new folder, for the secrets of the GitHub environment "release".
macos-release-identity folder:
    apps/macos/release-identity.sh {{folder}}

# Build the IBus input method and install it under /usr/local/lib/kanaemi; asks for sudo.
install-ibus:
    apps/ibus/install.sh

# Build the Windows input method and install it into Program Files; run from an elevated shell.
install-windows:
    powershell -NoProfile -ExecutionPolicy Bypass -File apps/windows/install.ps1

# Make the macOS installer package in target/package; run outside the Nix shell.
package-macos:
    apps/macos/package.sh

# Make the Debian and RPM packages in target/package; run outside the Nix shell, with nfpm.
package-ibus:
    apps/ibus/package.sh

# Make the Windows Installer package in target/package; needs the WiX toolset.
package-windows:
    powershell -NoProfile -ExecutionPolicy Bypass -File apps/windows/package.ps1

# Everything CI runs.
ci: fmt-check lint test
