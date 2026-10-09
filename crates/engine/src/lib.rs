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
mod placeholder;
mod ranking;
mod replace;
mod selections;
mod skk;
mod stamp;
mod text_dictionary;
mod user_custom;

pub use binary_dictionary::*;
pub use conjugation::*;
pub use dictionary::*;
pub use engine::*;
pub use okuri::*;
pub use placeholder::*;
pub use ranking::*;
pub use replace::*;
pub use selections::*;
pub use skk::*;
pub use stamp::*;
pub use text_dictionary::*;
pub use user_custom::*;

#[cfg(test)]
mod test_support;
