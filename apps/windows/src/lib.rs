//! The Windows input method.
//!
//! A COM DLL that the Text Services Framework loads into every application
//! taking text input, so it runs inside their processes and threads.

// Key translation, popup placement and what the server and the text
// services say to each other build and are tested on every platform, but
// only the programs built for Windows call them.
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod candidates;
#[cfg(windows)]
pub mod com;
pub mod control;
mod focus;
#[cfg(windows)]
mod indicator;
mod keys;
#[cfg(windows)]
pub mod link;
mod per_thread;
#[cfg(windows)]
pub mod pipe;
mod placement;
#[cfg(windows)]
mod popup;
mod registration;
#[cfg(windows)]
mod remote;
#[cfg(windows)]
mod tip;
#[cfg(windows)]
mod ui_element;

/// The log is set up once per process, by the first text service activated.
#[cfg(windows)]
fn init_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(kanaemi_runtime::init_logging);
}
