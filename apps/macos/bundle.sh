#!/usr/bin/env bash
# Assemble Kanaemi.app into target from the built input method and settings
# app, with the ad-hoc signature Apple silicon needs to run it.
#
#   bundle.sh [folder of the built binaries, target/release by default]
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
release="${1:-$root/target/release}"
assets="$root/apps/macos/assets"
bundle="$assets/bundle"
logo="$assets/logo"
app="$root/target/Kanaemi.app"

# The version kanaemi_core::VERSION reports, as the settings app prints it.
version="$("$release/kanaemi-settings" --version)"
plist() {
  perl -pe "s|\@VERSION\@|$version|g" "$1" >"$2"
}

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
plist "$bundle/Info.plist" "$app/Contents/Info.plist"
cp "$bundle/InfoPlist.strings" "$app/Contents/Resources/"
# The logo's small symbol with the keycap widened to the shape of the other
# input source icons; a template image that macOS tints to match the menu bar.
icon="$bundle/input-source-icon.svg"
rsvg-convert -w 22 -h 16 -o "$app/Contents/Resources/kanaemi.png" "$icon"
rsvg-convert -w 44 -h 32 -o "$app/Contents/Resources/kanaemi@2x.png" "$icon"
# The mode indicator's icon: the logo's small symbol, unchanged.
indicator="$logo/kanaemi-icon-small-mono-white.svg"
rsvg-convert -w 16 -h 16 -o "$app/Contents/Resources/indicator-icon.png" "$indicator"
rsvg-convert -w 32 -h 32 -o "$app/Contents/Resources/indicator-icon@2x.png" "$indicator"
cp "$release/kanaemi" "$app/Contents/MacOS/kanaemi"

# The settings app, opened from the input source menu.
settings="$app/Contents/Resources/KanaemiSettings.app"
mkdir -p "$settings/Contents/MacOS" "$settings/Contents/Resources"
plist "$bundle/SettingsInfo.plist" "$settings/Contents/Info.plist"
cp "$release/kanaemi-settings" "$settings/Contents/MacOS/kanaemi-settings"
iconset="$(mktemp -d)/kanaemi.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  rsvg-convert -w "$size" -h "$size" -o "$iconset/icon_${size}x${size}.png" "$logo/kanaemi-icon.svg"
  rsvg-convert -w "$((size * 2))" -h "$((size * 2))" -o "$iconset/icon_${size}x${size}@2x.png" "$logo/kanaemi-icon.svg"
done
iconutil -c icns -o "$settings/Contents/Resources/kanaemi.icns" "$iconset"
rm -rf "$(dirname "$iconset")"

codesign --force --sign - "$settings"
codesign --force --sign - "$app"
