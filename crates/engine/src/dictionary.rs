use std::io::{self, Read};
use std::path::Path;

use crate::{BINARY_MAGIC, BinaryDictionary, BinaryError, InvalidLine, TextDictionary};

/// One item of a dictionary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// For a conjugating word, its stem (書 for 書く); for an okurigana word,
    /// the part before the okurigana.
    pub surface: String,
    /// The conjugation type of a conjugating word.
    pub conjugation: Option<String>,
    /// Lower is better; only comparable within one dictionary.
    pub cost: u32,
}

/// A dictionary in the ordered list, as [`open_dictionary`] opens one from a
/// file.
pub trait Dictionary {
    /// Items whose reading is `key`: words that do not conjugate, and
    /// conjugating words whose stem reads `key`. Each kind comes cheapest first.
    fn lookup(&self, key: &str) -> Vec<Entry>;

    /// Words with okurigana whose reading before the okurigana is `stem` and
    /// whose okurigana starts in `row`, cheapest first. A row is the letter
    /// the binary dictionary format files it under: `k` for か through こ.
    /// Their surfaces stop before the okurigana.
    fn okuri(&self, stem: &str, row: char) -> Vec<Entry>;
}

#[derive(Debug, thiserror::Error)]
pub enum DictionaryError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Binary(#[from] BinaryError),
}

/// Opens a dictionary file: a binary dictionary by its first bytes, whatever
/// its name, or else a text dictionary with the lines it could not read.
pub fn open_dictionary(
    path: impl AsRef<Path>,
) -> Result<(Box<dyn Dictionary>, Vec<InvalidLine>), DictionaryError> {
    let path = path.as_ref();
    if is_binary(path)? {
        return Ok((Box::new(BinaryDictionary::open(path)?), Vec::new()));
    }
    let (dictionary, invalid) = TextDictionary::parse(std::fs::read(path)?);
    Ok((Box::new(dictionary), invalid))
}

/// Whether the file starts as a binary dictionary does.
pub fn is_binary(path: impl AsRef<Path>) -> io::Result<bool> {
    let mut head = Vec::with_capacity(BINARY_MAGIC.len());
    std::fs::File::open(path)?
        .take(BINARY_MAGIC.len() as u64)
        .read_to_end(&mut head)?;
    Ok(head == BINARY_MAGIC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary_dictionary::writer::encode;

    #[test]
    fn a_binary_dictionary_opens_as_one_whatever_its_name() {
        let path = std::env::temp_dir().join(format!(
            "kanaemi-engine-unit-{}-named-like-text.tsv",
            std::process::id()
        ));
        let (text, _) = TextDictionary::parse("きしゃ\t汽車");
        std::fs::write(&path, encode(&text, None)).unwrap();
        let (dictionary, invalid) = open_dictionary(&path).unwrap();
        assert_eq!(dictionary.lookup("きしゃ")[0].surface, "汽車");
        assert_eq!(invalid, []);
    }
}
