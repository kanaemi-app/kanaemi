//! SKK dictionaries, converted into the text dictionary format, and the user
//! custom dictionary written out as one.

use std::collections::BTreeMap;

use encoding_rs::Encoding;

use crate::placeholder::{CLOSE, OPEN};
use crate::text_dictionary::Word;
use crate::{
    ImeExport, ItemLine, OkuriHead, SkipReason, SkippedLine, TextDictionary, is_okuri_row,
    okuri_row, terminal_ending,
};

/// Converts an SKK dictionary into the text dictionary format. Each candidate
/// becomes a line costed by its place in its heading; an okuri-ari heading
/// files its candidates under its row, as SKK knows the okurigana's consonant
/// only. A heading with `#` becomes numeric items. What the format cannot hold
/// is left out: Lisp expressions, strict okurigana blocks, affix headings,
/// okuri-ari numeric headings, numbers in a notation Kanaemi lacks, the
/// noncharacters an item holds placeholders as, and a heading starting with a
/// byte order mark.
///
/// Bytes the dictionary's encoding cannot decode fail the import rather than
/// leave replacement characters in the words.
pub fn skk_to_text(bytes: impl AsRef<[u8]>) -> Result<String, SkkError> {
    read_skk_dictionary(bytes, SkkEncoding::EucJp).map(|read| read.text)
}

/// The encoding an SKK dictionary is in when its first line names none with
/// `coding:` and it has no byte order mark: the one its writer uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkkEncoding {
    /// That of most dictionaries, and of SKK implementations by default.
    EucJp,
    /// That of the implementations that write UTF-8 without saying so.
    Utf8,
}

/// An SKK dictionary as [`skk_to_text`] converts it, and what it left out:
/// each heading it dropped as its whole line, each candidate as its heading
/// with that candidate alone. A strict okurigana block only repeats
/// candidates for one okurigana and is not listed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkkWords {
    pub text: String,
    pub skipped: Vec<SkippedLine>,
}

/// Reads an SKK dictionary as [`skk_to_text`] does, listing what it leaves
/// out.
pub fn read_skk_dictionary(
    bytes: impl AsRef<[u8]>,
    unmarked: SkkEncoding,
) -> Result<SkkWords, SkkError> {
    let text = decode(bytes.as_ref(), unmarked)?;
    let mut read = SkkWords::default();
    let mut skip = |line: usize, reason: SkipReason, text: String| {
        read.skipped.push(SkippedLine { line, reason, text });
    };
    let mut out = String::new();
    for (i, line) in lines(&text).enumerate() {
        if line.starts_with(';') {
            continue;
        }
        let Some((heading, candidates)) = line.split_once(" /") else {
            if !line.trim().is_empty() {
                skip(i + 1, SkipReason::Unreadable, line.to_owned());
            }
            continue;
        };
        // A text dictionary takes a U+FEFF that starts its first line for a
        // byte order mark.
        if heading.is_empty()
            || heading.contains(['>', OPEN, CLOSE])
            || heading.starts_with('\u{feff}')
        {
            skip(i + 1, SkipReason::Unrepresentable, line.to_owned());
            continue;
        }
        let (stem, okurigana) = match okuri_ari(heading) {
            Some((stem, row)) => (stem, Some(row)),
            None if heading
                .chars()
                .last()
                .is_some_and(|c| c.is_ascii_lowercase())
                && !heading.is_ascii() =>
            {
                // An okuri-ari heading whose letter files no row.
                skip(i + 1, SkipReason::Unrepresentable, line.to_owned());
                continue;
            }
            None => (heading, None),
        };
        // A numeric heading's `#` stands for a number; okuri-ari ones never
        // get here, as a numeric item cannot have okurigana.
        let numbers = stem.matches('#').count();
        let stem = stem.replace('#', &format!("{OPEN}{CLOSE}"));
        let mut seen: Vec<&str> = Vec::new();
        // Inside a strict okurigana block (`[け/描/]`) until its `]`.
        let mut in_block = false;
        for candidate in candidates.split('/') {
            if in_block || candidate.starts_with('[') {
                in_block = candidate != "]";
                continue;
            }
            let surface = candidate
                .split_once(';')
                .map_or(candidate, |(surface, _)| surface);
            if surface.is_empty() || seen.contains(&surface) {
                continue;
            }
            if surface.starts_with('(') || surface.contains([OPEN, CLOSE]) {
                skip(
                    i + 1,
                    SkipReason::Unrepresentable,
                    format!("{heading} /{candidate}/"),
                );
                continue;
            }
            let written = match okurigana {
                None if numbers > 0 => match numeric_surface(surface) {
                    Some(written) if written.matches(OPEN).count() <= numbers => written,
                    _ => {
                        skip(
                            i + 1,
                            SkipReason::Unrepresentable,
                            format!("{heading} /{candidate}/"),
                        );
                        continue;
                    }
                },
                _ => surface.to_owned(),
            };
            let cost = seen.len();
            seen.push(surface);
            let okurigana = okurigana.map(String::from);
            let line = ItemLine {
                reading: &stem,
                okurigana: okurigana.as_deref(),
                surface: &written,
                conjugation: None,
                cost: Some(cost as u32),
            };
            out.push_str(&line.to_string());
            out.push('\n');
        }
    }
    read.text = out;
    Ok(read)
}

/// Writes the words of a user custom dictionary as an SKK dictionary in
/// `encoding`, which its first line names with `coding:`. A conjugating word
/// and an okurigana word go under an okuri-ari heading, its stem and the
/// consonant of the okurigana's first kana, the surface without that kana;
/// the others under okuri-nasi ones. Each heading's candidates keep the
/// dictionary's order, cheapest first. Left out and counted: hidden pairs,
/// words with placeholders, words whose heading or candidate SKK would read
/// otherwise, and words the encoding cannot hold.
pub fn write_skk_dictionary(dictionary: &TextDictionary, encoding: SkkEncoding) -> ImeExport {
    let (coding, encoding) = match encoding {
        SkkEncoding::EucJp => ("euc-jp", encoding_rs::EUC_JP),
        SkkEncoding::Utf8 => ("utf-8", encoding_rs::UTF_8),
    };
    // Some characters encode without error into others (¥ into the byte of
    // \ in EUC-JP), so only what reads back the same is held.
    let holds = |s: &str| {
        let (bytes, _, unmappable) = encoding.encode(s);
        !unmappable && encoding.decode_without_bom_handling(&bytes).0 == s
    };
    let mut ari: BTreeMap<String, Vec<(u32, String)>> = BTreeMap::new();
    let mut nasi: BTreeMap<String, Vec<(u32, String)>> = BTreeMap::new();
    let mut skipped = 0;
    for (word, cost) in dictionary.costed_words_in_order() {
        let Some((heading, candidate, okuri)) =
            skk_entry(word).filter(|(heading, candidate, _)| holds(heading) && holds(candidate))
        else {
            skipped += 1;
            continue;
        };
        if okuri { &mut ari } else { &mut nasi }
            .entry(heading)
            .or_default()
            .push((cost, candidate));
    }
    // Words of other readings meet under one heading (an okurigana word and
    // a conjugating one of the same stem): cheapest first again, each once.
    let [ari, nasi] = [ari, nasi].map(|headings| {
        headings
            .into_iter()
            .map(|(heading, mut costed)| {
                costed.sort_by_key(|(cost, _)| *cost);
                let mut candidates: Vec<String> = Vec::new();
                for (_, candidate) in costed {
                    if !candidates.contains(&candidate) {
                        candidates.push(candidate);
                    }
                }
                (heading, candidates)
            })
            .collect::<BTreeMap<_, _>>()
    });
    let entries = |entries: Vec<(&String, &Vec<String>)>| -> String {
        entries
            .into_iter()
            .map(|(heading, candidates)| format!("{heading} /{}/\n", candidates.join("/")))
            .collect()
    };
    // SKK keeps okuri-ari headings in descending order, okuri-nasi ones in
    // ascending.
    let text = format!(
        ";; -*- coding: {coding} -*-\n;; okuri-ari entries.\n{};; okuri-nasi entries.\n{}",
        entries(ari.iter().rev().collect()),
        entries(nasi.iter().collect()),
    );
    ImeExport {
        bytes: encoding.encode(&text).0.into_owned(),
        written: ari.values().chain(nasi.values()).map(Vec::len).sum(),
        skipped,
    }
}

/// A word's SKK heading and candidate, and whether the heading is okuri-ari.
fn skk_entry(word: Word<'_>) -> Option<(String, String, bool)> {
    let (heading, candidate, okuri) = match word {
        Word::Item {
            reading,
            surface,
            conjugation: None,
        } => (reading.to_owned(), surface.to_owned(), false),
        Word::Item {
            reading,
            surface,
            conjugation: Some(conjugation),
        } => {
            // The okurigana starts with the stem's last kana where the surface
            // ends with it too (たべ, 食べ), else with the terminal ending.
            let last = reading.chars().last()?;
            let (stem, surface, kana) = match surface.strip_suffix(last) {
                Some(rest) if !rest.is_empty() && okuri_row(last).is_some() => {
                    (&reading[..reading.len() - last.len_utf8()], rest, last)
                }
                _ => (
                    reading,
                    surface,
                    terminal_ending(conjugation)?.chars().next()?,
                ),
            };
            (
                format!("{stem}{}", skk_row(kana)?),
                surface.to_owned(),
                true,
            )
        }
        // SKK has no row of small kana.
        Word::Okuri {
            stem,
            head: OkuriHead::Row(row),
            surface,
        } if row != 'x' => (format!("{stem}{row}"), surface.to_owned(), true),
        Word::Okuri {
            head: OkuriHead::Row(_),
            ..
        } => return None,
        Word::Okuri {
            stem,
            head: OkuriHead::Kana(kana),
            surface,
        } => {
            let surface = surface.strip_suffix(kana).unwrap_or(surface);
            (
                format!("{stem}{}", skk_row(kana)?),
                surface.to_owned(),
                true,
            )
        }
        Word::Hidden { .. } => return None,
    };
    // What SKK takes for a number, an affix or okurigana in a heading: a `#`,
    // a `>`, and a last lowercase letter after other characters, which an
    // okuri-ari heading alone may have, after a stem without ASCII.
    let stem = if okuri {
        heading.strip_suffix(|c: char| c.is_ascii_lowercase())
    } else {
        Some(heading.as_str())
    };
    let misread = match stem {
        None => true,
        Some(stem) if okuri => stem.chars().any(|c| c.is_ascii()),
        Some(reading) => reading.ends_with(|c: char| c.is_ascii_lowercase()) && !reading.is_ascii(),
    } || heading.contains(['#', '>']);
    let unreadable = misread
        || heading.is_empty()
        || candidate.is_empty()
        || heading.contains([' ', '/', '\t', '\n', '\r', OPEN, CLOSE])
        || candidate.contains(['/', ';', '\n', '\r', OPEN, CLOSE])
        || candidate.starts_with(['[', '('])
        || heading.starts_with(';');
    (!unreadable).then_some((heading, candidate, okuri))
}

/// The letter SKK files okurigana starting with `kana` under: its row's,
/// except small kana, which SKK has no row for.
fn skk_row(kana: char) -> Option<char> {
    okuri_row(kana).filter(|&row| row != 'x')
}

/// Why an SKK dictionary could not be imported.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SkkError {
    /// `line`, from 1, holds bytes `encoding` cannot decode.
    #[error("line {line} is not {encoding}")]
    Undecodable { encoding: &'static str, line: usize },
    /// The `coding:` of the first line names an encoding Kanaemi cannot
    /// decode, such as JIS X 0213, whose bytes would otherwise read as other
    /// characters without any error.
    #[error("the encoding {name} is not supported")]
    UnsupportedEncoding { name: String },
}

/// A numeric heading's candidate with each `#` and digit made a placeholder of
/// the notation SKK writes with that digit. `None` when a digit names a
/// notation Kanaemi does not have. A `#` without a digit stays itself.
fn numeric_surface(candidate: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = candidate.chars().peekable();
    while let Some(c) = chars.next() {
        let Some(digit) = chars.next_if(|d| c == '#' && d.is_ascii_digit()) else {
            out.push(c);
            continue;
        };
        let name = match digit {
            '0' => "",
            '1' => "wide-num",
            '2' => "kanji-num",
            '3' => "kanji",
            '5' => "daiji",
            '8' => "grouped-num",
            _ => return None,
        };
        out.push(OPEN);
        out.push_str(name);
        out.push(CLOSE);
    }
    Some(out)
}

/// An okuri-ari heading (`かk`) split into its stem and the letter of its row.
fn okuri_ari(heading: &str) -> Option<(&str, char)> {
    let letter = heading.chars().last()?;
    let stem = &heading[..heading.len() - letter.len_utf8()];
    if !letter.is_ascii_lowercase() || stem.is_empty() || stem.chars().any(|c| c.is_ascii()) {
        return None;
    }
    // Some dictionaries file ち under c, after its spelling chi. SKK files by
    // the okurigana's consonant, so its x is no row of small kana.
    let row = match letter {
        'c' => 't',
        'x' => return None,
        letter => letter,
    };
    is_okuri_row(row).then_some((stem, row))
}

fn decode(bytes: &[u8], unmarked: SkkEncoding) -> Result<String, SkkError> {
    let first = bytes
        .split(|&b| b == b'\n' || b == b'\r')
        .next()
        .unwrap_or_default();
    // A byte order mark tells it first: CorvusSKK writes UTF-16 with one.
    let encoding = match Encoding::for_bom(bytes) {
        Some((encoding, _)) => encoding,
        None => marked_encoding(&String::from_utf8_lossy(first))?.unwrap_or(match unmarked {
            SkkEncoding::EucJp => encoding_rs::EUC_JP,
            SkkEncoding::Utf8 => encoding_rs::UTF_8,
        }),
    };
    let (text, had_errors) = encoding.decode_with_bom_removal(bytes);
    if !had_errors {
        return Ok(text.into_owned());
    }
    // What cannot be decoded became U+FFFD; one the bytes held themselves
    // decodes without errors and never gets here.
    let line = lines(&text)
        .position(|line| line.contains('\u{fffd}'))
        .map_or(1, |i| i + 1);
    Err(SkkError::Undecodable {
        encoding: encoding.name(),
        line,
    })
}

/// The lines of `text`, each ended by LF, CR LF or a lone CR, as SKK
/// dictionaries from every platform end them.
pub(crate) fn lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = (!text.is_empty()).then_some(text);
    std::iter::from_fn(move || {
        let current = rest?;
        let Some(at) = current.find(['\n', '\r']) else {
            rest = None;
            return Some(current);
        };
        let ending = if current[at..].starts_with("\r\n") {
            2
        } else {
            1
        };
        rest = Some(&current[at + ending..]).filter(|r| !r.is_empty());
        Some(&current[..at])
    })
}

/// The encoding the first line names with `coding:`, as an Emacs file
/// variable does. SKK dictionaries without one are mostly EUC-JP.
#[cfg(test)]
fn encoding(first_line: &str) -> Result<&'static Encoding, SkkError> {
    marked_encoding(first_line).map(|marked| marked.unwrap_or(encoding_rs::EUC_JP))
}

/// The encoding the first line names with `coding:`, if it names one.
fn marked_encoding(first_line: &str) -> Result<Option<&'static Encoding>, SkkError> {
    first_line
        .split_once("coding:")
        .and_then(|(_, rest)| {
            rest.split(|c: char| c.is_whitespace() || c == ';')
                .find(|label| !label.is_empty())
        })
        .map(coding_system)
        .transpose()
}

/// The encoding of an Emacs coding system. Its end-of-line variant does not
/// matter, as lines are split on any of them; one Emacs names but no Japanese
/// SKK dictionary uses is taken as EUC-JP. A JIS X 0213 one is refused.
fn coding_system(written: &str) -> Result<&'static Encoding, SkkError> {
    let name = written.to_ascii_lowercase();
    let name = ["-unix", "-dos", "-mac"]
        .iter()
        .find_map(|eol| name.strip_suffix(eol))
        .unwrap_or(&name);
    let unprefixed = name.strip_prefix("japanese-").unwrap_or(name);
    // JIS X 0213 extends the JIS X 0208 code space, so its bytes mostly
    // decode as the older encodings without error, into other characters.
    if ["2004", "x0213", "iso-2022-jp-3"]
        .iter()
        .any(|mark| unprefixed.contains(mark))
    {
        return Err(SkkError::UnsupportedEncoding {
            name: written.to_owned(),
        });
    }
    Ok(if name.contains("utf-8") || name.contains("utf8") {
        encoding_rs::UTF_8
    } else if ["sjis", "shift_jis", "shift-jis", "cp932"]
        .iter()
        .any(|family| unprefixed.starts_with(family))
    {
        encoding_rs::SHIFT_JIS
    } else if name.starts_with("iso-2022-jp") || ["junet", "japanese-iso-7bit"].contains(&name) {
        encoding_rs::ISO_2022_JP
    } else {
        // euc-jp, euc-japan and japanese-iso-8bit among them.
        encoding_rs::EUC_JP
    })
}

#[cfg(test)]
mod tests;
