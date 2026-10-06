//! Input Monitoring. Input Method Kit passes on no key's release, so a key
//! bound to be held is let go only as the keyboard's state tells, and macOS
//! hides the state of the keys that type characters from a process without
//! this permission: they read as up at once, and every press is alone.
//!
//! The permission is not asked for: from Kanaemi, even started as an
//! ordinary app, neither `CGRequestListenEventAccess`, `IOHIDRequestAccess`
//! nor a listening event tap prompts or lists it in the settings. The input
//! menu and the settings app point the way instead.

use kanaemi_core::Config;
use objc2_core_graphics::CGPreflightListenEventAccess;

/// The warning the input menu shows.
pub const WARNING: &str = "押さえたままのキーを使うには「入力監視」の許可が必要です";

/// The Privacy & Security pane that lists the permission.
pub const SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent";

/// Whether a key is bound to be held but its release cannot be told.
pub fn missing(config: &Config) -> bool {
    config.bindings.hold_a_key() && !CGPreflightListenEventAccess()
}
