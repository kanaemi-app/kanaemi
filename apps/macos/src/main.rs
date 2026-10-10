//! The macOS input method.
//!
//! An app of its own under `~/Library/Input Methods`, which macOS starts and
//! talks to through Input Method Kit.

// Key translation builds and is tested on every platform, but only the macOS
// app calls it.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

#[cfg(target_os = "macos")]
mod accessibility;
#[cfg(target_os = "macos")]
mod app;
#[cfg(target_os = "macos")]
mod candidate_window;
mod candidates;
mod handled;
#[cfg(target_os = "macos")]
mod indicator;
#[cfg(target_os = "macos")]
mod input_monitoring;
#[cfg(target_os = "macos")]
mod key_tap;
mod keys;
#[cfg(target_os = "macos")]
mod meaning;
mod posted;
#[cfg(target_os = "macos")]
mod secure_input;

#[cfg(target_os = "macos")]
fn main() {
    app::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("kanaemi-macos runs only on macOS");
    std::process::exit(1);
}
