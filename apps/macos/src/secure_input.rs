//! Secure Event Input. While any process holds it, macOS hands keys past
//! every input method, yet the input method stays selected and is still
//! activated: typing comes out unconverted with no sign of why. A process that
//! fails to release it, such as a terminal after a password prompt, leaves
//! every application like that until it quits, so the holder is named in the
//! log and in the input menu.

use std::cell::Cell;

use objc2::rc::Retained;
use objc2_app_kit::NSRunningApplication;
use objc2_core_graphics::CGSessionCopyCurrentDictionary;
use objc2_foundation::{NSDictionary, NSNumber, NSString};

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> bool;
}

/// The process holding Secure Event Input, as far as it can be told.
pub struct Holder {
    pid: Option<i32>,
    name: Option<String>,
}

impl Holder {
    /// The warning the input menu shows.
    pub fn label(&self) -> String {
        let who = match (&self.name, self.pid) {
            (Some(name), _) => format!("「{name}」"),
            (None, Some(pid)) => format!("PID {pid} のプロセス"),
            (None, None) => "ほかのアプリ".to_owned(),
        };
        format!("{who}が Secure Input を有効にしているため、日本語を入力できません")
    }
}

thread_local! {
    /// The holder last logged, so a stuck one is logged once, not on every
    /// focus change.
    static LOGGED: Cell<Option<Option<i32>>> = const { Cell::new(None) };
}

/// Returns the holder of Secure Event Input, logging when it changes.
pub fn check() -> Option<Holder> {
    let holder = unsafe { IsSecureEventInputEnabled() }.then(|| {
        let pid = holder_pid();
        Holder {
            pid,
            name: pid.and_then(app_name),
        }
    });
    let pid = holder.as_ref().map(|holder| holder.pid);
    if LOGGED.replace(pid) != pid {
        match &holder {
            Some(holder) => tracing::warn!(
                pid = holder.pid,
                app = holder.name.as_deref(),
                "secure input is enabled; keys pass the input method until the holder releases it"
            ),
            None => tracing::info!("secure input is released"),
        }
    }
    holder
}

/// The session dictionary is what `ioreg` shows under IOConsoleUsers; it
/// names the holder's process.
fn holder_pid() -> Option<i32> {
    let session = CGSessionCopyCurrentDictionary()?;
    let session: *const _ = &*session;
    // SAFETY: CFDictionary is toll-free bridged to NSDictionary, and `session`
    // outlives the borrow.
    let session: &NSDictionary = unsafe { &*session.cast() };
    let key = NSString::from_str("kCGSSessionSecureInputPID");
    let pid = session.objectForKey(&key)?;
    let pid: Retained<NSNumber> = pid.downcast().ok()?;
    Some(pid.as_i32())
}

fn app_name(pid: i32) -> Option<String> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?
        .localizedName()
        .map(|name| name.to_string())
}
