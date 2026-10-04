//! SKK dictionaries, converted into the text dictionary format.

use encoding_rs::Encoding;

use crate::numeric::{CLOSE, OPEN};
use crate::{ItemLine, row_kana};

/// Converts an SKK dictionary into the text dictionary format. Each candidate
/// becomes a line costed by its place in its heading; an okuri-ari heading
/// files its candidates under the first kana of its row, which matches every
/// okurigana of that row. A heading with `#` becomes numeric items. What the
/// format cannot hold is left out: Lisp expressions, strict okurigana blocks,
/// affix headings, okuri-ari numeric headings and numbers in a notation
/// Kanaemi lacks.
///
/// Bytes the dictionary's encoding cannot decode fail the import rather than
/// leave replacement characters in the words.
pub fn skk_to_text(bytes: impl AsRef<[u8]>) -> Result<String, SkkError> {
    let text = decode(bytes.as_ref())?;
    let mut out = String::new();
    for line in lines(&text) {
        if line.starts_with(';') {
            continue;
        }
        let Some((heading, candidates)) = line.split_once(" /") else {
            continue;
        };
        if heading.is_empty() || heading.contains('>') {
            continue;
        }
        let (stem, okurigana) = match okuri_ari(heading) {
            Some((stem, kana)) => (stem, Some(kana)),
            None if heading
                .chars()
                .last()
                .is_some_and(|c| c.is_ascii_lowercase())
                && !heading.is_ascii() =>
            {
                // An okuri-ari heading whose letter files no row.
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
            if surface.is_empty() || surface.starts_with('(') || seen.contains(&surface) {
                continue;
            }
            let written = match okurigana {
                Some(kana) => format!("{surface}{kana}"),
                None if numbers > 0 => match numeric_surface(surface) {
                    Some(written) if written.matches(OPEN).count() <= numbers => written,
                    _ => continue,
                },
                None => surface.to_owned(),
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
    Ok(out)
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

/// An okuri-ari heading (`かk`) split into its stem and the kana standing for
/// its row.
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
    Some((stem, row_kana(row)?))
}

fn decode(bytes: &[u8]) -> Result<String, SkkError> {
    let first = bytes
        .split(|&b| b == b'\n' || b == b'\r')
        .next()
        .unwrap_or_default();
    let encoding = encoding(&String::from_utf8_lossy(first))?;
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
fn lines(text: &str) -> impl Iterator<Item = &str> {
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
fn encoding(first_line: &str) -> Result<&'static Encoding, SkkError> {
    first_line
        .split_once("coding:")
        .and_then(|(_, rest)| {
            rest.split(|c: char| c.is_whitespace() || c == ';')
                .find(|label| !label.is_empty())
        })
        .map_or(Ok(encoding_rs::EUC_JP), coding_system)
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
