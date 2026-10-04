//! Helpers the unit tests share.

use std::io;

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
