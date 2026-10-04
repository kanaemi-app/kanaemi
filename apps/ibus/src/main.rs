//! The Linux input method, an IBus engine.
//!
//! A program ibus-daemon starts, as the component file `kanaemi.xml` says,
//! which answers on the IBus bus. It needs no C library: IBus is spoken over
//! D-Bus directly.

// What IBus is told builds and is tested on every platform, but only the
// engine on a Unix with IBus sends it.
#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

mod ibus;
mod keys;
mod reply;
#[cfg(all(unix, not(target_os = "macos")))]
mod service;

#[cfg(all(unix, not(target_os = "macos")))]
fn main() {
    kanaemi_runtime::init_logging();
    let Some(dir) = kanaemi_config::dir() else {
        tracing::error!("HOME is not set; no settings or dictionary is kept");
        std::process::exit(1);
    };
    if let Err(error) = service::run(dir) {
        tracing::error!(%error, "IBus not served");
        std::process::exit(1);
    }
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn main() {
    eprintln!("kanaemi-ibus runs only on Linux and other Unix systems with IBus");
    std::process::exit(1);
}
