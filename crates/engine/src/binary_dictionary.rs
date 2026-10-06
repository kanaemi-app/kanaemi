use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::ops::Range;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::Arc;

use fst::raw::{CompiledAddr, Fst};
use fst::{IntoStreamer, Map, Streamer};
use memmap2::Mmap;

use crate::{Dictionary, Entry, InvalidLine, TextDictionary, okuri_key};

#[cfg(test)]
mod tests;
pub(crate) mod writer;

/// The first bytes of every binary dictionary file.
pub(crate) const BINARY_MAGIC: &[u8; 8] = b"KANAEMID";

const FORMAT_VERSION: u32 = 1;
const HEADER_LEN: usize = 32;
const SECTION_ENTRY_LEN: usize = 32;
const ENTRY_LEN: usize = 12;

const STRINGS: u32 = 1;
const INDEX: u32 = 2;
const ENTRIES: u32 = 3;
const OKURI: u32 = 4;
const SOURCE: u32 = 5;

#[derive(Debug, thiserror::Error)]
pub enum BinaryError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("not a Kanaemi binary dictionary")]
    Magic,
    #[error("format version {0} is not supported")]
    Version(u32),
    #[error("the header is malformed: {0}")]
    Header(&'static str),
    #[error("section {0} is missing")]
    Missing(u32),
    #[error("section {0} appears twice")]
    Doubled(u32),
    #[error("section {0} lies outside the file")]
    OutOfFile(u32),
    #[error("section {kind} is malformed: {reason}")]
    Malformed { kind: u32, reason: &'static str },
    #[error("section {0} does not match its checksum")]
    Checksum(u32),
}

type Bytes = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// One section of the file, for an FST to read in place.
#[derive(Clone)]
struct Slice {
    bytes: Bytes,
    range: Range<usize>,
}

impl AsRef<[u8]> for Slice {
    fn as_ref(&self) -> &[u8] {
        &(*self.bytes).as_ref()[self.range.clone()]
    }
}

struct Section {
    kind: u32,
    range: Range<usize>,
    checksum: u64,
}

/// A dictionary in the binary format, read in place. Every reference in it is
/// checked on opening, so a lookup never fails.
pub struct BinaryDictionary {
    bytes: Bytes,
    sections: Vec<Section>,
    strings: Range<usize>,
    entries: Range<usize>,
    index: Map<Slice>,
    okuri: Option<Map<Slice>>,
}

/// One entry as the file holds it.
struct Raw {
    surface: u32,
    conj: u32,
    cost: u32,
}

impl BinaryDictionary {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, BinaryError> {
        let file = File::open(path)?;
        if file.metadata()?.len() < HEADER_LEN as u64 {
            return Err(BinaryError::Magic);
        }
        // SAFETY: a binary dictionary is never rewritten in place; a new one
        // is written beside it and renamed over it, which leaves this mapping
        // on the old file.
        let map = unsafe { Mmap::map(&file)? };
        Self::parse(Arc::new(map))
    }

    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self, BinaryError> {
        Self::parse(Arc::new(bytes.into()))
    }

    /// Checks every section against its checksum; opening does not, to stay
    /// fast.
    pub fn verify_checksums(&self) -> Result<(), BinaryError> {
        let bytes = (*self.bytes).as_ref();
        for section in &self.sections {
            if xxhash_rust::xxh3::xxh3_64(&bytes[section.range.clone()]) != section.checksum {
                return Err(BinaryError::Checksum(section.kind));
            }
        }
        Ok(())
    }

    /// The SHA-256 of the text dictionary this one was converted from, if it
    /// holds one.
    pub fn source_digest(&self) -> Option<[u8; 32]> {
        let section = self.sections.iter().find(|s| s.kind == SOURCE)?;
        (*self.bytes).as_ref()[section.range.clone()]
            .try_into()
            .ok()
    }

    fn parse(bytes: Bytes) -> Result<Self, BinaryError> {
        let b = (*bytes).as_ref();
        if b.len() < HEADER_LEN || &b[..8] != BINARY_MAGIC {
            return Err(BinaryError::Magic);
        }
        let version = u32_at(b, 8);
        if version != FORMAT_VERSION {
            return Err(BinaryError::Version(version));
        }
        if b[16..HEADER_LEN].iter().any(|&byte| byte != 0) {
            return Err(BinaryError::Header("a reserved field is not zero"));
        }
        let count = u32_at(b, 12) as usize;
        let table_end = count
            .checked_mul(SECTION_ENTRY_LEN)
            .and_then(|n| n.checked_add(HEADER_LEN))
            .filter(|&end| end <= b.len())
            .ok_or(BinaryError::Header(
                "the section table lies outside the file",
            ))?;
        let mut sections: Vec<Section> = Vec::with_capacity(count);
        for at in (HEADER_LEN..table_end).step_by(SECTION_ENTRY_LEN) {
            let kind = u32_at(b, at);
            if u32_at(b, at + 4) != 0 {
                return Err(BinaryError::Header("a reserved field is not zero"));
            }
            let range = usize::try_from(u64_at(b, at + 8))
                .ok()
                .zip(usize::try_from(u64_at(b, at + 16)).ok())
                .and_then(|(offset, len)| Some(offset..offset.checked_add(len)?))
                .filter(|range| range.end <= b.len())
                .ok_or(BinaryError::OutOfFile(kind))?;
            if [STRINGS, INDEX, ENTRIES, OKURI, SOURCE].contains(&kind)
                && sections.iter().any(|s| s.kind == kind)
            {
                return Err(BinaryError::Doubled(kind));
            }
            sections.push(Section {
                kind,
                range,
                checksum: u64_at(b, at + 24),
            });
        }
        let find = |kind| {
            sections
                .iter()
                .find(|s| s.kind == kind)
                .map(|s| s.range.clone())
        };
        if find(SOURCE).is_some_and(|range| range.len() != 32) {
            return Err(BinaryError::Malformed {
                kind: SOURCE,
                reason: "not a SHA-256",
            });
        }
        let required = |kind| find(kind).ok_or(BinaryError::Missing(kind));
        let strings = required(STRINGS)?;
        let entries = required(ENTRIES)?;
        let fst = |range: Range<usize>, kind| {
            Map::new(Slice {
                bytes: bytes.clone(),
                range,
            })
            .map_err(|_| BinaryError::Malformed {
                kind,
                reason: "not an FST map",
            })
        };
        let index = fst(required(INDEX)?, INDEX)?;
        let okuri = find(OKURI).map(|range| fst(range, OKURI)).transpose()?;
        let dictionary = Self {
            bytes: bytes.clone(),
            sections,
            strings,
            entries,
            index,
            okuri,
        };
        dictionary.check()?;
        Ok(dictionary)
    }

    fn check(&self) -> Result<(), BinaryError> {
        let malformed = |kind, reason| BinaryError::Malformed { kind, reason };
        if !self.entries.len().is_multiple_of(ENTRY_LEN) {
            return Err(malformed(ENTRIES, "not a whole number of entries"));
        }
        if self.string(0) != Some("") {
            return Err(malformed(STRINGS, "no empty string at 0"));
        }
        for i in 0..self.entry_count() {
            let raw = self.raw(i);
            // An okurigana entry's surface may be empty: く for KaKu.
            let strings_ok = [raw.surface, raw.conj]
                .into_iter()
                .all(|at| self.string(at).is_some());
            if !strings_ok {
                return Err(malformed(ENTRIES, "a string reference is broken"));
            }
        }
        for (map, kind) in [(Some(&self.index), INDEX), (self.okuri.as_ref(), OKURI)] {
            let Some(map) = map else { continue };
            // The fst crate trusts the bytes it walks and panics on a broken
            // node. Walking every node once here leaves no node for a later
            // lookup to break on. A small FST can spell an endless number of
            // paths, so the nodes are checked first, each once: when every
            // node leads to a key, walking the keys is bounded by their count.
            // Each key owns at least one entry, so a map with more keys than
            // entries is broken, and the walk stops there.
            let walked = std::panic::catch_unwind(AssertUnwindSafe(|| {
                if !every_node_leads_to_a_key(map.as_fst()) {
                    return Err(malformed(kind, "a node leads to no key"));
                }
                let mut stream = map.stream();
                let mut ranges = Vec::new();
                while let Some((_, value)) = stream.next() {
                    let range = unpack(value);
                    if value >> 48 != 0 || range.is_empty() || range.end > self.entry_count() {
                        return Err(malformed(kind, "a range lies outside the entries"));
                    }
                    ranges.push(range);
                    if ranges.len() > self.entry_count() {
                        return Err(malformed(kind, "more keys than entries"));
                    }
                }
                ranges.sort_unstable_by_key(|r| r.start);
                if ranges.windows(2).any(|pair| pair[0].end > pair[1].start) {
                    return Err(malformed(kind, "the ranges of two keys overlap"));
                }
                Ok(())
            }));
            walked.unwrap_or(Err(malformed(kind, "a broken FST node")))?;
        }
        Ok(())
    }

    fn entry_count(&self) -> usize {
        self.entries.len() / ENTRY_LEN
    }

    fn raw(&self, i: usize) -> Raw {
        let at = self.entries.start + i * ENTRY_LEN;
        let e = &(*self.bytes).as_ref()[at..at + ENTRY_LEN];
        Raw {
            surface: u32_at(e, 0),
            conj: u32_at(e, 4),
            cost: u32_at(e, 8),
        }
    }

    fn string(&self, at: u32) -> Option<&str> {
        let strings = &(*self.bytes).as_ref()[self.strings.clone()];
        let at = at as usize;
        let len = u16::from_le_bytes(strings.get(at..at + 2)?.try_into().ok()?) as usize;
        std::str::from_utf8(strings.get(at + 2..at + 2 + len)?).ok()
    }

    fn found(&self, map: Option<&Map<Slice>>, key: &str) -> Vec<Entry> {
        map.and_then(|map| map.get(key))
            .map(unpack)
            .unwrap_or_default()
            .map(|i| self.entry(&self.raw(i)))
            .collect()
    }

    fn entry(&self, raw: &Raw) -> Entry {
        let text = |at: u32| (at != 0).then(|| self.string(at).unwrap_or_default().to_owned());
        Entry {
            surface: self.string(raw.surface).unwrap_or_default().to_owned(),
            conjugation: text(raw.conj),
            cost: raw.cost,
        }
    }
}

impl Dictionary for BinaryDictionary {
    fn lookup(&self, key: &str) -> Vec<Entry> {
        self.found(Some(&self.index), key)
    }

    fn okuri(&self, stem: &str, row: char) -> Vec<Entry> {
        self.found(self.okuri.as_ref(), &okuri_key(stem, row))
    }

    fn readings_from(&self, prefix: &str, limit: usize) -> Vec<String> {
        let mut stream = self.index.range().ge(prefix).into_stream();
        let mut readings = Vec::new();
        while readings.len() < limit
            && let Some((key, _)) = stream.next()
        {
            match std::str::from_utf8(key) {
                Ok(reading) if reading.starts_with(prefix) => readings.push(reading.to_owned()),
                _ => break,
            }
        }
        readings
    }
}

/// Converts a text dictionary into a binary one, which remembers the
/// [`text_digest`] of `text` so it tells whether the text changed since. The
/// lines of `text` that could not be read are returned with it.
pub fn convert_text(text: impl AsRef<[u8]>) -> (Vec<u8>, Vec<InvalidLine>) {
    let text = text.as_ref();
    let (dictionary, invalid) = TextDictionary::parse(text);
    let bytes = writer::encode(&dictionary, Some(text_digest(text)));
    (bytes, invalid)
}

/// The SHA-256 of a text dictionary's bytes.
pub fn text_digest(bytes: impl AsRef<[u8]>) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(bytes.as_ref()).into()
}

/// Whether every node below the root leads to a key; the root alone may not,
/// in a map without keys. A transition must point below its node, as the fst
/// crate writes them: a forged delta can wrap around to the node itself or
/// above it where subtraction is not checked, and a walk would never end.
/// With every transition pointing down, the nodes form no cycle, and each is
/// looked at once.
fn every_node_leads_to_a_key(fst: &Fst<Slice>) -> bool {
    let mut leading: HashSet<CompiledAddr> = HashSet::new();
    let root = fst.root();
    // The nodes from the root down, each with the next transition to follow
    // and whether it was seen to lead to a key.
    let mut path = vec![(root, 0, root.is_final())];
    while let Some(top) = path.last_mut() {
        if top.1 < top.0.len() {
            let child = top.0.transition_addr(top.1);
            top.1 += 1;
            if child >= top.0.addr() {
                return false;
            }
            if leading.contains(&child) {
                top.2 = true;
            } else {
                let child = fst.node(child);
                path.push((child, 0, child.is_final()));
            }
            continue;
        }
        let (node, _, leads) = path.pop().expect("the loop saw a node");
        let Some(parent) = path.last_mut() else {
            break;
        };
        if !leads {
            return false;
        }
        leading.insert(node.addr());
        parent.2 = true;
    }
    true
}

fn unpack(value: u64) -> Range<usize> {
    let first = (value & 0xffff_ffff) as usize;
    let count = ((value >> 32) & 0xffff) as usize;
    first..first + count
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"))
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("eight bytes"))
}
