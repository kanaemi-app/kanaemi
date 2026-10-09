use std::fs;
use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use kanaemi_core::Converter;
use kanaemi_engine::{
    Engine, FileLock, FileSink, InvalidReason, LineSink, Slot, TextDictionary, open_dictionary,
    registered, replace_file, replace_file_unsynced, unhide, unregister,
};

use crate::common::{Discard, Learn, dictionary, temp_path};

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
    let replaces: [fn(&std::path::Path, &str) -> io::Result<()>; 2] = [
        |path, bytes| replace_file(path, bytes),
        |path, bytes| replace_file_unsynced(path, bytes),
    ];
    for replace in replaces {
        replaced_holds_only_the_new_bytes(replace);
    }
}

fn replaced_holds_only_the_new_bytes(replace: fn(&std::path::Path, &str) -> io::Result<()>) {
    let path = temp_path("replaced.tsv");
    fs::write(&path, "old and longer").unwrap();
    replace(&path, "new").unwrap();
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
    let unsynced = temp_path("private-replace-unsynced.tsv");
    replace_file_unsynced(&unsynced, b"x").unwrap();
    assert_eq!(mode(&unsynced), 0o600);
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

#[test]
fn the_hidden_pairs_are_listed_by_reading_then_surface() {
    let (user, _) = TextDictionary::parse_user_custom(
        "!きしゃ\t汽車\n!かく\t欠く\n!かく\t書く\nかく\t欠く\n!きしゃ\t汽車\n",
    );

    let hidden: Vec<(&str, &str)> = user.hidden().collect();

    assert_eq!(hidden, [("かく", "書く"), ("きしゃ", "汽車")]);
}

#[test]
fn unhiding_removes_every_hide_line_of_the_pair_and_nothing_else() {
    let path = temp_path("unhide.tsv");
    fs::write(
        &path,
        "# 説明\n!きしゃ\t汽車\nきしゃ\t記者\n\\!きしゃ\t汽車\n!きしゃ\t汽車\r\n!かく\t書く",
    )
    .unwrap();

    unhide(&path, "きしゃ", "汽車").unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "# 説明\nきしゃ\t記者\n\\!きしゃ\t汽車\n!かく\t書く"
    );
    let (user, _) = TextDictionary::read_user_custom(&path).unwrap();
    assert!(user.hidden().all(|pair| pair != ("きしゃ", "汽車")));
    assert_eq!(user.hidden().collect::<Vec<_>>(), [("かく", "書く")]);
}

#[test]
fn a_hidden_pair_is_registered_when_a_word_line_of_the_file_gives_it() {
    let path = temp_path("registered.tsv");
    fs::write(
        &path,
        "きしゃ\t記者\n!きしゃ\t記者\n!かく\t書く\nか*っ\t勝っ\n!かった\t勝った\n",
    )
    .unwrap();

    let pairs = [("きしゃ", "記者"), ("かった", "勝った"), ("かく", "書く")];
    assert_eq!(registered(&path, &pairs).unwrap(), [true, true, false]);
}

#[test]
fn an_okurigana_word_of_a_row_registers_every_okurigana_of_its_row() {
    let path = temp_path("registered-row.tsv");
    fs::write(&path, "か*k\t書\n!かけ\t書け\n").unwrap();

    let pairs = [("かけ", "書け"), ("かさ", "書さ")];
    assert_eq!(registered(&path, &pairs).unwrap(), [true, false]);
    unregister(&path, "かけ", "書け").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "");
}

#[test]
fn an_okurigana_word_of_a_kana_registers_okurigana_starting_with_it() {
    let path = temp_path("registered-kana.tsv");
    fs::write(&path, "か*く\t書く\n").unwrap();

    let pairs = [("かくと", "書くと"), ("かけ", "書け")];
    assert_eq!(registered(&path, &pairs).unwrap(), [true, false]);
}

#[test]
fn a_comment_registers_nothing_and_stays() {
    let path = temp_path("registered-comment.tsv");
    fs::write(&path, "#tag\tタグ\n!\\#tag\tタグ\n").unwrap();

    assert_eq!(registered(&path, &[("#tag", "タグ")]).unwrap(), [false]);
    unregister(&path, "#tag", "タグ").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "#tag\tタグ\n");
}

#[test]
fn a_missing_file_registers_nothing() {
    let path = temp_path("registered-missing.tsv");
    assert_eq!(registered(path, &[("きしゃ", "記者")]).unwrap(), [false]);
}

#[test]
fn unregistering_removes_the_registration_and_the_hide_lines_of_the_pair() {
    let path = temp_path("unregister.tsv");
    fs::write(
        &path,
        "# 説明\nきしゃ\t記者\n!きしゃ\t記者\nか*っ\t勝っ\n!かった\t勝った\n!かく\t書く\n",
    )
    .unwrap();

    unregister(&path, "きしゃ", "記者").unwrap();
    unregister(&path, "かった", "勝った").unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "# 説明\n!かく\t書く\n");
}

#[test]
fn unhiding_a_reading_with_a_literal_star_matches_its_escaped_hide_line() {
    let path = temp_path("unhide-star.tsv");
    fs::write(&path, "!あ\\*\t亜\n!あ\t亜\n").unwrap();

    unhide(&path, "あ*", "亜").unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "!あ\t亜\n");
}

#[test]
fn unhiding_keeps_lines_that_are_not_utf_8_byte_for_byte() {
    let path = temp_path("unhide-broken.tsv");
    let mut bytes = with_a_broken_line();
    bytes.extend_from_slice("!きしゃ\t汽車\n".as_bytes());
    fs::write(&path, &bytes).unwrap();

    unhide(&path, "きしゃ", "汽車").unwrap();

    assert_eq!(fs::read(&path).unwrap(), with_a_broken_line());
}

#[test]
fn unhiding_matches_the_first_line_after_a_byte_order_mark() {
    let path = temp_path("unhide-bom.tsv");
    fs::write(&path, "\u{feff}!きしゃ\t汽車\nきしゃ\t記者\n").unwrap();

    unhide(&path, "きしゃ", "汽車").unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "きしゃ\t記者\n");
}

#[cfg(unix)]
#[test]
fn unhiding_a_pair_that_is_not_hidden_leaves_the_file_alone() {
    use std::os::unix::fs::MetadataExt;
    let path = temp_path("unhide-nothing.tsv");
    fs::write(&path, "!かく\t書く\n").unwrap();
    let before = fs::metadata(&path).unwrap().ino();

    unhide(&path, "きしゃ", "汽車").unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "!かく\t書く\n");
    assert_eq!(fs::metadata(&path).unwrap().ino(), before, "not replaced");
}

#[test]
fn unhiding_in_a_missing_file_does_nothing() {
    let path = temp_path("unhide-missing.tsv");

    unhide(&path, "きしゃ", "汽車").unwrap();

    assert!(!path.exists());
}

#[test]
fn a_lock_held_elsewhere_is_not_waited_for_beyond_the_time_given() {
    let path = temp_path("held.tsv");
    let held = FileLock::hold(&path).unwrap();
    let start = Instant::now();
    let taken = FileLock::try_hold(&path, Duration::from_millis(20)).unwrap();
    assert!(taken.is_none());
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(held);
    assert!(FileLock::try_hold(&path, Duration::ZERO).unwrap().is_some());
}

#[test]
fn appending_while_the_lock_is_held_elsewhere_fails_instead_of_waiting() {
    let path = temp_path("held-append.tsv");
    fs::write(&path, "かく\t書く\n").unwrap();
    let held = FileLock::hold(&path).unwrap();
    // Appended on another thread, so a wait without end fails the test
    // rather than hanging it. The bound is far above the sink's wait.
    let (done, appended) = mpsc::channel();
    let appending = path.clone();
    thread::spawn(move || {
        let _ = done.send(FileSink::new(&appending).append("きしゃ\t記者"));
    });
    let error = appended
        .recv_timeout(Duration::from_secs(60))
        .expect("the append waits for the lock without end")
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(fs::read_to_string(&path).unwrap(), "かく\t書く\n");
    drop(held);
    FileSink::waiting(&path, Duration::ZERO)
        .append("きしゃ\t記者")
        .unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "かく\t書く\nきしゃ\t記者\n"
    );
}

#[test]
fn a_word_registered_while_the_file_is_locked_is_written_when_the_focus_moves() {
    let path = temp_path("held-register.tsv");
    let held = FileLock::hold(&path).unwrap();
    let mut e = Engine::new(
        [Slot::UserCustom],
        TextDictionary::parse_user_custom("").0,
        FileSink::waiting(&path, Duration::ZERO),
    );
    e.register("きしゃ", "記者");
    let errors = e.take_write_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(!path.exists());
    assert_eq!(e.convert("きしゃ", None)[0].surface, "記者");
    // Still locked: kept for later again.
    e.move_focus();
    assert_eq!(e.take_write_errors().len(), 1);
    drop(held);
    e.move_focus();
    assert!(e.take_write_errors().is_empty());
    assert_eq!(fs::read_to_string(&path).unwrap(), "きしゃ\t記者\n");
}
