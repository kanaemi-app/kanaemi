//! The input method's state machine.
//!
//! It performs no I/O and answers every call synchronously, so the same
//! sequence of events always produces the same output. That is what lets it
//! run inside any host — an OS input method framework, a test — without the
//! host's help.

/// The version of this crate, as Cargo records it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
