#!/usr/bin/env bash
# Types into a GTK entry through Fcitx5 with Kanaemi and prints what the
# entry holds. Runs headless: Xvfb, its own D-Bus session and Fcitx5.
#
#   e2e/run.sh <xdotool key>...   e.g. e2e/run.sh Shift_R semicolon k a n j i space Return
#
# Needs Kanaemi installed (install.sh), Xvfb, xdotool, and GTK 4 with
# Fcitx5's input module and Python bindings. KANAEMI_E2E_SCREENSHOT names a
# file to save the screen to before the entry is read.
set -u
# On a bus of its own, so a desktop's Fcitx5 is neither replaced nor ended.
if [ -z "${KANAEMI_E2E_BUS:-}" ]; then
  exec env -u WAYLAND_DISPLAY KANAEMI_E2E_BUS=1 dbus-run-session -- "$0" "$@"
fi
export DISPLAY=:99
out=$(mktemp)
# A settings folder of its own, so what is typed here never reaches the
# user dictionary or the record of picks. The settings, dictionaries,
# romaji tables and model are copied from the user's, to convert alike.
home=$(mktemp -d)
mkdir -p "$home/config/kanaemi" "$home/config/fcitx5" "$home/state"
user="${XDG_CONFIG_HOME:-$HOME/.config}/kanaemi"
for name in config.toml dictionaries romaji ranking.model; do
  if [ -e "$user/$name" ]; then cp -R "$user/$name" "$home/config/kanaemi/"; fi
done
# Kanaemi is the one input method, on from the start. Left Shift alone would
# otherwise switch the input method for a moment, as Fcitx5 does by default.
cat >"$home/config/fcitx5/profile" <<'EOF'
[Groups/0]
Name=Default
Default Layout=us
DefaultIM=kanaemi

[Groups/0/Items/0]
Name=keyboard-us
Layout=

[Groups/0/Items/1]
Name=kanaemi
Layout=

[GroupOrder]
0=Default
EOF
cat >"$home/config/fcitx5/config" <<'EOF'
[Hotkey/AltTriggerKeys]

[Behavior]
ActiveByDefault=True
EOF
export XDG_CONFIG_HOME="$home/config" XDG_STATE_HOME="$home/state"
xvfb=
if [ ! -e /tmp/.X11-unix/X99 ]; then
  Xvfb :99 -screen 0 800x600x24 >/dev/null 2>&1 &
  xvfb=$!
  sleep 1
fi
# GDK would otherwise open on a desktop's Wayland display, if one runs.
export GDK_BACKEND=x11 GTK_IM_MODULE=fcitx XMODIFIERS=@im=fcitx
fcitx5 -d --replace >/dev/null 2>&1
sleep 2
python3 "$(dirname "$0")/../../ibus/e2e/app.py" "$out" 12 &
app=$!
sleep 3
# No window manager runs under Xvfb, so the entry is focused by a click.
xdotool mousemove 200 30 click 1
sleep 0.5
echo "input method: $(fcitx5-remote -n)"
# Named by its keysym, Shift_R is pressed together with Shift_L, which is no
# tap; by its keycode in Xvfb's keymap it is pressed alone.
keys=()
for key in "$@"; do
  if [ "$key" = Shift_R ]; then keys+=(62); else keys+=("$key"); fi
done
xdotool key --delay 80 "${keys[@]}"
if [ -n "${KANAEMI_E2E_SCREENSHOT:-}" ]; then
  sleep 0.3
  import -window root "$KANAEMI_E2E_SCREENSHOT"
fi
wait "$app"
echo "text: [$(cat "$out")]"
fcitx5-remote -e >/dev/null 2>&1
rm -f "$out"
rm -rf "$home"
if [ -n "$xvfb" ]; then kill "$xvfb"; fi
