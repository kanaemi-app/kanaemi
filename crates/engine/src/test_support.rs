//! Helpers the unit tests share.

use std::io;

use proptest::collection::vec;
use proptest::prelude::*;

use crate::{Dictionary, Entry, LineSink};

/// Words that do not conjugate, as a dictionary lists them for a reading.
pub(crate) trait Words {
    fn words(&self, reading: &str) -> Vec<Entry>;
}

impl<D: Dictionary + ?Sized> Words for D {
    fn words(&self, reading: &str) -> Vec<Entry> {
        self.lookup(reading)
            .into_iter()
            .filter(|e| e.conjugation.is_none())
            .collect()
    }
}

/// The conjugating items of `key`, as [`Dictionary::lookup`] finds them.
pub(crate) fn stems(dictionary: &impl Dictionary, key: &str) -> Vec<Entry> {
    dictionary
        .lookup(key)
        .into_iter()
        .filter(|e| e.conjugation.is_some())
        .collect()
}

/// An item of a word that does not conjugate.
pub(crate) fn entry(surface: &str, cost: u32) -> Entry {
    Entry {
        surface: surface.to_owned(),
        conjugation: None,
        cost,
    }
}

/// Writes nothing, for an engine whose registrations need not last.
pub(crate) struct Discard;

impl LineSink for Discard {
    fn append(&mut self, _line: &str) -> io::Result<()> {
        Ok(())
    }
}

/// Pieces of a dictionary line that the format gives a meaning to, so text
/// built from them reaches far more of a parser than arbitrary text does.
#[rustfmt::skip]
const PIECES: &[&str] = &[
    // Readings and surfaces.
    "か", "き", "しゃ", "っ", "く", "ア", "記者", "書", "勝", "a", "k", "1", " ", "e\u{301}",
    // Separators and marks.
    "\t", "\t\t", "\r", "*", "!", "#", "\u{FEFF}", "\u{1F}", "\u{FFFD}",
    // Escapes, whole and broken.
    "\\", "\\\\", "\\t", "\\n", "\\*", "\\#", "\\!", "\\{", "\\}",
    // Placeholders, whole and broken.
    "{", "}", "{}", "{kanji}", "{1:wide-num}", "{-:x y}", "{0:}", "\u{FDD0}", "\u{FDD1}",
    // Conjugation types and costs.
    "五段-カ行", "下一段-カ行", "サ行変格", "0", "7", "4294967295", "4294967296", "-1",
];

/// Text made of lines of [`PIECES`], with an arbitrary line now and then.
pub(crate) fn dictionary_text() -> impl Strategy<Value = String> {
    let piece = proptest::sample::select(PIECES);
    let line = prop_oneof![
        9 => vec(piece, 0..8).prop_map(|pieces| pieces.concat()),
        1 => any::<String>(),
    ];
    vec(line, 0..12).prop_map(|lines| lines.join("\n"))
}
