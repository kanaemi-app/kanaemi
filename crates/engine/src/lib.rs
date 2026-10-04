//! Conversion from readings to candidates.
//!
//! It looks a reading up in the user's dictionaries and orders what it finds,
//! so the core can stay free of files and of ranking.

mod binary_dictionary;
mod conjugation;
mod dictionary;
mod engine;
mod numeric;
mod okuri;
mod ranking;
mod selections;
mod text_dictionary;
mod user_custom;

pub use binary_dictionary::*;
pub use conjugation::*;
pub use dictionary::*;
pub use engine::*;
pub use ranking::*;
pub use selections::*;
pub use text_dictionary::*;
pub use user_custom::*;

use okuri::*;

#[cfg(test)]
mod test_support;
