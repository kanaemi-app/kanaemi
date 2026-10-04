//! The user's settings file, the folder it lives in, and the romaji tables
//! Kanaemi ships with.
//!
//! Both the input method and the settings app read them through this crate,
//! so they agree on what every setting means. The core knows only the romaji
//! table format and how tables stack; the bundled tables and the order they
//! are stacked in by default live here.

mod edit;
mod folder;
mod keys;
mod romaji;
mod settings;

pub use edit::*;
pub use folder::*;
pub use keys::*;
pub use romaji::*;
pub use settings::*;
pub use toml_edit::Value;
