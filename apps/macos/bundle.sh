#!/usr/bin/env bash
# Assemble Kanaemi.app into target/bundle.noindex from the built input method
# and settings app, with the signature Apple silicon needs to run it: ad-hoc,
# or by the identity KANAEMI_SIGN_IDENTITY names.
#
#   bundle.sh [folder of the built binaries, target/release by default]
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
release="${1:-$root/target/release}"
assets="$root/apps/macos/assets"
bundle="$assets/bundle"
logo="$assets/logo"
# Out of Spotlight's reach: an app it indexes is registered with Launch
# Services, which may then start this copy of the input method in place of the
# installed one, as both have the same bundle identifier.
app="$root/target/bundle.noindex/Kanaemi.app"

# The version kanaemi_core::VERSION reports, as the settings app prints it.
version="$("$release/kanaemi-settings" --version)"
plist() {
  perl -pe "s|\@VERSION\@|$version|g" "$1" >"$2"
}

rm -rf "$app"
# A copy assembled where Spotlight looks keeps being found until it is gone.
rm -rf "$root/target/Kanaemi.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
plist "$bundle/Info.plist" "$app/Contents/Info.plist"
cp "$bundle/InfoPlist.strings" "$app/Contents/Resources/"
# The logo's small symbol with the keycap widened to the shape of the other
# input source icons; a template image that macOS tints to match the menu bar.
# Named apart from the app icon: sharing its base name, macOS picks kanaemi.icns
# instead and tints the whole plate into a blank square.
icon="$bundle/input-source-icon.svg"
rsvg-convert -w 22 -h 16 -o "$app/Contents/Resources/input-source.png" "$icon"
rsvg-convert -w 44 -h 32 -o "$app/Contents/Resources/input-source@2x.png" "$icon"
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
# The app icon Finder, the Dock and the privacy settings show, for both apps.
# At 32 pixels and below the logo drawn for small sizes, which stays legible.
icon() {
  local pixels="$1" out="$2" art="$bundle/app-icon.svg"
  if [ "$pixels" -le 32 ]; then
    art="$bundle/app-icon-small.svg"
  fi
  rsvg-convert -w "$pixels" -h "$pixels" -o "$out" "$art"
}
iconset="$(mktemp -d)/kanaemi.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  icon "$size" "$iconset/icon_${size}x${size}.png"
  icon "$((size * 2))" "$iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns -o "$app/Contents/Resources/kanaemi.icns" "$iconset"
cp "$app/Contents/Resources/kanaemi.icns" "$settings/Contents/Resources/kanaemi.icns"
rm -rf "$(dirname "$iconset")"

identity="${KANAEMI_SIGN_IDENTITY:--}"
codesign --force --sign "$identity" "$settings"
codesign --force --sign "$identity" "$app"
