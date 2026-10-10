#!/usr/bin/env bash
# Build Kanaemi.app and package it as an installer package into
# target/package, which installs it into ~/Library/Input Methods of the user
# who runs it.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
app="$root/target/bundle.noindex/Kanaemi.app"
out="$root/target/package"

cargo build --release -p kanaemi-macos -p kanaemi-settings --manifest-path "$root/Cargo.toml"
"$root/apps/macos/bundle.sh"

# A build in the Nix development shell links libraries from /nix/store,
# which other Macs do not have.
for binary in "$app/Contents/MacOS/kanaemi" "$app/Contents/Resources/KanaemiSettings.app/Contents/MacOS/kanaemi-settings"; do
  if otool -L "$binary" | grep /nix/store >/dev/null; then
    echo "$binary links libraries from /nix/store; build outside the Nix shell" >&2
    exit 1
  fi
done

version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/root/Library/Input Methods" "$out"
cp -R "$app" "$work/root/Library/Input Methods/"

# The installer would otherwise put the app where it finds another copy of
# the same bundle, such as one in target.
pkgbuild --analyze --root "$work/root" "$work/components.plist" >/dev/null
plutil -replace 0.BundleIsRelocatable -bool NO "$work/components.plist"

pkgbuild \
  --root "$work/root" \
  --component-plist "$work/components.plist" \
  --scripts "$root/apps/macos/package" \
  --identifier io.github.kanaemi-app.kanaemi \
  --version "$version" \
  --install-location / \
  "$work/kanaemi.pkg"

# Only the user's home is offered: an input method needs nothing outside it,
# so the installer asks for no administrator and runs the scripts as the user.
# A copy under /Library as well would be shadowed by the one in the home.
cat >"$work/distribution.xml" <<XML
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>Kanaemi</title>
    <domains enable_anywhere="false" enable_currentUserHome="true" enable_localSystem="false"/>
    <options customize="never" require-scripts="false" hostArchitectures="$(uname -m)"/>
    <choices-outline>
        <line choice="kanaemi"/>
    </choices-outline>
    <choice id="kanaemi" visible="false">
        <pkg-ref id="io.github.kanaemi-app.kanaemi"/>
    </choice>
    <pkg-ref id="io.github.kanaemi-app.kanaemi" version="$version" onConclusion="none">kanaemi.pkg</pkg-ref>
</installer-gui-script>
XML

productbuild \
  --distribution "$work/distribution.xml" \
  --package-path "$work" \
  "$out/Kanaemi-$version.pkg"

# The bare app for the Homebrew cask, which moves it into ~/Library/Input
# Methods itself: the installer package cannot serve it, since a cask installs
# a package for the whole system. ditto keeps the signature intact. Unlike
# the other installers, a zip does not tell the OS by its extension, so the
# name does.
ditto -c -k --keepParent "$app" "$out/Kanaemi-$version-macos-$(uname -m).zip"
