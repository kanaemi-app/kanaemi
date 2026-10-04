//! The input method's state machine.
//!
//! It performs no I/O and answers every call synchronously, so the same
//! sequence of events always produces the same output. That is what lets it
//! run inside any host — an OS input method framework, WebAssembly, a test —
//! without the host's help.

mod config;
mod converter;
mod edit;
mod event;
mod machine;
mod output;
mod romaji;
mod word;

pub use config::*;
pub use converter::*;
pub use event::*;
pub use machine::*;
pub use output::*;
pub use romaji::*;

/// The version of this build: the latest `v` tag as `git describe` gives it,
/// without the `v` (`0.1.0`, or `0.1.0-3-gabc1234` three commits past it).
/// Without such a tag or a Git checkout, the version Cargo records.
pub const VERSION: &str = env!("KANAEMI_VERSION");
