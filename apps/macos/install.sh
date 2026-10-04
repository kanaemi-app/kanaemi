#!/usr/bin/env bash
# Build Kanaemi.app and install it into ~/Library/Input Methods.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
assets="$root/apps/macos/assets"
bundle="$assets/bundle"
logo="$assets/logo"
app="$root/target/Kanaemi.app"
dest="$HOME/Library/Input Methods/Kanaemi.app"

cargo build --release -p kanaemi-macos -p kanaemi-settings --manifest-path "$root/Cargo.toml"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bundle/Info.plist" "$app/Contents/Info.plist"
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
cp "$root/target/release/kanaemi" "$app/Contents/MacOS/kanaemi"

# The settings app, opened from the input source menu.
settings="$app/Contents/Resources/KanaemiSettings.app"
mkdir -p "$settings/Contents/MacOS" "$settings/Contents/Resources"
cp "$bundle/SettingsInfo.plist" "$settings/Contents/Info.plist"
cp "$root/target/release/kanaemi-settings" "$settings/Contents/MacOS/kanaemi-settings"
iconset="$(mktemp -d)/kanaemi.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  rsvg-convert -w "$size" -h "$size" -o "$iconset/icon_${size}x${size}.png" "$logo/kanaemi-icon.svg"
  rsvg-convert -w "$((size * 2))" -h "$((size * 2))" -o "$iconset/icon_${size}x${size}@2x.png" "$logo/kanaemi-icon.svg"
done
iconutil -c icns -o "$settings/Contents/Resources/kanaemi.icns" "$iconset"
plutil -insert CFBundleIconFile -string kanaemi "$settings/Contents/Info.plist"
# The version kanaemi_core::VERSION reports, as the settings app prints it.
version="$("$root/target/release/kanaemi-settings" --version)"
plutil -replace CFBundleShortVersionString -string "$version" "$app/Contents/Info.plist"
plutil -replace CFBundleShortVersionString -string "$version" "$settings/Contents/Info.plist"

codesign --force --sign - "$settings"

codesign --force --sign - "$app"

mkdir -p "$HOME/Library/Input Methods"
rm -rf "$dest"
cp -R "$app" "$dest"
# A running instance keeps the old binary; macOS starts the new one when needed.
pkill -x kanaemi || true
pkill -x kanaemi-settings || true
echo "installed $dest"
