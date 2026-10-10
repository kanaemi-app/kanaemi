//! What the Linux input methods, the IBus engine and the Fcitx5 add-on, do
//! alike. Both frameworks give keys as X keysyms and show a preedit, a list
//! of candidates and a line of text beside it, so the core's keys and what
//! to show of its output are worked out once here.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use kanaemi_core::Event;
use kanaemi_runtime::{Field, Profile};

use crate::reply::{Reply, Signal};

pub mod keys;
pub mod reply;

/// Ends the focus in `field` and drops what is being typed, for a field the
/// framework no longer delivers text to. A panic starts the field over.
pub fn drop_focus(field: &mut Field, profile: &mut Profile) {
    if catch_unwind(AssertUnwindSafe(|| field.drop_focus(profile))).is_err() {
        tracing::warn!("dropping the focus panicked; the state was reset");
        field.restart(profile);
    }
}

/// The settings app, installed beside the input method.
const SETTINGS_APP: &str = "kanaemi-settings";

/// Opens the settings app installed beside `input_method`, the file the
/// input method runs from.
pub fn open_settings(input_method: &Path) {
    let app = input_method.with_file_name(SETTINGS_APP);
    match std::process::Command::new(&app).spawn() {
        // Waited for on a thread of its own, so no finished app lingers.
        Ok(mut child) => {
            let waiting = std::thread::Builder::new()
                .name("settings-launcher".to_owned())
                .spawn(move || child.wait());
            if let Err(error) = waiting {
                tracing::warn!(%error, "settings app not waited for");
            }
        }
        Err(error) => tracing::warn!(app = %app.display(), %error, "settings app not opened"),
    }
}

/// Feeds one event to `field` and says what to tell the framework, the mode
/// shown by the caret only where the panel shows it well (`indicator`). A
/// panic clears the preedit, starts the field over and hands the key to the
/// application.
pub fn handle(field: &mut Field, profile: &mut Profile, event: Event, indicator: bool) -> Reply {
    match catch_unwind(AssertUnwindSafe(|| field.handle(profile, event))) {
        Ok(output) => {
            tracing::debug!(?event, ?output, "handled");
            let mut reply = reply::reply(&output, indicator);
            // The text is erased by keys forwarded ahead of what follows,
            // which the framework takes without telling whether they arrived.
            if output.erase.is_some() {
                let erased = handle(field, profile, Event::Erased(true), indicator);
                reply.signals.extend(erased.signals);
            }
            reply
        }
        Err(_) => {
            // The event is left out: it may be a key the user typed.
            tracing::warn!("handling an event panicked; the state was reset");
            field.restart(profile);
            Reply {
                consumed: false,
                signals: vec![Signal::Preedit(String::new(), 0), Signal::HideCandidates],
            }
        }
    }
}
