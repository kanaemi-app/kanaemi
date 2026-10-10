//! The user dictionaries of other input methods, taken in as text
//! dictionaries of their own, and the user custom dictionary written out as
//! them.

use std::io;

use encoding_rs::{Encoding, SHIFT_JIS, UTF_8, UTF_16BE, UTF_16LE};
use unicode_normalization::UnicodeNormalization;

use crate::placeholder::{CLOSE, OPEN};
use crate::skk::lines;
use crate::text_dictionary::{Word, nfc};
use crate::{ItemLine, OkuriHead, TextDictionary, terminal_ending};

/// The input method a user dictionary file is written for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImeFormat {
    /// Microsoft IME, as its dictionary tool lists the words.
    MsIme,
    /// Google Japanese Input (and Mozc), as its dictionary tool exports them.
    Google,
    /// ATOK, as its dictionary utility lists the words.
    Atok,
    /// The user dictionary of macOS, a property list.
    MacOs,
}

impl ImeFormat {
    pub const ALL: [Self; 4] = [Self::MsIme, Self::Google, Self::Atok, Self::MacOs];

    /// What starts a comment line, in the text formats.
    fn comment(self) -> &'static str {
        match self {
            Self::MsIme | Self::Atok => "!",
            Self::Google => "#",
            Self::MacOs => "",
        }
    }
}

/// A word taken from another input method's dictionary: the text
/// dictionary lines it becomes, most often one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImeWord {
    /// Where it was, from 1: the line, or the element of a property list.
    pub line: usize,
    pub lines: Vec<String>,
}

/// A line, or an element of a property list, that was not taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedLine {
    /// From 1.
    pub line: usize,
    pub reason: SkipReason,
    /// What it held, to show: the line, or an element's fields joined by a
    /// tab.
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// It has no reading or no word.
    Unreadable,
    /// Its reading cannot be typed in Kanaemi, or its word holds characters
    /// a dictionary cannot.
    Unrepresentable,
    /// It hides a word (抑制単語), which only the user custom dictionary can.
    Hidden,
}

/// What another input method's dictionary holds, as Kanaemi takes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImeWords {
    pub words: Vec<ImeWord>,
    pub skipped: Vec<SkippedLine>,
}

impl ImeWords {
    /// The words as a text dictionary of their own, each line once.
    pub fn text(&self) -> String {
        let mut seen = std::collections::HashSet::new();
        let mut text = String::new();
        for line in self.words.iter().flat_map(|w| &w.lines) {
            if seen.insert(line) {
                text.push_str(line);
                text.push('\n');
            }
        }
        text
    }

    fn take(&mut self, line: usize, text: &str, reading: &str, surface: &str, pos: Option<&str>) {
        match word_lines(reading, surface, pos) {
            Ok(lines) => self.words.push(ImeWord { line, lines }),
            Err(reason) => self.skip(line, reason, text),
        }
    }

    fn skip(&mut self, line: usize, reason: SkipReason, text: &str) {
        self.skipped.push(SkippedLine {
            line,
            reason,
            text: text.to_owned(),
        });
    }
}

/// Why another input method's dictionary could not be read at all.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ImeDictionaryError {
    /// `line`, from 1, holds bytes `encoding` cannot decode.
    #[error("line {line} is not {encoding}")]
    Undecodable { encoding: &'static str, line: usize },
    #[error("not a property list: {0}")]
    NotPropertyList(String),
    /// A property list whose top is not an array of words.
    #[error("the property list is not an array")]
    NotArray,
}

/// Reads the user dictionary of another input method into text dictionary
/// lines. A line Kanaemi cannot take is left out and listed; only
/// a file that cannot be decoded fails.
pub fn read_ime_dictionary(
    bytes: impl AsRef<[u8]>,
    format: ImeFormat,
) -> Result<ImeWords, ImeDictionaryError> {
    let bytes = bytes.as_ref();
    let mut read = ImeWords::default();
    if format == ImeFormat::MacOs {
        let value = plist::Value::from_reader(io::Cursor::new(bytes))
            .map_err(|e| ImeDictionaryError::NotPropertyList(e.to_string()))?;
        let items = value.into_array().ok_or(ImeDictionaryError::NotArray)?;
        for (i, item) in items.iter().enumerate() {
            let field = |key: &str| {
                item.as_dictionary()
                    .and_then(|d| d.get(key))
                    .and_then(|v| v.as_string())
                    .filter(|s| !s.is_empty())
            };
            let (reading, surface) = (field("shortcut"), field("phrase"));
            let text = [reading, surface].map(Option::unwrap_or_default).join("\t");
            match (reading, surface) {
                (Some(reading), Some(surface)) => read.take(i + 1, &text, reading, surface, None),
                _ => read.skip(i + 1, SkipReason::Unreadable, &text),
            }
        }
        return Ok(read);
    }
    let text = decode(bytes)?;
    for (i, line) in lines(&text).enumerate() {
        if line.is_empty() || line.starts_with(format.comment()) {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            [reading, surface, rest @ ..] if !reading.is_empty() && !surface.is_empty() => {
                read.take(i + 1, line, reading, surface, rest.first().copied());
            }
            _ => read.skip(i + 1, SkipReason::Unreadable, line),
        }
    }
    Ok(read)
}

/// Reads the text replacements of macOS, its user dictionary, as (shortcut,
/// phrase) pairs, numbered from 1 in the order given.
pub fn read_text_replacements(entries: impl IntoIterator<Item = (String, String)>) -> ImeWords {
    let mut read = ImeWords::default();
    for (i, (reading, surface)) in entries.into_iter().enumerate() {
        let text = format!("{reading}\t{surface}");
        if reading.is_empty() || surface.is_empty() {
            read.skip(i + 1, SkipReason::Unreadable, &text);
        } else {
            read.take(i + 1, &text, &reading, &surface, None);
        }
    }
    read
}

/// A user dictionary written for another input method, and how many words
/// went in and were left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImeExport {
    pub bytes: Vec<u8>,
    pub written: usize,
    pub skipped: usize,
}

/// Writes the words of a user custom dictionary as another input method's
/// user dictionary. What that input method cannot hold is left out and
/// counted.
pub fn write_ime_dictionary(dictionary: &TextDictionary, format: ImeFormat) -> ImeExport {
    let mut entries: Vec<(String, String, Pos)> = Vec::new();
    let mut skipped = 0;
    for word in dictionary.words_in_order() {
        match exported(word, format) {
            Some(entry) => entries.push(entry),
            None => skipped += 1,
        }
    }
    // A サ行変格 stem stands for the word of the same pair too, as a
    // サ変名詞 is taken in as both.
    let suru: Vec<(String, String)> = entries
        .iter()
        .filter(|(_, _, pos)| *pos == Pos::SuruNoun)
        .map(|(r, s, _)| (r.clone(), s.clone()))
        .collect();
    entries
        .retain(|(r, s, pos)| *pos != Pos::Noun || !suru.iter().any(|(sr, ss)| sr == r && ss == s));
    // macOS has no parts of speech: a word written with two is one word.
    if format == ImeFormat::MacOs {
        for (_, _, pos) in &mut entries {
            *pos = Pos::Noun;
        }
    }
    let mut seen = std::collections::HashSet::new();
    entries.retain(|entry| seen.insert(entry.clone()));
    // By the reading written, each reading's words staying cheapest first.
    entries.sort_by(|(a, ..), (b, ..)| a.cmp(b));
    let written = entries.len();
    let bytes = match format {
        ImeFormat::MacOs => property_list(&entries),
        ImeFormat::Google => {
            let mut text = String::new();
            for (reading, surface, pos) in &entries {
                text.push_str(&format!("{reading}\t{surface}\t{}\n", pos.name(format)));
            }
            text.into_bytes()
        }
        ImeFormat::MsIme | ImeFormat::Atok => {
            let mut text = String::from(if format == ImeFormat::MsIme {
                "!Microsoft IME Dictionary Tool\r\n"
            } else {
                "!!ATOK_TANGO_TEXT_HEADER_1\r\n"
            });
            for (reading, surface, pos) in &entries {
                text.push_str(&format!("{reading}\t{surface}\t{}\r\n", pos.name(format)));
            }
            let mut bytes = vec![0xFF, 0xFE];
            bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
            bytes
        }
    };
    ImeExport {
        bytes,
        written,
        skipped,
    }
}

/// A part of speech Kanaemi writes out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Pos {
    Noun,
    /// A 五段 verb of the row of this katakana (カ, ガ … ワ).
    Godan(char),
    Ichidan,
    Adjective,
    SuruNoun,
    Suppressed,
}

impl Pos {
    fn name(self, format: ImeFormat) -> String {
        use ImeFormat::*;
        match (self, format) {
            (Self::Noun, _) => "名詞".to_owned(),
            (Self::Godan('ワ'), MsIme) => "あわ行五段".to_owned(),
            (Self::Godan(row), MsIme) => format!("{}行五段", hiragana(row)),
            (Self::Godan(row), Google) => format!("動詞{row}行五段"),
            (Self::Godan(row), _) => format!("{row}行五段"),
            (Self::Ichidan, Google) => "動詞一段".to_owned(),
            (Self::Ichidan, _) => "一段動詞".to_owned(),
            (Self::Adjective, _) => "形容詞".to_owned(),
            (Self::SuruNoun, MsIme) => "さ変名詞".to_owned(),
            (Self::SuruNoun, _) => "名詞サ変".to_owned(),
            (Self::Suppressed, _) => "抑制単語".to_owned(),
        }
    }
}

/// A word as another input method holds it: its reading, word and part of
/// speech. `None` for one `format` cannot hold.
fn exported(word: Word<'_>, format: ImeFormat) -> Option<(String, String, Pos)> {
    let (reading, surface, pos) = match word {
        Word::Item {
            reading,
            surface,
            conjugation: None,
        } => (reading.to_owned(), surface.to_owned(), Pos::Noun),
        Word::Item {
            reading,
            surface,
            conjugation: Some(conjugation),
        } => conjugated(reading, surface, conjugation)?,
        Word::Okuri {
            stem,
            head: OkuriHead::Kana(kana),
            surface,
        } => (format!("{stem}{kana}"), surface.to_owned(), Pos::Noun),
        // The okurigana's kana is not known.
        Word::Okuri { .. } => return None,
        Word::Hidden { .. } if matches!(format, ImeFormat::Atok | ImeFormat::MacOs) => return None,
        Word::Hidden { reading, surface } => {
            (reading.to_owned(), surface.to_owned(), Pos::Suppressed)
        }
    };
    let placeholders = |s: &str| s.contains([OPEN, CLOSE]);
    if placeholders(&reading) || placeholders(&surface) {
        return None;
    }
    if format != ImeFormat::MacOs {
        let breaks = |s: &str| s.contains(['\t', '\n', '\r']);
        if breaks(&reading) || breaks(&surface) || reading.starts_with(format.comment()) {
            return None;
        }
    }
    Some((reading, surface, pos))
}

/// A conjugating word in its dictionary form, or its stem for a サ行変格
/// one, which other input methods hold as a noun that takes する.
fn conjugated(stem: &str, surface: &str, conjugation: &str) -> Option<(String, String, Pos)> {
    if conjugation == "サ行変格" {
        return Some((stem.to_owned(), surface.to_owned(), Pos::SuruNoun));
    }
    let pos = if let Some(rest) = conjugation.strip_prefix("五段-") {
        Pos::Godan(rest.chars().next()?)
    } else if conjugation.starts_with("上一段-") || conjugation.starts_with("下一段-") {
        Pos::Ichidan
    } else if conjugation == "形容詞" {
        Pos::Adjective
    } else {
        return None;
    };
    let ending = terminal_ending(conjugation)?;
    Some((format!("{stem}{ending}"), format!("{surface}{ending}"), pos))
}

fn property_list(entries: &[(String, String, Pos)]) -> Vec<u8> {
    let items = entries
        .iter()
        .map(|(reading, surface, _)| {
            let mut item = plist::Dictionary::new();
            item.insert("phrase".to_owned(), plist::Value::String(surface.clone()));
            item.insert("shortcut".to_owned(), plist::Value::String(reading.clone()));
            plist::Value::Dictionary(item)
        })
        .collect();
    let mut bytes = Vec::new();
    plist::Value::Array(items)
        .to_writer_xml(&mut bytes)
        .expect("writing to memory does not fail");
    bytes.push(b'\n');
    bytes
}

/// The text dictionary lines of one word of another input method.
fn word_lines(reading: &str, surface: &str, pos: Option<&str>) -> Result<Vec<String>, SkipReason> {
    let reading = normalize_reading(reading).ok_or(SkipReason::Unrepresentable)?;
    let surface = nfc(surface);
    if surface
        .chars()
        .any(|c| matches!(c, OPEN | CLOSE) || (c.is_control() && !matches!(c, '\t' | '\n')))
    {
        return Err(SkipReason::Unrepresentable);
    }
    let line = |reading: &str, surface: &str, conjugation: Option<&str>| {
        ItemLine {
            reading,
            okurigana: None,
            surface,
            conjugation,
            cost: None,
        }
        .to_string()
    };
    let word = line(&reading, &surface, None);
    let pos = pos.map(normalize_pos).unwrap_or_default();
    let lines = match kind(&pos) {
        Kind::Word => vec![word],
        Kind::Hide => return Err(SkipReason::Hidden),
        Kind::SuruNoun => vec![word, line(&reading, &surface, Some("サ行変格"))],
        Kind::Conjugates(conjugation) => {
            match stem(&reading, &surface, conjugation) {
                Some((stem, stem_surface, conjugation)) => {
                    vec![line(stem, stem_surface, Some(&conjugation))]
                }
                // The part of speech is dropped.
                None => vec![word],
            }
        }
    };
    if !lines.iter().all(|line| TextDictionary::reads_line(line)) {
        return Err(SkipReason::Unrepresentable);
    }
    Ok(lines)
}

/// What a part of speech of another input method becomes.
#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Word,
    Hide,
    /// A noun that takes する: a word, and a サ行変格 stem.
    SuruNoun,
    /// A conjugation type; [`ICHIDAN`] for one the stem's last kana picks.
    Conjugates(&'static str),
}

/// Stands for 上一段-〇行 and 下一段-〇行 until the stem is known.
const ICHIDAN: &str = "一段";

fn kind(pos: &str) -> Kind {
    match pos {
        "抑制単語" => Kind::Hide,
        "さ変名詞" | "名詞サ変" | "サ変名詞" | "さ変形動名詞" | "名サ形動" => {
            Kind::SuruNoun
        }
        "サ変動詞" | "動詞サ変" => Kind::Conjugates("サ行変格"),
        "一段動詞" | "動詞一段" => Kind::Conjugates(ICHIDAN),
        "か行促音便" => Kind::Conjugates("五段-カ行-促音便"),
        "あわ行う音便" | "ワ行五段音便" => Kind::Conjugates("五段-ワア行-ウ音便"),
        pos if pos.starts_with("形容詞") => Kind::Conjugates("形容詞"),
        pos => godan(pos).map_or(Kind::Word, Kind::Conjugates),
    }
}

/// The 五段 type of `か行五段`, `カ行五段` and `動詞カ行五段`, and of the
/// other rows alike.
fn godan(pos: &str) -> Option<&'static str> {
    let row = pos
        .strip_prefix("動詞")
        .unwrap_or(pos)
        .strip_suffix("行五段")?;
    let row: String = row.chars().map(katakana).collect();
    Some(match row.as_str() {
        "カ" => "五段-カ行",
        "ガ" => "五段-ガ行",
        "サ" => "五段-サ行",
        "タ" => "五段-タ行",
        "ナ" => "五段-ナ行",
        "バ" => "五段-バ行",
        "マ" => "五段-マ行",
        "ラ" => "五段-ラ行",
        "ワ" | "アワ" => "五段-ワア行",
        _ => return None,
    })
}

/// The stem of a conjugating word, both reading and surface, with its type:
/// the word without its terminal ending. `None` when the word does not end
/// so, the stem would be empty, or the table lacks the type.
fn stem<'a>(
    reading: &'a str,
    surface: &'a str,
    conjugation: &'static str,
) -> Option<(&'a str, &'a str, String)> {
    let ending = if conjugation == ICHIDAN {
        "る"
    } else {
        terminal_ending(conjugation)?
    };
    let stem = reading.strip_suffix(ending).filter(|s| !s.is_empty())?;
    let stem_surface = surface.strip_suffix(ending).filter(|s| !s.is_empty())?;
    let conjugation = match conjugation {
        ICHIDAN => ichidan(stem.chars().last()?)?,
        // 行く and its compounds make った and って.
        "五段-カ行"
            if (stem.ends_with('い') || stem.ends_with('ゆ'))
                && (stem_surface.ends_with('行') || stem_surface.ends_with('逝')) =>
        {
            "五段-カ行-促音便".to_owned()
        }
        conjugation => conjugation.to_owned(),
    };
    terminal_ending(&conjugation)?;
    Some((stem, stem_surface, conjugation))
}

/// The 一段 type of a stem ending in `kana`: 上一段 for an い-row kana, 下一段
/// for an え-row one, of the kana's row.
fn ichidan(kana: char) -> Option<String> {
    const I: &str = "いきぎしじちぢにひびぴみりゐ";
    const E: &str = "えけげせぜてでねへべぺめれゑ";
    const ROWS: [&str; 14] = [
        "ア", "カ", "ガ", "サ", "ザ", "タ", "ダ", "ナ", "ハ", "バ", "パ", "マ", "ラ", "ワ",
    ];
    let (grade, at) = match (
        I.chars().position(|c| c == kana),
        E.chars().position(|c| c == kana),
    ) {
        (Some(at), _) => ("上一段", at),
        (_, Some(at)) => ("下一段", at),
        _ => return None,
    };
    Some(format!("{grade}-{}行", ROWS[at]))
}

/// A reading made hiragana as Kanaemi types it, or `None` when it holds
/// what cannot be typed as a reading.
fn normalize_reading(reading: &str) -> Option<String> {
    let wide: String = reading
        .chars()
        .flat_map(|c| -> Box<dyn Iterator<Item = char>> {
            if ('\u{ff61}'..='\u{ff9f}').contains(&c) {
                Box::new(c.nfkc())
            } else {
                Box::new(std::iter::once(c))
            }
        })
        .collect();
    let normalized: String = nfc(&wide)
        .chars()
        .map(|c| match c {
            'ァ'..='ヶ' | 'ヽ' | 'ヾ' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            '!'..='~' if !c.is_ascii_alphabetic() => {
                char::from_u32(c as u32 - 0x21 + 0xFF01).unwrap_or(c)
            }
            c => c,
        })
        .collect();
    let typable = |c: char| match c {
        '\u{3041}'..='\u{3096}' | 'ゝ' | 'ゞ' | 'ー' | '・' | '\u{3001}'..='\u{303F}' => true,
        'Ａ'..='Ｚ' | 'ａ'..='ｚ' => false,
        '\u{FF01}'..='\u{FF5E}' => true,
        _ => false,
    };
    (!normalized.is_empty() && normalized.chars().all(typable)).then_some(normalized)
}

/// A part of speech with its full-width ASCII made half-width and its
/// half-width katakana full-width, without the marks ATOK puts at its end.
fn normalize_pos(pos: &str) -> String {
    let pos: String = pos.nfkc().collect();
    pos.trim_end_matches(['*', '$']).trim().to_owned()
}

fn katakana(c: char) -> char {
    match c {
        'ぁ'..='ゖ' => char::from_u32(c as u32 + 0x60).unwrap_or(c),
        c => c,
    }
}

fn hiragana(c: char) -> char {
    match c {
        'ァ'..='ヶ' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        c => c,
    }
}

/// The text of a user dictionary file in the encoding its first bytes tell.
fn decode(bytes: &[u8]) -> Result<String, ImeDictionaryError> {
    let (encoding, body): (&'static Encoding, &[u8]) = match Encoding::for_bom(bytes) {
        Some((encoding, bom)) => (encoding, &bytes[bom..]),
        None => {
            // Neither UTF-8 nor Shift_JIS text has a zero byte. UTF-16 has one
            // in every tab and line break, in the upper half of the unit.
            let zeros = |parity| {
                bytes
                    .iter()
                    .skip(parity)
                    .step_by(2)
                    .filter(|&&b| b == 0)
                    .count()
            };
            let encoding = match (zeros(0), zeros(1)) {
                (0, 0) if std::str::from_utf8(bytes).is_ok() => UTF_8,
                (0, 0) => SHIFT_JIS,
                (even, odd) if odd >= even => UTF_16LE,
                _ => UTF_16BE,
            };
            (encoding, bytes)
        }
    };
    let (text, had_errors) = encoding.decode_without_bom_handling(body);
    if !had_errors {
        return Ok(text.into_owned());
    }
    let line = lines(&text)
        .position(|line| line.contains('\u{fffd}'))
        .map_or(1, |i| i + 1);
    Err(ImeDictionaryError::Undecodable {
        encoding: encoding.name(),
        line,
    })
}

#[cfg(test)]
mod tests;
