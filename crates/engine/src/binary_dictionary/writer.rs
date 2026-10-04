//! Writes a text dictionary in the binary format, so tests can open what a
//! dictionary builder would ship.

use std::collections::HashMap;
use std::ops::Range;

use fst::MapBuilder;

use super::{
    BINARY_MAGIC, ENTRIES, FORMAT_VERSION, HEADER_LEN, INDEX, OKURI, SECTION_ENTRY_LEN, STRINGS,
};
use crate::{Dictionary, Entry, TextDictionary, okuri_key};

const ALIGN: usize = 64;

/// The text dictionary in the binary format. Lines the format cannot hold,
/// such as a string over 65,535 bytes, are left out.
pub(crate) fn encode(text: &TextDictionary) -> Vec<u8> {
    let mut writer = Writer::default();
    let mut readings: Vec<&str> = text.readings().collect();
    readings.sort_unstable();
    let index: Vec<(String, Range<u32>)> = readings
        .into_iter()
        .map(|reading| {
            // Words first, then stems, each cheapest first.
            let (words, stems): (Vec<Entry>, Vec<Entry>) = text
                .lookup(reading)
                .into_iter()
                .partition(|e| e.conjugation.is_none());
            let range = writer.entries(words.into_iter().chain(stems));
            (reading.to_owned(), range)
        })
        .collect();
    let mut keys: Vec<(&str, char)> = text.okuri_keys().collect();
    keys.sort_unstable_by_key(|&(stem, row)| okuri_key(stem, row));
    let okuri: Vec<(String, Range<u32>)> = keys
        .into_iter()
        .map(|(stem, row)| {
            let range = writer.entries(text.okuri(stem, row).into_iter());
            (okuri_key(stem, row), range)
        })
        .collect();
    writer.finish(&index, &okuri)
}

#[derive(Default)]
struct Writer {
    strings: Vec<u8>,
    offsets: HashMap<String, u32>,
    entries: Vec<u8>,
    count: u32,
}

impl Writer {
    /// Adds the entries of one key and returns their numbers.
    fn entries(&mut self, found: impl Iterator<Item = Entry>) -> Range<u32> {
        let first = self.count;
        let fits = |s: &str| s.len() <= u16::MAX as usize;
        for entry in found {
            if self.count - first == u16::MAX as u32 {
                break;
            }
            let fields = [Some(entry.surface.as_str()), entry.conjugation.as_deref()];
            if !fields.into_iter().flatten().all(fits) {
                continue;
            }
            for field in fields {
                let at = field.map_or(0, |s| self.string(s));
                self.entries.extend_from_slice(&at.to_le_bytes());
            }
            self.entries.extend_from_slice(&entry.cost.to_le_bytes());
            self.count += 1;
        }
        first..self.count
    }

    fn string(&mut self, s: &str) -> u32 {
        if self.strings.is_empty() {
            self.strings.extend_from_slice(&0u16.to_le_bytes());
        }
        if s.is_empty() {
            return 0;
        }
        if let Some(&at) = self.offsets.get(s) {
            return at;
        }
        let at = self.strings.len() as u32;
        self.strings
            .extend_from_slice(&(s.len() as u16).to_le_bytes());
        self.strings.extend_from_slice(s.as_bytes());
        self.offsets.insert(s.to_owned(), at);
        at
    }

    fn finish(mut self, index: &[(String, Range<u32>)], okuri: &[(String, Range<u32>)]) -> Vec<u8> {
        self.string("");
        let sections = [
            (STRINGS, self.strings),
            (INDEX, fst_map(index)),
            (ENTRIES, self.entries),
            (OKURI, fst_map(okuri)),
        ];
        let mut out = Vec::new();
        out.extend_from_slice(BINARY_MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&(sections.len() as u32).to_le_bytes());
        out.resize(HEADER_LEN, 0);
        let mut offset = align(HEADER_LEN + SECTION_ENTRY_LEN * sections.len());
        for (kind, contents) in &sections {
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&(offset as u64).to_le_bytes());
            out.extend_from_slice(&(contents.len() as u64).to_le_bytes());
            out.extend_from_slice(&xxhash_rust::xxh3::xxh3_64(contents).to_le_bytes());
            offset = align(offset + contents.len());
        }
        for (_, contents) in &sections {
            out.resize(align(out.len()), 0);
            out.extend_from_slice(contents);
        }
        out
    }
}

/// `keys` must be sorted by their bytes.
fn fst_map(keys: &[(String, Range<u32>)]) -> Vec<u8> {
    let mut builder = MapBuilder::memory();
    for (key, range) in keys {
        if range.is_empty() {
            continue;
        }
        let value = u64::from(range.start) | (u64::from(range.end - range.start) << 32);
        builder
            .insert(key, value)
            .expect("keys are sorted and unique");
    }
    builder.into_inner().expect("writing to memory cannot fail")
}

fn align(n: usize) -> usize {
    n.div_ceil(ALIGN) * ALIGN
}
