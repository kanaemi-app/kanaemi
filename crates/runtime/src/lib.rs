//! What every platform's input method does alike: opening the dictionaries
//! and the ranking model, reading the settings and reading them again when
//! they change, writing the log, and keeping each field's state on the
//! profile every field shares. A platform's input method keeps only what
//! talks to its own input method framework.

mod dictionaries;
mod field;
mod logging;
mod profile;
mod settings;

pub use field::*;
pub use logging::*;
pub use profile::*;
