#!/bin/sh
# IBus lists the engines from a cache of the component files; a running
# ibus-daemon reads it again when restarted.
if command -v ibus >/dev/null 2>&1; then
  ibus write-cache --system >/dev/null 2>&1 || true
fi
