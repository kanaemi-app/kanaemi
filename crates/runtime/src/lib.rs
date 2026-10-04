//! What every platform's input method does alike: opening the dictionaries
//! and the ranking model, reading the settings and reading them again when
//! they change, writing the log, and keeping each field's state on the
//! profile every field shares. A platform's input method keeps only what
//! talks to its own input method framework.

mod control;
mod dictionaries;
mod field;
mod logging;
mod profile;
mod settings;
mod shared;

pub use control::*;
pub use field::*;
pub use logging::*;
pub use profile::*;
// A sandboxed profile takes one, so a platform needs no other crate to make it.
pub use kanaemi_engine::LineSink;
