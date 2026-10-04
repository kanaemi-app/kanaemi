#!/usr/bin/env bash
# Types into a GTK entry through IBus with Kanaemi and prints what the entry
# holds. Runs headless: Xvfb, its own D-Bus session and ibus-daemon.
#
#   e2e/run.sh <xdotool key>...   e.g. e2e/run.sh Shift_R semicolon k a n j i space Return
#
# Needs Kanaemi installed (install.sh), Xvfb, xdotool, and GTK 4
# with its IBus module and Python bindings.
set -u
# On a bus of its own, so a desktop's IBus is neither replaced nor ended.
if [ -z "${KANAEMI_E2E_BUS:-}" ]; then
  exec env -u IBUS_ADDRESS -u WAYLAND_DISPLAY KANAEMI_E2E_BUS=1 dbus-run-session -- "$0" "$@"
fi
export DISPLAY=:99
out=$(mktemp)
# A settings folder of its own, so what is typed here never reaches the
# user dictionary or the record of picks. The settings, dictionaries,
# romaji tables and model are copied from the user's, to convert alike.
home=$(mktemp -d)
mkdir -p "$home/config/kanaemi" "$home/state"
user="${XDG_CONFIG_HOME:-$HOME/.config}/kanaemi"
for name in config.toml dictionaries romaji ranking.model; do
  if [ -e "$user/$name" ]; then cp -R "$user/$name" "$home/config/kanaemi/"; fi
done
export XDG_CONFIG_HOME="$home/config" XDG_STATE_HOME="$home/state"
xvfb=
if [ ! -e /tmp/.X11-unix/X99 ]; then
  Xvfb :99 -screen 0 800x600x24 >/dev/null 2>&1 &
  xvfb=$!
  sleep 1
fi
# GDK would otherwise open on a desktop's Wayland display, if one runs.
export GDK_BACKEND=x11 GTK_IM_MODULE=ibus XMODIFIERS=@im=ibus
ibus-daemon -drx --panel=disable >/dev/null 2>&1
# Its exit status is no guide: on Debian 13 it fails even when the engine is
# set, so asking again would only set it over and over.
for _ in $(seq 1 50); do
  ibus engine kanaemi >/dev/null 2>&1
  [ "$(ibus engine 2>/dev/null)" = kanaemi ] && break
  sleep 0.2
done
echo "engine: $(ibus engine)"
python3 "$(dirname "$0")/app.py" "$out" 12 &
app=$!
sleep 3
# No window manager runs under Xvfb, so the entry is focused by a click.
xdotool mousemove 200 30 click 1
sleep 0.5
# Named by its keysym, Shift_R is pressed together with Shift_L, which is no
# tap; by its keycode in Xvfb's keymap it is pressed alone.
keys=()
for key in "$@"; do
  if [ "$key" = Shift_R ]; then keys+=(62); else keys+=("$key"); fi
done
xdotool key --delay 80 "${keys[@]}"
wait "$app"
echo "text: [$(cat "$out")]"
ibus exit >/dev/null 2>&1
rm -f "$out"
rm -rf "$home"
if [ -n "$xvfb" ]; then kill "$xvfb"; fi
