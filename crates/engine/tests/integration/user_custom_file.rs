use std::fs;

use kanaemi_core::Converter;
use kanaemi_engine::{
    Engine, FileSink, InvalidReason, LineSink, Slot, TextDictionary, open_dictionary, replace_file,
};

use crate::common::{Discard, dictionary, temp_path};

#[test]
fn appending_creates_the_file() {
    let path = temp_path("create.tsv");
    let mut sink = FileSink::new(&path);
    sink.append("きしゃ\t記者").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "きしゃ\t記者\n");
}

#[test]
fn appending_adds_a_newline_first_when_the_last_line_lacks_one() {
    let path = temp_path("newline.tsv");
    fs::write(&path, "かく\t書く").unwrap();
    FileSink::new(&path).append("きしゃ\t記者").unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "かく\t書く\nきしゃ\t記者\n"
    );
}

#[test]
fn a_missing_file_reads_as_an_empty_dictionary() {
    let path = temp_path("missing.tsv");
    let (user, invalid) = TextDictionary::read_user_custom(&path).unwrap();
    assert_eq!(invalid, []);
    let engine = Engine::new([Slot::UserCustom], user, Discard);
    assert_eq!(engine.convert("きしゃ", None), []);
}

#[test]
fn what_was_appended_reads_back() {
    let path = temp_path("roundtrip.tsv");
    let mut sink = FileSink::new(&path);
    sink.append("きしゃ\t記者").unwrap();
    sink.append("!きしゃ\t汽車").unwrap();
    let (user, invalid) = TextDictionary::read_user_custom(&path).unwrap();
    assert_eq!(invalid, []);
    let engine = Engine::new(
        [
            Slot::UserCustom,
            Slot::Dictionary(dictionary("きしゃ\t汽車")),
        ],
        user,
        Discard,
    );
    let surfaces: Vec<String> = engine
        .convert("きしゃ", None)
        .into_iter()
        .map(|c| c.surface)
        .collect();
    assert_eq!(surfaces, ["記者"]);
}

#[test]
fn a_text_dictionary_file_opens_with_the_lines_it_could_not_read() {
    let path = temp_path("any.tsv");
    fs::write(&path, "きしゃ\t記者\nbroken").unwrap();
    let (dictionary, invalid) = open_dictionary(&path).unwrap();
    assert_eq!(dictionary.lookup("きしゃ")[0].surface, "記者");
    assert_eq!(invalid.len(), 1);
    assert_eq!(invalid[0].line, 2);
}

#[test]
fn a_broken_binary_dictionary_does_not_open() {
    let path = temp_path("broken.kdic");
    fs::write(&path, b"KANAEMID and nothing more").unwrap();
    assert!(open_dictionary(&path).is_err());
}

#[test]
fn a_replaced_file_holds_only_the_new_bytes_and_leaves_nothing_beside_it() {
    let path = temp_path("replaced.tsv");
    fs::write(&path, "old and longer").unwrap();
    replace_file(&path, "new").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    let folder = path.parent().unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let beside = fs::read_dir(folder)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&format!("{name}."))
        })
        .count();
    assert_eq!(beside, 0, "no partial file is left");
}

#[cfg(unix)]
#[test]
fn files_of_what_the_user_types_are_theirs_alone() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &std::path::Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let appended = temp_path("private-append.tsv");
    FileSink::new(&appended).append("きしゃ\t記者").unwrap();
    assert_eq!(mode(&appended), 0o600);
    let replaced = temp_path("private-replace.tsv");
    replace_file(&replaced, b"x").unwrap();
    assert_eq!(mode(&replaced), 0o600);
}

/// きしゃ 記者, a line cut off inside か, and かく 書く.
fn with_a_broken_line() -> Vec<u8> {
    let mut bytes = "きしゃ\t記者\n".as_bytes().to_vec();
    bytes.extend_from_slice(&"か".as_bytes()[..2]);
    bytes.extend_from_slice("\nかく\t書く\n".as_bytes());
    bytes
}

#[test]
fn a_line_that_is_not_utf_8_is_skipped_in_the_user_custom_dictionary() {
    let path = temp_path("broken-utf8.tsv");
    fs::write(&path, with_a_broken_line()).unwrap();
    let (user, invalid) = TextDictionary::read_user_custom(&path).unwrap();
    assert_eq!(invalid.len(), 1);
    assert_eq!(invalid[0].line, 2);
    let engine = Engine::new([Slot::UserCustom], user, Discard);
    assert_eq!(engine.convert("きしゃ", None).len(), 1);
    assert_eq!(engine.convert("かく", None).len(), 1);
}

#[test]
fn a_line_that_is_not_utf_8_is_skipped_in_a_text_dictionary() {
    let path = temp_path("broken-utf8-dictionary.tsv");
    fs::write(&path, with_a_broken_line()).unwrap();
    let (dictionary, invalid) = open_dictionary(&path).unwrap();
    assert_eq!(invalid.len(), 1);
    assert_eq!(invalid[0].line, 2);
    assert_eq!(dictionary.lookup("かく")[0].surface, "書く");
}

#[test]
fn a_line_that_is_not_utf_8_is_reported_as_such() {
    let path = temp_path("broken-utf8-reason.tsv");
    fs::write(&path, with_a_broken_line()).unwrap();
    let (_, invalid) = TextDictionary::read_user_custom(&path).unwrap();
    assert_eq!(invalid[0].reason, InvalidReason::Encoding);
}
