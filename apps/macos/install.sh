#!/usr/bin/env bash
# Build Kanaemi.app and install it into ~/Library/Input Methods.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
app="$root/target/Kanaemi.app"
dest="$HOME/Library/Input Methods/Kanaemi.app"

# Signed by the development identity when there is one, so that a rebuild
# keeps the permissions macOS gave the app (dev-identity.sh creates it).
identity="Kanaemi Development"
if security find-certificate -c "$identity" >/dev/null 2>&1; then
  export KANAEMI_SIGN_IDENTITY="$identity"
else
  echo "no \"$identity\" in the keychain; signing ad-hoc (run apps/macos/dev-identity.sh to keep permissions across rebuilds)" >&2
fi

cargo build --release -p kanaemi-macos -p kanaemi-settings --manifest-path "$root/Cargo.toml"
"$root/apps/macos/bundle.sh"

mkdir -p "$HOME/Library/Input Methods"
rm -rf "$dest"
cp -R "$app" "$dest"
# A running instance keeps the old binary; macOS starts the new one when needed.
pkill -x kanaemi || true
pkill -x kanaemi-settings || true
echo "installed $dest"
