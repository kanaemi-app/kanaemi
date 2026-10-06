//! One page per group of settings, in the order of the settings file.

mod about;
mod dictionaries;
mod display;
mod input;

use std::cell::RefCell;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use dioxus::prelude::*;
use kanaemi_config::{
    DICTIONARY_DIR, DictionarySource, ROMAJI_DIR, SELECTIONS_FILE, TEXT_EXTENSION, USER_CUSTOM,
    USER_CUSTOM_FILE, binary_name, bundled_romaji_tables, default_romaji_tables, description,
    dictionary_files, dictionary_sources, read_romaji_table,
};
use kanaemi_core::{Config, RomajiTable};
use kanaemi_engine::{
    Dictionary, TextDictionary, mark_placeholders, okuri_lookup, open_dictionary,
    show_placeholders, unhide,
};
use unicode_normalization::UnicodeNormalization;

use crate::checks::invalid_lines;
use crate::controls::{Group, KeyToggle, ListItem, OrderedList, ResetLine, Row};
use crate::convert::{Conversion, conversion, convert, import_skk, is_binary};
use crate::icons::{self, Icon};
use crate::logs;
use crate::official::{self, Catalog, Entry, Status};
use crate::{Ctx, open_folder, open_url};

pub use about::*;
pub use dictionaries::*;
pub use display::*;
pub use input::*;

fn path(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

/// Reads the loaded settings; pages are only shown while there are some.
fn config(ctx: Ctx) -> Config {
    ctx.store
        .read()
        .state
        .as_ref()
        .map(|loaded| loaded.settings.config.clone())
        .unwrap_or_default()
}

/// The files in a folder with one of `extensions`, by name.
fn files_with(dir: &Path, extensions: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter(|name| {
                    Path::new(name)
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| extensions.contains(&e))
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The description on a file's first line, read without reading the rest:
/// a dictionary can be large.
fn file_description(path: &Path) -> Option<String> {
    if is_binary(path) {
        return Some("バイナリの辞書".to_owned());
    }
    let mut first = String::new();
    BufReader::new(fs::File::open(path).ok()?)
        .read_line(&mut first)
        .ok()?;
    description(&first)
}

/// A dictionary file as the settings file names it: its place in the
/// dictionaries folder, sub-folders included.
fn listed_name(folder: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(folder).unwrap_or(path);
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// A dictionary of the list as the settings file names it.
fn source_name(folder: &Path, source: &DictionarySource) -> String {
    match source {
        DictionarySource::UserCustom => USER_CUSTOM.to_owned(),
        DictionarySource::File(path) | DictionarySource::Converted { binary: path, .. } => {
            listed_name(folder, path)
        }
    }
}

/// The dictionaries the IME reads while the settings list none, by name.
fn default_dictionaries(dir: &Path) -> Vec<String> {
    let folder = dir.join(DICTIONARY_DIR);
    dictionary_sources(dir, None)
        .iter()
        .map(|source| source_name(&folder, source))
        .collect()
}

/// A file's size the way a person reads it.
fn file_size(path: &Path) -> Option<String> {
    Some(size_text(fs::metadata(path).ok()?.len()))
}

/// A number of bytes the way a person reads it.
fn size_text(bytes: u64) -> String {
    let bytes = bytes as f64;
    if bytes < 1024.0 {
        format!("{bytes} B")
    } else if bytes < 1024.0 * 1024.0 {
        format!("{:.1} KB", bytes / 1024.0)
    } else {
        format!("{:.1} MB", bytes / 1024.0 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dictionary_in_a_sub_folder_keeps_its_place_in_the_name() {
        let folder = Path::new("/support/dictionaries");
        assert_eq!(
            listed_name(folder, &folder.join("sub").join("a.tsv")),
            "sub/a.tsv"
        );
        assert_eq!(listed_name(folder, &folder.join("a.tsv")), "a.tsv");
    }

    #[test]
    fn without_a_list_the_dictionaries_in_sub_folders_are_chosen_by_name() {
        let dir = std::env::temp_dir().join(format!(
            "kanaemi-settings-pages-{}-default",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let folder = dir.join(DICTIONARY_DIR);
        fs::create_dir_all(folder.join("sub")).unwrap();
        for name in ["sub/a.tsv", "sub/a.kdic", "b.tsv"] {
            fs::write(folder.join(name), "").unwrap();
        }

        let chosen = default_dictionaries(&dir);

        assert_eq!(chosen, [USER_CUSTOM, "b.tsv", "sub/a.kdic"]);
    }
}
