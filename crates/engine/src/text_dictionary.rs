use std::collections::{HashMap, HashSet};

use unicode_normalization::UnicodeNormalization;

use crate::placeholder::{CLOSE, OPEN, Placeholder, fits};
use crate::{ConjugationTable, Dictionary, Entry, OkuriHead};

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
    /// Includes a line with placeholders that conjugates or has okurigana,
    /// which the format does not allow.
    #[error("a placeholder is not written as the format allows")]
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

/// A word line of a user custom dictionary.
pub(crate) struct Registration(Record);

impl Registration {
    /// Whether the word gives (`reading`, `surface`): a word line of that
    /// pair, or an okurigana word whose stems the pair goes on from with
    /// okurigana the word is found by, as conversion finds it (`か*っ` and
    /// `勝っ` for `かった` and `勝った`; `か*k` and `書` for `かけ` and `書け`).
    pub(crate) fn gives(&self, reading: &str, surface: &str) -> bool {
        match &self.0 {
            Record::Word { reading: r, item } => r == reading && item.surface == surface,
            Record::Okuri { stem, head, item } => {
                okuri_gives(stem, *head, &item.surface, reading, surface)
            }
            Record::Hide { .. } => false,
        }
    }

    /// The word, as registering one writes it: its reading (the stem of an
    /// okurigana word, with placeholders as an item holds them), the head of
    /// its okurigana, and its surface. `None` for a word with a conjugation
    /// type or a cost, which only a person writes.
    pub(crate) fn plain(&self) -> Option<(&str, Option<OkuriHead>, &str)> {
        let (reading, head, item) = match &self.0 {
            Record::Word { reading, item } => (reading, None, item),
            Record::Okuri { stem, head, item } => (stem, Some(*head), item),
            Record::Hide { .. } => return None,
        };
        (item.conjugation.is_none() && item.cost.is_none()).then_some((
            reading.as_str(),
            head,
            item.surface.as_str(),
        ))
    }
}

/// Whether the okurigana word of `stem` and `head`, written `word`, gives
/// (`reading`, `surface`): both go on from its stems with the same okurigana,
/// one the word is found by.
fn okuri_gives(stem: &str, head: OkuriHead, word: &str, reading: &str, surface: &str) -> bool {
    let rest = reading.strip_prefix(stem);
    rest.is_some_and(|rest| head.starts(rest))
        && surface.strip_prefix(okuri_surface_stem(head, word)) == rest
}

/// The surface of an okurigana word before its okurigana: a word filed under
/// a kana is written ending in it (書く), one filed under a row without it (書).
fn okuri_surface_stem(head: OkuriHead, word: &str) -> &str {
    match head {
        OkuriHead::Kana(kana) => word.strip_suffix(kana).unwrap_or(word),
        OkuriHead::Row(_) => word,
    }
}

/// A word a dictionary gives, or a pair it hides, as
/// [`TextDictionary::words_in_order`] lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Word<'a> {
    /// A word, which conjugates when it has a type.
    Item {
        reading: &'a str,
        surface: &'a str,
        conjugation: Option<&'a str>,
    },
    /// An okurigana word, its surface written as its line writes it.
    Okuri {
        stem: &'a str,
        head: OkuriHead,
        surface: &'a str,
    },
    Hidden {
        reading: &'a str,
        surface: &'a str,
    },
}

impl Word<'_> {
    fn rank(&self) -> u8 {
        match self {
            Word::Item { .. } => 0,
            Word::Okuri { .. } => 1,
            Word::Hidden { .. } => 2,
        }
    }

    fn surface(&self) -> &str {
        match self {
            Word::Item { surface, .. }
            | Word::Okuri { surface, .. }
            | Word::Hidden { surface, .. } => surface,
        }
    }

    fn conjugation(&self) -> Option<&str> {
        match self {
            Word::Item { conjugation, .. } => *conjugation,
            _ => None,
        }
    }
}

enum Record {
    Word {
        reading: String,
        item: Item,
    },
    Okuri {
        stem: String,
        head: OkuriHead,
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
    /// What is written after the `*` that marks okurigana: its first kana,
    /// or the letter of that kana's row where only the row is known.
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
    okuri: HashMap<(String, OkuriHead), Vec<Item>>,
    okuri_heads: HashMap<String, Vec<OkuriHead>>,
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

    /// The word `line` of a user custom dictionary registers, read as the
    /// dictionary reads it: a blank line, a comment, a hide line or an
    /// invalid one registers none.
    pub(crate) fn registration(line: &str) -> Option<Registration> {
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        match parse_line(line, true) {
            Ok(Record::Hide { .. }) | Err(_) => None,
            Ok(record) => Some(Registration(record)),
        }
    }

    /// A `*` in `reading` is literal: a hide line never marks okurigana.
    pub(crate) fn hide_line(reading: &str, surface: &str) -> String {
        let reading = escape_literal_reading(reading);
        format!("!{reading}\t{}", escape_surface(surface))
    }

    /// Whether `line` is a line a dictionary other than the user custom one
    /// reads.
    pub(crate) fn reads_line(line: &str) -> bool {
        parse_line(line, false).is_ok()
    }

    /// Every word the dictionary gives and every pair it hides, by reading
    /// (an okurigana word's stem and what follows its `*`), then cost,
    /// cheapest first.
    pub(crate) fn words_in_order(&self) -> Vec<Word<'_>> {
        self.costed_words_in_order()
            .into_iter()
            .map(|(word, _)| word)
            .collect()
    }

    /// [`Self::words_in_order`] with each word's cost; a hidden pair's is the
    /// highest.
    pub(crate) fn costed_words_in_order(&self) -> Vec<(Word<'_>, u32)> {
        let mut words: Vec<(String, u32, Word<'_>)> = Vec::new();
        for (reading, items) in &self.words {
            for (item, cost) in costed(items) {
                let word = Word::Item {
                    reading,
                    surface: &item.surface,
                    conjugation: item.conjugation.as_deref(),
                };
                words.push((reading.clone(), cost, word));
            }
        }
        for ((stem, head), items) in &self.okuri {
            for (item, cost) in costed(items) {
                let key = format!("{stem}{}", head.as_char());
                let word = Word::Okuri {
                    stem,
                    head: *head,
                    surface: &item.surface,
                };
                words.push((key, cost, word));
            }
        }
        for (reading, surface) in &self.hidden {
            words.push((reading.clone(), u32::MAX, Word::Hidden { reading, surface }));
        }
        // Hash maps give no order, so ties go by what is left to compare.
        words.sort_by(|(a, a_cost, a_word), (b, b_cost, b_word)| {
            (
                a,
                a_cost,
                a_word.rank(),
                a_word.surface(),
                a_word.conjugation(),
            )
                .cmp(&(
                    b,
                    b_cost,
                    b_word.rank(),
                    b_word.surface(),
                    b_word.conjugation(),
                ))
        });
        words
            .into_iter()
            .map(|(_, cost, word)| (word, cost))
            .collect()
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
            Record::Okuri { stem, head, item } => {
                // The word brings back its forms going on from the okurigana
                // (勝った for か*っ), as registered from them.
                self.hidden
                    .retain(|(r, s)| !okuri_gives(&stem, head, &item.surface, r, s));
                let heads = self.okuri_heads.entry(stem.clone()).or_default();
                if !heads.contains(&head) {
                    heads.push(head);
                }
                push_front(self.okuri.entry((stem, head)).or_default(), item);
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

    fn okuri(&self, stem: &str, head: OkuriHead) -> Vec<Entry> {
        let Some(items) = self.okuri.get(&(stem.to_owned(), head)) else {
            return Vec::new();
        };
        entries(items)
            .into_iter()
            .map(|mut entry| {
                entry.surface = okuri_surface_stem(head, &entry.surface).to_owned();
                entry
            })
            .collect()
    }

    fn readings_from(&self, prefix: &str, limit: usize) -> Vec<String> {
        let mut readings: Vec<&str> = self
            .readings()
            .filter(|reading| reading.starts_with(prefix))
            .collect();
        readings.sort_unstable();
        readings.truncate(limit);
        readings.into_iter().map(str::to_owned).collect()
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

    /// Every stem and head with okurigana lines, once each.
    pub(crate) fn okuri_keys(&self) -> impl Iterator<Item = (&str, OkuriHead)> {
        self.okuri_heads
            .iter()
            .flat_map(|(stem, heads)| heads.iter().map(move |&head| (stem.as_str(), head)))
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
    let placeholders = numbers > 0 || surface.contains(OPEN);
    if !fits(&surface, numbers)
        || placeholders && (conjugation.is_some() || reading.okurigana.is_some())
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
            // Only a kana with an okurigana row, or a row, can ever be looked
            // up.
            let head = match OkuriHead::parse(&reading.text[at..]) {
                Some(head @ OkuriHead::Kana(kana)) if item.surface.ends_with(kana) => head,
                Some(head @ OkuriHead::Row(_)) => head,
                _ => return Err(InvalidReason::Okurigana),
            };
            if hide || conjugation.is_some() {
                return Err(InvalidReason::Okurigana);
            }
            Ok(Record::Okuri {
                stem: reading.text[..at].to_owned(),
                head,
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

/// A surface, whose `{…}` are placeholders.
pub(crate) fn unescape_surface(field: &str) -> Result<String, InvalidReason> {
    let mut out = String::new();
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(unescape_surface_char(&mut chars)?),
            '{' => {
                let mut inside = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('\\') => inside.push(unescape_surface_char(&mut chars)?),
                        Some(OPEN | CLOSE) | None => return Err(InvalidReason::Placeholder),
                        Some(c) => inside.push(c),
                    }
                }
                Placeholder::parse(&inside).ok_or(InvalidReason::Placeholder)?;
                out.push(OPEN);
                out.push_str(&inside);
                out.push(CLOSE);
            }
            OPEN | CLOSE => return Err(InvalidReason::Placeholder),
            c => out.push(c),
        }
    }
    Ok(out)
}

/// The character a surface's `\` escapes, read from `rest`.
fn unescape_surface_char(rest: &mut impl Iterator<Item = char>) -> Result<char, InvalidReason> {
    match rest.next() {
        Some(c @ ('{' | '}')) => Ok(c),
        Some(c) => unescape_char(c),
        None => Err(InvalidReason::Escape),
    }
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

/// A reading or surface the engine gave out, with its placeholders written
/// in braces again, for people to read. A literal
/// `{` is shown as it is, so the text cannot be given back to the engine.
pub fn show_placeholders(text: impl AsRef<str>) -> String {
    text.as_ref().replace(OPEN, "{").replace(CLOSE, "}")
}

/// `text` with each `{…}` written as a placeholder may be made one, as an
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
            Some(close) if Placeholder::parse(&inside[..close]).is_some() => {
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

/// Writes placeholders as `{…}`, a literal `{` so it is not taken for one, and
/// a `}` in a placeholder so it does not end it.
pub(crate) fn escape_surface(surface: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in escape(surface).chars() {
        match c {
            OPEN => {
                inside = true;
                out.push('{');
            }
            CLOSE => {
                inside = false;
                out.push('}');
            }
            '{' => out.push_str("\\{"),
            '}' if inside => out.push_str("\\}"),
            c => out.push(c),
        }
    }
    out
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
