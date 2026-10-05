#!/usr/bin/env bash
# Build the IBus engine and the settings app, install them under
# /usr/local/lib/kanaemi, and tell IBus about the engine. Asks for sudo.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
lib=/usr/local/lib/kanaemi

cargo build --release -p kanaemi-ibus -p kanaemi-settings --manifest-path "$root/Cargo.toml"

sudo install -d "$lib"
sudo install -m 755 "$root/target/release/kanaemi-ibus" "$lib/kanaemi-ibus"
# The engine opens the settings app from beside itself.
sudo install -m 755 "$root/target/release/kanaemi-settings" "$lib/kanaemi-settings"
sudo install -m 644 "$root/apps/macos/assets/logo/kanaemi-icon.svg" "$lib/kanaemi.svg"
# The version kanaemi_core::VERSION reports, as the settings app prints it.
version="$("$root/target/release/kanaemi-settings" --version)"
component="$(mktemp)"
perl -pe "s|\@LIBDIR\@|$lib|g; s|\@VERSION\@|$version|g" "$root/apps/ibus/kanaemi.xml" >"$component"
sudo install -m 644 "$component" /usr/share/ibus/component/kanaemi.xml
rm -f "$component"

# A running ibus-daemon reads the components again when restarted.
if ibus address >/dev/null 2>&1 && [ "$(ibus address)" != "(null)" ]; then
  ibus write-cache
  ibus restart
fi
echo "installed $lib; add Kanaemi under Japanese in the input sources"
