#!/usr/bin/env bash
# Build the Fcitx5 add-on and the settings app, install them under
# /usr/local/lib/kanaemi, and tell Fcitx5 about the add-on. Asks for sudo.
#
# Fcitx5 loads the add-on into its own process, so it is built against the
# Fcitx5 that runs it: run this outside the Nix shell, with Fcitx5's
# development files installed.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
lib=/usr/local/lib/kanaemi
share=/usr/local/share/fcitx5
# Fcitx5 looks for add-on libraries only in its own folder.
addons="$(pkg-config --variable=libdir Fcitx5Core)/fcitx5"

cargo build --release -p kanaemi-fcitx5 -p kanaemi-settings --manifest-path "$root/Cargo.toml"

sudo install -d "$lib" "$share/addon" "$share/inputmethod"
sudo install -m 755 "$root/target/release/libkanaemi_fcitx5.so" "$lib/kanaemi-fcitx5.so"
# The add-on opens the settings app from beside the file its link names.
sudo ln -sf "$lib/kanaemi-fcitx5.so" "$addons/kanaemi.so"
sudo install -m 755 "$root/target/release/kanaemi-settings" "$lib/kanaemi-settings"
sudo install -m 644 "$root/apps/macos/assets/logo/kanaemi-icon.svg" "$lib/kanaemi.svg"
# The version kanaemi_core::VERSION reports, as the settings app prints it.
version="$("$root/target/release/kanaemi-settings" --version)"
for conf in addon inputmethod; do
  filled="$(mktemp)"
  perl -pe "s|\@LIBDIR\@|$lib|g; s|\@VERSION\@|$version|g" "$root/apps/fcitx5/$conf.conf" >"$filled"
  sudo install -m 644 "$filled" "$share/$conf/kanaemi.conf"
  rm -f "$filled"
done

# A running Fcitx5 reads the add-ons again when restarted.
if pgrep -x fcitx5 >/dev/null; then
  fcitx5 -rd >/dev/null 2>&1
fi
echo "installed $lib; add Kanaemi in Fcitx5's configuration"
# Fcitx5 takes a left Shift tap by default to switch input methods for a
# moment, so Kanaemi never sees it. Changing the user's settings is theirs
# to do.
config="${XDG_CONFIG_HOME:-$HOME/.config}/fcitx5/config"
echo "Fcitx5 takes a left Shift tap by default; to let Kanaemi have it, clear"
echo "\"Temporarily switch between first and current Input Method\" in Fcitx5's"
echo "global options, or leave [Hotkey/AltTriggerKeys] empty in $config"
