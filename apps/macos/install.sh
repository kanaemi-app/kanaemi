#!/usr/bin/env bash
# Build Kanaemi.app and install it into ~/Library/Input Methods.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
app="$root/target/Kanaemi.app"
dest="$HOME/Library/Input Methods/Kanaemi.app"

cargo build --release -p kanaemi-macos -p kanaemi-settings --manifest-path "$root/Cargo.toml"
"$root/apps/macos/bundle.sh"

mkdir -p "$HOME/Library/Input Methods"
rm -rf "$dest"
cp -R "$app" "$dest"
# A running instance keeps the old binary; macOS starts the new one when needed.
pkill -x kanaemi || true
pkill -x kanaemi-settings || true
echo "installed $dest"
