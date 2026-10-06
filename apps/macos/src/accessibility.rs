//! Accessibility. Posting a key to the application needs it: a key sent in
//! place of the one pressed, and the Backspaces that erase a commit undone.
//! Without it the key pressed goes on as it is, and nothing is undone.
//!
//! As for Input Monitoring, the input menu and the settings app point the
//! way to the permission.

use std::cell::Cell;

use kanaemi_core::Config;

/// The warning the input menu shows.
pub const WARNING: &str = "キーを送るには「アクセシビリティ」の許可が必要です";

/// The Privacy & Security pane that lists the permission.
pub const SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

thread_local! {
    /// Whether the permission was missing when last noted.
    static NOTED: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Whether keys can be posted to the application.
pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Keeps the file that tells the settings app the permission is missing in
/// step with the permission, writing only when it changed.
pub fn note() {
    let missing = !trusted();
    if NOTED.replace(Some(missing)) == Some(missing) {
        return;
    }
    let Some(file) = kanaemi_config::accessibility_missing_file() else {
        return;
    };
    let written = if missing {
        file.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&file, ""))
    } else {
        match std::fs::remove_file(&file) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    };
    if let Err(error) = written {
        tracing::warn!(%error, file = %file.display(), "Accessibility not noted for the settings app");
    }
}

/// Whether keys are bound to be sent but cannot be.
pub fn missing(config: &Config) -> bool {
    config.bindings.sends_keys() && !trusted()
}
