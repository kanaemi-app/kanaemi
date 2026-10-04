use std::collections::{HashMap, HashSet};

use unicode_normalization::UnicodeNormalization;

use crate::numeric::{CLOSE, Notation, OPEN};
use crate::{ConjugationTable, Dictionary, Entry, okuri_row};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InvalidReason {
    #[error("the line is not UTF-8")]
    Encoding,
    #[error("the line has too few or too many fields")]
    FieldCount,
    #[error("the reading or the surface is empty")]
    Empty,
    #[error("a backslash escapes nothing it can")]
    Escape,
    #[error("the okurigana is not marked as the format allows")]
    Okurigana,
    /// A `!` line outside the user custom dictionary, or one that cannot hide.
    #[error("a hide line is not allowed here")]
    Hide,
    #[error("the conjugation type is unknown")]
    ConjugationType,
    #[error("the cost is not an integer from 0 to 4294967295")]
    Cost,
    /// Includes a numeric line that conjugates or has okurigana, which the
    /// format does not allow.
    #[error("a number placeholder is not written as the format allows")]
    Placeholder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidLine {
    /// 1-based.
    pub line: usize,
    pub reason: InvalidReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Item {
    surface: String,
    conjugation: Option<String>,
    /// Without one, the item costs its place in its group.
    cost: Option<u32>,
}

enum Record {
    Word {
        reading: String,
        item: Item,
    },
    Okuri {
        stem: String,
        kana: String,
        item: Item,
    },
    Hide {
        reading: String,
        surface: String,
    },
}

/// One item line of a text dictionary. It is written with columns up to its
/// last given field, so a plain word stays two columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemLine<'a> {
    /// The reading before any okurigana; a `*` in it is a literal `*`.
    pub reading: &'a str,
    /// The first kana of the okurigana, written after the `*` that marks it.
    pub okurigana: Option<&'a str>,
    pub surface: &'a str,
    pub conjugation: Option<&'a str>,
    pub cost: Option<u32>,
}

impl std::fmt::Display for ItemLine<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", escape_literal_reading(self.reading))?;
        if let Some(kana) = self.okurigana {
            write!(f, "*{}", escape(kana))?;
        }
        write!(f, "\t{}", escape_surface(self.surface))?;
        let columns = if self.cost.is_some() {
            2
        } else {
            usize::from(self.conjugation.is_some())
        };
        let optional = [
            self.conjugation.map(escape),
            self.cost.map(|c| c.to_string()),
        ];
        for field in optional.into_iter().take(columns) {
            write!(f, "\t{}", field.unwrap_or_default())?;
        }
        Ok(())
    }
}

/// A dictionary in the hand-editable text format. Groups keep the last-written
/// line first, so an item's cost is its position in its group.
#[derive(Clone, Debug, Default)]
pub struct TextDictionary {
    user_custom: bool,
    words: HashMap<String, Vec<Item>>,
    okuri: HashMap<(String, String), Vec<Item>>,
    okuri_kana: HashMap<String, Vec<String>>,
    hidden: HashSet<(String, String)>,
}

impl TextDictionary {
    /// A dictionary in the text format, with the lines it could not read. A
    /// line that is not UTF-8 is one such line.
    pub fn parse(text: impl AsRef<[u8]>) -> (Self, Vec<InvalidLine>) {
        Self::parse_bytes(text.as_ref(), false)
    }

    /// A user custom dictionary: the text format, where `!` lines also hide
    /// pairs from every dictionary.
    pub fn parse_user_custom(text: impl AsRef<str>) -> (Self, Vec<InvalidLine>) {
        Self::parse_bytes(text.as_ref().as_bytes(), true)
    }

    /// Each line is decoded on its own, so a line that is not UTF-8 is one
    /// invalid line rather than a file that cannot be read.
    pub(crate) fn parse_bytes(bytes: &[u8], user_custom: bool) -> (Self, Vec<InvalidLine>) {
        let mut dictionary = Self {
            user_custom,
            ..Self::default()
        };
        let mut invalid = Vec::new();
        let bytes = bytes.strip_prefix("\u{feff}".as_bytes()).unwrap_or(bytes);
        for (i, line) in bytes.split(|&b| b == b'\n').enumerate() {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let applied = match std::str::from_utf8(line) {
                Ok(line) if line.is_empty() || line.starts_with('#') => continue,
                Ok(line) => dictionary.append(line),
                Err(_) => Err(InvalidReason::Encoding),
            };
            if let Err(reason) = applied {
                invalid.push(InvalidLine {
                    line: i + 1,
                    reason,
                });
            }
        }
        (dictionary, invalid)
    }

    /// Applies one line as if it were the last line of the file.
    pub(crate) fn append(&mut self, line: impl AsRef<str>) -> Result<(), InvalidReason> {
        let record = parse_line(line.as_ref(), self.user_custom)?;
        self.apply(record);
        Ok(())
    }

    pub(crate) fn is_hidden(&self, reading: &str, surface: &str) -> bool {
        self.hidden
            .contains(&(reading.to_owned(), surface.to_owned()))
    }

    /// The pairs `!` lines hide from every dictionary, by reading, then
    /// surface. A numeric pair holds its placeholders as the engine does:
    /// [`show_placeholders`] writes it for people to read.
    pub fn hidden(&self) -> impl Iterator<Item = (&str, &str)> {
        let mut pairs: Vec<(&str, &str)> = self
            .hidden
            .iter()
            .map(|(reading, surface)| (reading.as_str(), surface.as_str()))
            .collect();
        pairs.sort_unstable();
        pairs.into_iter()
    }

    /// Whether `line` of a user custom dictionary hides (`reading`, `surface`).
    pub(crate) fn hides(line: &str, reading: &str, surface: &str) -> bool {
        matches!(
            parse_line(line, true),
            Ok(Record::Hide { reading: r, surface: s }) if r == reading && s == surface
        )
    }

    /// A `*` in `reading` is literal: a hide line never marks okurigana.
    pub(crate) fn hide_line(reading: &str, surface: &str) -> String {
        let reading = escape_literal_reading(reading);
        format!("!{reading}\t{}", escape_surface(surface))
    }

    fn apply(&mut self, record: Record) {
        match record {
            Record::Word { reading, item } => {
                // A hide line has no conjugation type, so only a later line
                // without one is the same (reading, surface, type) and wins.
                if item.conjugation.is_none() {
                    self.hidden.remove(&(reading.clone(), item.surface.clone()));
                }
                push_front(self.words.entry(reading).or_default(), item);
            }
            Record::Okuri { stem, kana, item } => {
                self.hidden
                    .remove(&(format!("{stem}{kana}"), item.surface.clone()));
                let kanas = self.okuri_kana.entry(stem.clone()).or_default();
                if !kanas.contains(&kana) {
                    kanas.push(kana.clone());
                }
                push_front(self.okuri.entry((stem, kana)).or_default(), item);
            }
            Record::Hide { reading, surface } => {
                if let Some(items) = self.words.get_mut(&reading) {
                    items.retain(|i| i.surface != surface || i.conjugation.is_some());
                }
                self.hidden.insert((reading, surface));
            }
        }
    }
}

impl Dictionary for TextDictionary {
    fn lookup(&self, key: &str) -> Vec<Entry> {
        self.group(key)
            .map(|(item, cost)| entry_of(item, cost))
            .collect()
    }

    fn okuri(&self, stem: &str, row: char) -> Vec<Entry> {
        let Some(kanas) = self.okuri_kana.get(stem) else {
            return Vec::new();
        };
        let mut found: Vec<Entry> = kanas
            .iter()
            .filter(|kana| kana.chars().next().and_then(okuri_row) == Some(row))
            .flat_map(|kana| {
                let items = &self.okuri[&(stem.to_owned(), kana.clone())];
                entries(items).into_iter().map(move |mut entry| {
                    entry.surface.truncate(entry.surface.len() - kana.len());
                    entry
                })
            })
            .collect();
        found.sort_by_key(|e| e.cost);
        found
    }
}

impl TextDictionary {
    /// Every reading with words or stems.
    pub(crate) fn readings(&self) -> impl Iterator<Item = &str> {
        self.words
            .iter()
            .filter(|(_, items)| !items.is_empty())
            .map(|(reading, _)| reading.as_str())
    }

    /// Every stem and okurigana row with okurigana lines, once each.
    pub(crate) fn okuri_keys(&self) -> impl Iterator<Item = (&str, char)> {
        self.okuri_kana.iter().flat_map(|(stem, kanas)| {
            let mut rows: Vec<char> = kanas
                .iter()
                .filter_map(|kana| kana.chars().next().and_then(okuri_row))
                .collect();
            rows.sort_unstable();
            rows.dedup();
            rows.into_iter().map(move |row| (stem.as_str(), row))
        })
    }
}

impl TextDictionary {
    /// The items of one reading with their costs, cheapest first.
    fn group(&self, reading: &str) -> impl Iterator<Item = (&Item, u32)> {
        self.words
            .get(reading)
            .into_iter()
            .flat_map(|items| costed(items))
    }
}

fn push_front(items: &mut Vec<Item>, item: Item) {
    items.retain(|i| i.surface != item.surface || i.conjugation != item.conjugation);
    items.insert(0, item);
}

fn entry_of(item: &Item, cost: u32) -> Entry {
    Entry {
        surface: item.surface.clone(),
        conjugation: item.conjugation.clone(),
        cost,
    }
}

fn entries(items: &[Item]) -> Vec<Entry> {
    costed(items)
        .map(|(item, cost)| entry_of(item, cost))
        .collect()
}

/// Items paired with their costs, cheapest first; equal costs keep their place.
fn costed(items: &[Item]) -> impl Iterator<Item = (&Item, u32)> {
    let mut costed: Vec<(&Item, u32)> = items
        .iter()
        .zip(0..)
        .map(|(item, place)| (item, item.cost.unwrap_or(place)))
        .collect();
    costed.sort_by_key(|&(_, cost)| cost);
    costed.into_iter()
}

fn parse_line(line: &str, user_custom: bool) -> Result<Record, InvalidReason> {
    let fields: Vec<&str> = line.split('\t').collect();
    if !(2..=4).contains(&fields.len()) {
        return Err(InvalidReason::FieldCount);
    }
    let (hide, reading) = match fields[0].strip_prefix('!') {
        Some(_) if !user_custom => return Err(InvalidReason::Hide),
        Some(rest) => (true, rest),
        None => (false, fields[0]),
    };
    let reading = Reading::parse(reading)?;
    let surface = nfc(&unescape_surface(fields[1])?);
    let conjugation = match fields.get(2) {
        Some(c) => Some(unescape(c)?).filter(|c| !c.is_empty()),
        None => None,
    };
    let cost = match fields.get(3).copied() {
        Some("") => None,
        Some(c) => Some(parse_cost(c)?),
        None => None,
    };
    if reading.text.is_empty() || surface.is_empty() {
        return Err(InvalidReason::Empty);
    }
    let numbers = reading.text.matches(OPEN).count();
    if surface.matches(OPEN).count() > numbers
        || numbers > 0 && (conjugation.is_some() || reading.okurigana.is_some())
    {
        return Err(InvalidReason::Placeholder);
    }
    let item = Item {
        surface,
        conjugation: conjugation.clone(),
        cost,
    };
    match reading.okurigana {
        Some(at) => {
            let kana = &reading.text[at..];
            // Only a kana with an okurigana row can ever be looked up.
            let single_kana =
                kana.chars().count() == 1 && kana.chars().all(|c| okuri_row(c).is_some());
            if hide || conjugation.is_some() || !single_kana || !item.surface.ends_with(kana) {
                return Err(InvalidReason::Okurigana);
            }
            Ok(Record::Okuri {
                stem: reading.text[..at].to_owned(),
                kana: kana.to_owned(),
                item,
            })
        }
        None if hide && conjugation.is_some() => Err(InvalidReason::Hide),
        None if conjugation
            .as_deref()
            .is_some_and(|c| ConjugationTable::builtin().suffixes(c).is_none()) =>
        {
            Err(InvalidReason::ConjugationType)
        }
        None if hide => Ok(Record::Hide {
            reading: reading.text,
            surface: item.surface,
        }),
        None => Ok(Record::Word {
            reading: reading.text,
            item,
        }),
    }
}

struct Reading {
    text: String,
    /// Byte offset in `text` where the okurigana starts.
    okurigana: Option<usize>,
}

impl Reading {
    fn parse(field: &str) -> Result<Self, InvalidReason> {
        let mut parts = vec![String::new()];
        let mut chars = field.chars();
        while let Some(c) = chars.next() {
            let at_head = parts.len() == 1 && parts[0].is_empty();
            let part = parts.last_mut().unwrap();
            match c {
                '*' => parts.push(String::new()),
                '\\' => match chars.next() {
                    Some(c @ ('*' | '{')) => part.push(c),
                    Some(c @ ('#' | '!')) if at_head => part.push(c),
                    Some(c) => part.push(unescape_char(c)?),
                    None => return Err(InvalidReason::Escape),
                },
                c => push_reading_char(part, c, &mut chars)?,
            }
        }
        match parts.as_slice() {
            [text] => Ok(Self {
                text: nfc(text),
                okurigana: None,
            }),
            [stem, kana] => {
                let stem = nfc(stem);
                Ok(Self {
                    okurigana: Some(stem.len()),
                    text: stem + &nfc(kana),
                })
            }
            _ => Err(InvalidReason::Okurigana),
        }
    }
}

/// Digits only: no sign, space or full-width digit.
fn parse_cost(field: &str) -> Result<u32, InvalidReason> {
    if field.is_empty() || !field.bytes().all(|b| b.is_ascii_digit()) {
        return Err(InvalidReason::Cost);
    }
    field.parse::<u32>().map_err(|_| InvalidReason::Cost)
}

pub(crate) fn unescape(field: &str) -> Result<String, InvalidReason> {
    let mut out = String::new();
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(unescape_char(chars.next().ok_or(InvalidReason::Escape)?)?);
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// A reading as [`escape_literal_reading`] writes it: every `*` is literal.
pub(crate) fn unescape_literal_reading(field: &str) -> Result<String, InvalidReason> {
    let mut out = String::new();
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            push_reading_char(&mut out, c, &mut chars)?;
            continue;
        }
        match chars.next() {
            Some(c @ ('*' | '{')) => out.push(c),
            Some(c @ ('#' | '!')) if out.is_empty() => out.push(c),
            Some(c) => out.push(unescape_char(c)?),
            None => return Err(InvalidReason::Escape),
        }
    }
    Ok(out)
}

/// Pushes a reading's unescaped `c`, reading on past the `}` a placeholder
/// needs. A reading's placeholder names no notation: the surface does.
fn push_reading_char(
    out: &mut String,
    c: char,
    rest: &mut impl Iterator<Item = char>,
) -> Result<(), InvalidReason> {
    match c {
        '{' if rest.next() == Some('}') => out.extend([OPEN, CLOSE]),
        '{' | OPEN | CLOSE => return Err(InvalidReason::Placeholder),
        c => out.push(c),
    }
    Ok(())
}

/// A surface, whose `{…}` are placeholders with the name of a notation.
pub(crate) fn unescape_surface(field: &str) -> Result<String, InvalidReason> {
    let mut out = String::new();
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('{') => out.push('{'),
                Some(c) => out.push(unescape_char(c)?),
                None => return Err(InvalidReason::Escape),
            },
            '{' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => name.push(c),
                        None => return Err(InvalidReason::Placeholder),
                    }
                }
                Notation::named(&name).ok_or(InvalidReason::Placeholder)?;
                out.push(OPEN);
                out.push_str(&name);
                out.push(CLOSE);
            }
            OPEN | CLOSE => return Err(InvalidReason::Placeholder),
            c => out.push(c),
        }
    }
    Ok(out)
}

fn unescape_char(c: char) -> Result<char, InvalidReason> {
    match c {
        '\\' => Ok('\\'),
        't' => Ok('\t'),
        'n' => Ok('\n'),
        _ => Err(InvalidReason::Escape),
    }
}

pub(crate) fn escape(field: &str) -> String {
    field
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

/// A reading or surface the engine gave out, with a numeric item's
/// placeholders written `{}` and `{name}` again, for people to read. A literal
/// `{` is shown as it is, so the text cannot be given back to the engine.
pub fn show_placeholders(text: impl AsRef<str>) -> String {
    text.as_ref().replace(OPEN, "{").replace(CLOSE, "}")
}

/// `text` with each `{}` or `{name}` of a notation made a placeholder, as an
/// item's reading or surface holds them; other braces stay as written.
/// `None` when there is no placeholder. The inverse of [`show_placeholders`].
pub fn mark_placeholders(text: impl AsRef<str>) -> Option<String> {
    let mut out = String::new();
    let mut found = false;
    let mut rest = text.as_ref();
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let inside = &rest[open + 1..];
        match inside.find('}') {
            Some(close) if Notation::named(&inside[..close]).is_some() => {
                out.push(OPEN);
                out.push_str(&inside[..close]);
                out.push(CLOSE);
                found = true;
                rest = &inside[close + 1..];
            }
            _ => {
                out.push('{');
                rest = inside;
            }
        }
    }
    out.push_str(rest);
    found.then_some(out)
}

pub(crate) fn escape_surface(surface: &str) -> String {
    escape_braces(&escape(surface))
}

/// Writes placeholders as `{…}`, and a literal `{` so it is not taken for one.
fn escape_braces(field: &str) -> String {
    field
        .replace('{', "\\{")
        .replace(OPEN, "{")
        .replace(CLOSE, "}")
}

/// A reading whose `*` is a literal one, not the okurigana mark.
pub(crate) fn escape_literal_reading(reading: &str) -> String {
    escape_reading(reading).replace('*', "\\*")
}

fn escape_reading(reading: &str) -> String {
    let escaped = escape_braces(&escape(reading));
    if escaped.starts_with(['#', '!']) {
        format!("\\{escaped}")
    } else {
        escaped
    }
}

pub(crate) fn nfc(s: &str) -> String {
    s.nfc().collect()
}

#[cfg(test)]
mod tests;
