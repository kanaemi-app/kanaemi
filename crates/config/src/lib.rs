//! The user's settings file, and the romaji tables Kanaemi ships with.
//!
//! Both the input method and the settings app read them through this crate,
//! so they agree on what every setting means. The core knows only the romaji
//! table format and how tables stack; the bundled tables and the order they
//! are stacked in by default live here.

mod romaji;

pub use romaji::*;
