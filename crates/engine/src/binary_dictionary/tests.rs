use kanaemi_core::Converter;

use super::writer::encode;
use super::*;
use crate::test_support::{Discard, Words, stems};
use crate::{Engine, Slot, TextDictionary};

const TEXT: &str = "\
きしゃ\t記者
きしゃ\t汽車
きしゃ\t貴社\t\t40
か\t蚊
か\t書\t五段-カ行
か\t掛\t下一段-カ行\t7
か*く\t欠く
か*け\t賭け
か*つ\t勝つ
おも*ち\tお持ち\t\t900
おも*っ\t思っ\t\t200
\\!\t感嘆
";

const READINGS: [&str; 6] = ["きしゃ", "か", "かく", "おも", "!", "ない"];

fn text() -> TextDictionary {
    let (dictionary, invalid) = TextDictionary::parse(TEXT);
    assert_eq!(invalid, []);
    dictionary
}

fn binary(text: &TextDictionary) -> BinaryDictionary {
    BinaryDictionary::from_bytes(encode(text, None)).unwrap()
}

#[test]
fn a_converted_dictionary_finds_what_the_text_finds() {
    let text = text();
    let binary = binary(&text);
    for reading in READINGS {
        assert_eq!(binary.words(reading), text.words(reading), "{reading}");
        assert_eq!(stems(&binary, reading), stems(&text, reading), "{reading}");
        for row in ['k', 't', 's'] {
            assert_eq!(
                binary.okuri(reading, row),
                text.okuri(reading, row),
                "{reading} {row}"
            );
        }
    }
}

fn engine(dictionary: impl Dictionary + 'static) -> Engine {
    Engine::new(
        vec![Slot::Dictionary(Box::new(dictionary))],
        TextDictionary::parse_user_custom("").0,
        Discard,
    )
}

#[test]
fn a_converted_dictionary_ranks_candidates_as_the_text_does() {
    let text = text();
    let binary = engine(binary(&text));
    let text = engine(text);
    for (reading, okurigana) in [
        ("きしゃ", None),
        ("かく", None),
        ("かけ", None),
        ("かく", Some("く")),
        ("かけ", Some("け")),
        ("おもっ", Some("っ")),
    ] {
        assert_eq!(
            binary.convert(reading, okurigana),
            text.convert(reading, okurigana),
            "{reading} {okurigana:?}"
        );
    }
}

#[test]
fn hide_lines_are_not_converted() {
    let (user, _) = TextDictionary::parse_user_custom("きしゃ\t記者\n!きしゃ\t汽車");
    let binary = binary(&user);
    assert_eq!(binary.words("きしゃ").len(), 1);
}

#[test]
fn a_string_too_long_for_the_format_leaves_its_line_out() {
    let long = "長".repeat(65_535 / 3 + 1);
    let (text, _) = TextDictionary::parse(format!("きしゃ\t{long}\nきしゃ\t記者"));
    let binary = binary(&text);
    assert_eq!(binary.words("きしゃ").len(), 1);
    assert_eq!(binary.words("きしゃ")[0].surface, "記者");
}

#[test]
fn converting_twice_gives_the_same_bytes() {
    assert_eq!(encode(&text(), None), encode(&text(), None));
}

#[test]
fn a_file_on_disk_opens() {
    let path =
        std::env::temp_dir().join(format!("kanaemi-engine-binary-{}.kdic", std::process::id()));
    std::fs::write(&path, encode(&text(), None)).unwrap();
    let binary = BinaryDictionary::open(&path).unwrap();
    assert_eq!(binary.words("きしゃ"), text().words("きしゃ"));
}

// The file layout, rebuilt here from the spec to craft broken files.

const STRINGS: u32 = 1;
const INDEX: u32 = 2;
const ENTRIES: u32 = 3;
const OKURI: u32 = 4;
const SOURCE: u32 = 5;
const ENTRY_LEN: usize = 12;

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

/// The sections of a file, as (kind, contents).
fn sections(bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
    (0..u32_at(bytes, 12) as usize)
        .map(|i| {
            let at = 32 + i * 32;
            let offset = u64_at(bytes, at + 8) as usize;
            let len = u64_at(bytes, at + 16) as usize;
            (u32_at(bytes, at), bytes[offset..offset + len].to_vec())
        })
        .collect()
}

/// A file of `sections` with correct checksums.
fn file(sections: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let align = |n: usize| n.div_ceil(64) * 64;
    let mut out = Vec::new();
    out.extend_from_slice(b"KANAEMID");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(sections.len() as u32).to_le_bytes());
    out.resize(32, 0);
    let mut offset = align(32 + 32 * sections.len());
    for (kind, contents) in sections {
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(offset as u64).to_le_bytes());
        out.extend_from_slice(&(contents.len() as u64).to_le_bytes());
        out.extend_from_slice(&xxhash_rust::xxh3::xxh3_64(contents).to_le_bytes());
        offset = align(offset + contents.len());
    }
    for (_, contents) in sections {
        out.resize(align(out.len()), 0);
        out.extend_from_slice(contents);
    }
    out
}

fn rejects(bytes: Vec<u8>) -> bool {
    BinaryDictionary::from_bytes(bytes).is_err()
}

#[test]
fn the_layout_follows_the_spec() {
    let bytes = encode(&text(), None);
    assert_eq!(file(&sections(&bytes)), bytes);
}

#[test]
fn an_unknown_section_is_skipped() {
    let mut parts = sections(&encode(&text(), None));
    parts.insert(1, (99, b"later".to_vec()));
    let binary = BinaryDictionary::from_bytes(file(&parts)).unwrap();
    assert_eq!(binary.words("きしゃ"), text().words("きしゃ"));
}

#[test]
fn the_okuri_section_may_be_missing() {
    let parts: Vec<_> = sections(&encode(&text(), None))
        .into_iter()
        .filter(|(kind, _)| *kind != OKURI)
        .collect();
    let binary = BinaryDictionary::from_bytes(file(&parts)).unwrap();
    assert_eq!(binary.okuri("か", 'k'), []);
    assert_eq!(binary.words("きしゃ"), text().words("きしゃ"));
}

#[test]
fn a_wrong_magic_or_version_is_rejected() {
    let good = encode(&text(), None);
    let mut magic = good.clone();
    magic[0] = b'X';
    assert!(rejects(magic));
    let mut version = good.clone();
    version[8] = 2;
    assert!(rejects(version));
    assert!(rejects(good[..20].to_vec()));
}

#[test]
fn a_missing_or_doubled_required_section_is_rejected() {
    let parts = sections(&encode(&text(), None));
    for kind in [STRINGS, INDEX, ENTRIES] {
        let without: Vec<_> = parts.iter().filter(|(k, _)| *k != kind).cloned().collect();
        assert!(rejects(file(&without)), "without {kind}");
        let mut doubled = parts.clone();
        doubled.push(parts.iter().find(|(k, _)| *k == kind).unwrap().clone());
        assert!(rejects(file(&doubled)), "doubled {kind}");
    }
}

#[test]
fn a_section_past_the_end_is_rejected() {
    let bytes = encode(&text(), None);
    assert!(rejects(bytes[..bytes.len() - 1].to_vec()));
}

#[test]
fn broken_references_are_rejected() {
    let parts = sections(&encode(&text(), None));
    let with = |kind: u32, change: &dyn Fn(&mut Vec<u8>)| {
        let mut parts = parts.clone();
        let (_, contents) = parts.iter_mut().find(|(k, _)| *k == kind).unwrap();
        change(contents);
        file(&parts)
    };
    // An entry count that is not whole.
    assert!(rejects(with(ENTRIES, &|e| e.truncate(e.len() - 1))));
    // An index range past the entries.
    assert!(rejects(with(ENTRIES, &|e| e.truncate(e.len() - ENTRY_LEN))));
    // A surface past the strings.
    assert!(rejects(with(ENTRIES, &|e| {
        e[0..4].copy_from_slice(&u32::MAX.to_le_bytes())
    })));
    // A string that is not UTF-8.
    assert!(rejects(with(STRINGS, &|s| {
        let last = s.len() - 1;
        s[last] = 0xff;
    })));
    // An index that is not an FST.
    assert!(rejects(with(INDEX, &|i| i.truncate(3))));
}

#[test]
fn opening_does_not_check_the_checksums() {
    let mut bytes = encode(&text(), None);
    let at = {
        let i = (0..u32_at(&bytes, 12) as usize)
            .find(|i| u32_at(&bytes, 32 + i * 32) == ENTRIES)
            .unwrap();
        u64_at(&bytes, 32 + i * 32 + 8) as usize
    };
    // The first entry's cost.
    bytes[at + 8] ^= 1;
    assert!(BinaryDictionary::from_bytes(bytes).is_ok());
}

#[test]
fn a_changed_byte_fails_the_checksum_of_its_section_only_when_verified() {
    let mut bytes = encode(&text(), None);
    let at = {
        let i = (0..u32_at(&bytes, 12) as usize)
            .find(|i| u32_at(&bytes, 32 + i * 32) == ENTRIES)
            .unwrap();
        u64_at(&bytes, 32 + i * 32 + 8) as usize
    };
    // The first entry's cost.
    bytes[at + 8] ^= 1;

    let binary = BinaryDictionary::from_bytes(bytes).unwrap();

    assert!(matches!(
        binary.verify_checksums(),
        Err(BinaryError::Checksum(ENTRIES))
    ));
}

#[test]
fn every_section_of_a_written_file_passes_its_checksum() {
    let source = Some([7; 32]);

    let binary = BinaryDictionary::from_bytes(encode(&text(), source)).unwrap();

    assert!(binary.verify_checksums().is_ok());
}

#[test]
fn a_source_digest_is_written_as_the_source_section() {
    let digest = [7; 32];

    let bytes = encode(&text(), Some(digest));

    let parts = sections(&bytes);
    assert_eq!(parts.last(), Some(&(SOURCE, digest.to_vec())));
    assert_eq!(file(&parts), bytes);
    let binary = BinaryDictionary::from_bytes(bytes).unwrap();
    assert_eq!(binary.source_digest(), Some(digest));
    assert_eq!(binary.words("きしゃ"), text().words("きしゃ"));
}

#[test]
fn a_file_without_a_source_section_has_no_source_digest() {
    let bytes = encode(&text(), None);

    let binary = BinaryDictionary::from_bytes(bytes).unwrap();

    assert_eq!(binary.source_digest(), None);
}

#[test]
fn a_shared_dictionary_can_be_read_from_other_threads() {
    fn send_sync<T: Send + Sync>(_: &T) {}
    send_sync(&binary(&text()));
}

#[test]
fn a_broken_fst_is_rejected_without_panicking() {
    let parts = sections(&encode(&text(), None));
    for root in [100u64, 30, 5] {
        let mut parts = parts.clone();
        let (_, index) = parts.iter_mut().find(|(k, _)| *k == INDEX).unwrap();
        // Version 3, an FST of type 0, one key, and a root past the data.
        let mut forged = Vec::new();
        forged.extend_from_slice(&3u64.to_le_bytes());
        forged.extend_from_slice(&0u64.to_le_bytes());
        forged.extend_from_slice(&[0; 4]);
        forged.extend_from_slice(&1u64.to_le_bytes());
        forged.extend_from_slice(&root.to_le_bytes());
        *index = forged;
        assert!(rejects(file(&parts)), "root {root}");
    }
}

/// An FST of version 3 whose root starts a chain of `depth` nodes, each with
/// two transitions to the next, ending in a node that is not final: it spells
/// `2^depth` paths and not one key.
fn fst_without_keys(depth: usize) -> Vec<u8> {
    let mut fst = Vec::new();
    fst.extend_from_slice(&3u64.to_le_bytes());
    fst.extend_from_slice(&0u64.to_le_bytes());
    // A node that is neither final nor has transitions: pack sizes, a count
    // of zero transitions, and the state byte.
    fst.extend_from_slice(&[0, 0, 0]);
    let mut last = fst.len() - 1;
    for _ in 0..depth {
        // Two one-byte deltas from the node's first byte back to the last
        // byte of the previous node, the inputs b and a, pack sizes (one-byte
        // deltas, no outputs), and the state byte of a node with two
        // transitions.
        fst.extend_from_slice(&[1, 1, b'b', b'a', 0x10, 0b00_000010]);
        last = fst.len() - 1;
    }
    fst.extend_from_slice(&1u64.to_le_bytes());
    fst.extend_from_slice(&(last as u64).to_le_bytes());
    fst.extend_from_slice(&[0; 4]);
    fst
}

#[test]
fn an_fst_of_endless_paths_without_keys_is_rejected_at_once() {
    let mut parts = sections(&encode(&text(), None));
    parts.iter_mut().find(|(k, _)| *k == INDEX).unwrap().1 = fst_without_keys(50);
    assert!(rejects_at_once(file(&parts)));
}

#[test]
fn an_okurigana_line_whose_surface_is_the_okurigana_converts() {
    let (text, invalid) = TextDictionary::parse("か*く\tく\nきしゃ\t記者");
    assert_eq!(invalid, []);
    let binary = binary(&text);
    assert_eq!(binary.okuri("か", 'k'), text.okuri("か", 'k'));
    assert_eq!(binary.words("きしゃ"), text.words("きしゃ"));
}

#[test]
fn a_numeric_item_holds_its_placeholders_as_noncharacters() {
    let (text, invalid) = TextDictionary::parse("{}こ\t{kanji}個\n\\{\t\\{}");
    assert_eq!(invalid, []);
    let bytes = encode(&text, None);
    let reading = "\u{FDD0}\u{FDD1}こ";
    let surface = "\u{FDD0}kanji\u{FDD1}個";
    let binary = BinaryDictionary::from_bytes(bytes.clone()).unwrap();
    assert_eq!(binary.words(reading), text.words(reading));
    assert_eq!(binary.words(reading)[0].surface, surface);
    assert_eq!(binary.words("{")[0].surface, "{}");
    let sections = sections(&bytes);
    let section = |kind| &sections.iter().find(|(k, _)| *k == kind).unwrap().1;
    let index = Map::new(section(INDEX).clone()).unwrap();
    assert!(index.contains_key(reading));
    assert!(index.contains_key("{"));
    let strings = String::from_utf8_lossy(section(STRINGS));
    assert!(strings.contains(surface), "{strings}");
}

#[test]
fn an_index_with_more_keys_than_entries_is_rejected() {
    let parts = sections(&encode(&text(), None));
    let entries = parts.iter().find(|(k, _)| *k == ENTRIES).unwrap().1.len() / ENTRY_LEN;
    let mut builder = fst::MapBuilder::memory();
    for i in 0..=entries {
        builder.insert(format!("{i:08}"), 1u64 << 32).unwrap();
    }
    let mut parts = parts.clone();
    parts.iter_mut().find(|(k, _)| *k == INDEX).unwrap().1 = builder.into_inner().unwrap();
    assert!(rejects(file(&parts)));
}

#[test]
fn keys_sharing_entries_are_rejected() {
    let mut builder = fst::MapBuilder::memory();
    builder.insert("か", 1u64 << 32).unwrap();
    builder.insert("きしゃ", 1u64 << 32).unwrap();
    let mut parts = sections(&encode(&text(), None));
    parts.iter_mut().find(|(k, _)| *k == INDEX).unwrap().1 = builder.into_inner().unwrap();
    assert!(rejects(file(&parts)));
}

#[test]
fn a_source_that_is_not_a_sha_256_is_rejected() {
    let mut parts = sections(&encode(&text(), None));
    parts.push((SOURCE, vec![0; 31]));
    assert!(rejects(file(&parts)));
}

/// Whether opening `bytes` ends within a few seconds, and with an error.
fn rejects_at_once(bytes: Vec<u8>) -> bool {
    let (done, opened) = std::sync::mpsc::channel();
    std::thread::spawn(move || done.send(rejects(bytes)));
    opened.recv_timeout(std::time::Duration::from_secs(5)) == Ok(true)
}

#[test]
fn a_transition_that_does_not_point_below_its_node_is_rejected_at_once() {
    // A root of two transitions whose delta wraps around to the root itself
    // where subtraction is not checked.
    let mut forged = Vec::new();
    forged.extend_from_slice(&3u64.to_le_bytes());
    forged.extend_from_slice(&0u64.to_le_bytes());
    forged.extend_from_slice(&(u64::MAX - 18).to_le_bytes());
    forged.extend_from_slice(&(u64::MAX - 18).to_le_bytes());
    forged.extend_from_slice(b"ba");
    forged.extend_from_slice(&[0x80, 0x02]);
    forged.extend_from_slice(&1u64.to_le_bytes());
    forged.extend_from_slice(&35u64.to_le_bytes());
    forged.extend_from_slice(&[0; 4]);
    let mut parts = sections(&encode(&text(), None));
    parts.iter_mut().find(|(k, _)| *k == INDEX).unwrap().1 = forged;
    assert!(rejects_at_once(file(&parts)));
}

#[test]
fn the_forged_fst_of_endless_paths_is_well_formed_but_spells_no_key() {
    let fst = fst::raw::Fst::new(fst_without_keys(3)).unwrap();
    let root = fst.root();
    assert_eq!(root.len(), 2);
    assert_eq!(root.transition_addr(0), root.transition_addr(1));
    let mut stream = fst.stream();
    assert!(stream.next().is_none());
}

#[test]
fn readings_are_listed_in_order_from_a_prefix() {
    let binary = BinaryDictionary::from_bytes(encode(&text(), None)).unwrap();
    for dictionary in [&binary as &dyn Dictionary, &text()] {
        assert_eq!(dictionary.readings_from("", 3), ["!", "か", "きしゃ"]);
        assert_eq!(dictionary.readings_from("き", 10), ["きしゃ"]);
        assert_eq!(dictionary.readings_from("ん", 10), Vec::<String>::new());
    }
}
