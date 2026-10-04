#!/usr/bin/env python3
"""A GTK window with one entry, focused; writes the entry's text to the path
given after the given number of seconds."""

import sys

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import GLib, Gtk  # noqa: E402

out, seconds = sys.argv[1], float(sys.argv[2])


def activate(app):
    win = Gtk.ApplicationWindow(application=app, title="kanaemi-e2e")
    entry = Gtk.Entry()
    win.set_child(entry)
    win.set_default_size(400, 60)
    win.present()
    entry.grab_focus()

    def done():
        with open(out, "w") as f:
            f.write(entry.get_text())
        app.quit()

    GLib.timeout_add(int(seconds * 1000), done)


app = Gtk.Application(application_id="io.github.kanaemi_app.KanaemiE2E")
app.connect("activate", activate)
app.run([])
