use std::fs;

use kanaemi_engine::{
    BinaryDictionary, BinaryError, Dictionary, TextDictionary, convert_text, is_binary,
    open_dictionary, text_digest,
};

use crate::common::temp_path;

const TEXT: &str = "きしゃ\t記者\nきしゃ\t汽車\nか*く\t欠く\n";

/// The text converted and written to a file, as the settings app ships it.
fn converted(text: impl AsRef<[u8]>) -> std::path::PathBuf {
    let (bytes, _) = convert_text(text);
    let path = temp_path("converted.kdic");
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn a_converted_dictionary_finds_what_the_text_finds() {
    let path = converted(TEXT);

    let binary = BinaryDictionary::open(&path).unwrap();

    let (text, _) = TextDictionary::parse(TEXT);
    assert_eq!(binary.lookup("きしゃ"), text.lookup("きしゃ"));
    assert_eq!(binary.okuri("か", 'k'), text.okuri("か", 'k'));
}

#[test]
fn converting_reports_the_lines_it_could_not_read() {
    let (_, invalid) = convert_text("きしゃ\t記者\nbroken\n");

    assert_eq!(invalid.len(), 1);
    assert_eq!(invalid[0].line, 2);
}

#[test]
fn a_converted_dictionary_remembers_the_digest_of_its_text() {
    let path = converted(TEXT);

    let binary = BinaryDictionary::open(&path).unwrap();

    assert_eq!(binary.source_digest(), Some(text_digest(TEXT)));
    assert_ne!(binary.source_digest(), Some(text_digest("other")));
}

#[test]
fn the_digest_is_the_sha_256_of_the_bytes() {
    let digest = text_digest(b"abc");

    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn the_digest_is_of_the_bytes_even_when_they_are_not_utf_8() {
    let mut bytes = TEXT.as_bytes().to_vec();
    bytes.extend_from_slice(&[0xff, b'\n']);
    let path = converted(&bytes);

    let binary = BinaryDictionary::open(&path).unwrap();

    assert_eq!(binary.source_digest(), Some(text_digest(&bytes)));
    assert_eq!(
        binary.lookup("きしゃ"),
        TextDictionary::parse(TEXT).0.lookup("きしゃ")
    );
}

#[test]
fn a_converted_dictionary_passes_its_checksums() {
    let path = converted(TEXT);

    let binary = BinaryDictionary::open(&path).unwrap();

    assert!(binary.verify_checksums().is_ok());
}

#[test]
fn a_changed_byte_opens_but_fails_its_checksum() {
    let path = converted(TEXT);
    let mut bytes = fs::read(&path).unwrap();
    // The file ends with the digest of the text, which opening takes as it is.
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(&path, bytes).unwrap();

    let binary = BinaryDictionary::open(&path).unwrap();

    assert!(matches!(
        binary.verify_checksums(),
        Err(BinaryError::Checksum(_))
    ));
}

#[test]
fn a_converted_dictionary_is_told_from_a_text_one_by_its_first_bytes() {
    let binary = converted(TEXT);
    let text = temp_path("text.kdic");
    fs::write(&text, TEXT).unwrap();

    assert!(is_binary(&binary).unwrap());
    assert!(!is_binary(&text).unwrap());
}

#[test]
fn a_file_shorter_than_the_magic_is_not_binary() {
    let path = temp_path("short.tsv");
    fs::write(&path, b"KANA").unwrap();

    assert!(!is_binary(&path).unwrap());
}

#[test]
fn a_converted_dictionary_opens_as_a_dictionary() {
    let path = converted(TEXT);

    let (dictionary, invalid) = open_dictionary(&path).unwrap();

    assert_eq!(invalid, []);
    assert_eq!(
        dictionary.lookup("きしゃ"),
        TextDictionary::parse(TEXT).0.lookup("きしゃ")
    );
}
