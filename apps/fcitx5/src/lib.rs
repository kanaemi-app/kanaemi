//! The Linux input method, a Fcitx5 add-on.
//!
//! Fcitx5 loads add-ons as shared libraries into its own process and talks
//! to them in C++. A thin C++ layer (`addon.cpp`) is the engine Fcitx5 sees,
//! and hands each event to this library over a C interface; what to show
//! comes back the same way.

// What Fcitx5 is told builds and is tested on every platform, but only the
// add-on on a Unix with Fcitx5 tells it.
#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

mod content;
#[cfg(all(unix, not(target_os = "macos")))]
mod ffi;
mod repeat;
#[cfg(all(unix, not(target_os = "macos")))]
mod shell;
mod show;
