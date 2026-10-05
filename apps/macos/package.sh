#!/usr/bin/env bash
# Build Kanaemi.app and package it as an installer package into
# target/package, which installs it into /Library/Input Methods.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
app="$root/target/Kanaemi.app"
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
# the same bundle, such as one in target or in ~/Library/Input Methods.
pkgbuild --analyze --root "$work/root" "$work/components.plist" >/dev/null
plutil -replace 0.BundleIsRelocatable -bool NO "$work/components.plist"

pkgbuild \
  --root "$work/root" \
  --component-plist "$work/components.plist" \
  --scripts "$root/apps/macos/package" \
  --identifier io.github.kanaemi-app.kanaemi \
  --version "$version" \
  --install-location / \
  "$out/Kanaemi-$version.pkg"
