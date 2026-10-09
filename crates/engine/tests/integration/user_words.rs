//! Words added, edited and removed as the settings app does, in the file the
//! IME reads them from.

use std::fs;

use kanaemi_core::Converter;
use kanaemi_engine::{
    Engine, Slot, TextDictionary, UserWord, WordError, add_user_word, edit_user_word,
    remove_user_word, user_words,
};

use crate::common::{Discard, temp_path};

fn word(reading: &str, surface: &str) -> UserWord {
    UserWord::new(reading, surface, |_| true).unwrap()
}

fn surfaces(path: &std::path::Path, reading: &str, okurigana: Option<&str>) -> Vec<String> {
    let (user, invalid) = TextDictionary::read_user_custom(path).unwrap();
    assert_eq!(invalid, []);
    Engine::new([Slot::UserCustom], user, Discard)
        .convert(reading, okurigana)
        .into_iter()
        .map(|c| c.surface)
        .collect()
}

fn shown(words: &[UserWord]) -> Vec<(String, String)> {
    words.iter().map(|w| (w.reading(), w.surface())).collect()
}

#[test]
fn an_added_word_is_appended_and_converted() {
    let path = temp_path("words-add.tsv");
    add_user_word(&path, &word("きしゃ", "記者")).unwrap();
    add_user_word(&path, &word("か*く", "書く")).unwrap();
    add_user_word(&path, &word("{}こ", "{}個")).unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "きしゃ\t記者\nか*く\t書く\n{}こ\t{}個\n"
    );
    assert_eq!(surfaces(&path, "きしゃ", None), ["記者"]);
    assert_eq!(surfaces(&path, "かく", Some("く")), ["書く"]);
}

#[test]
fn a_word_added_again_is_refused() {
    let path = temp_path("words-again.tsv");
    fs::write(&path, "きしゃ\t記者").unwrap();
    let error = add_user_word(&path, &word("きしゃ", "記者")).unwrap_err();
    assert!(matches!(error, WordError::Exists), "{error}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "きしゃ\t記者");
}

#[test]
fn an_added_word_that_was_hidden_shows_again() {
    let path = temp_path("words-hidden.tsv");
    fs::write(&path, "!きしゃ\t記者\n").unwrap();
    add_user_word(&path, &word("きしゃ", "記者")).unwrap();
    assert_eq!(surfaces(&path, "きしゃ", None), ["記者"]);
}

#[test]
fn the_words_listed_are_those_it_can_write_each_once_by_reading() {
    let path = temp_path("words-list.tsv");
    fs::write(
        &path,
        "# 説明\nきしゃ\t記者\nか*く\t書く\n!かく\t書く\nきしゃ\t記者\n{}こ\t{kanji}個\n\
         か*k\t書\nか\t書\t五段-カ行\nあい\t愛\t\t3\nかっこ\t\\{-:}\nbroken\n",
    )
    .unwrap();
    assert_eq!(
        shown(&user_words(&path).unwrap()),
        [
            ("{}こ".to_owned(), "{kanji}個".to_owned()),
            ("か*く".to_owned(), "書く".to_owned()),
            ("きしゃ".to_owned(), "記者".to_owned()),
        ]
    );
    assert_eq!(user_words(temp_path("words-none.tsv")).unwrap(), []);
}

#[test]
fn an_edited_word_takes_the_place_of_its_last_line() {
    let path = temp_path("words-edit.tsv");
    fs::write(
        &path,
        "\u{feff}きしゃ\t記者\r\nきしゃ\t汽車\n!きしゃ\t記者\nきしゃ\t記者\r\nきしゃ\t帰社",
    )
    .unwrap();

    edit_user_word(&path, &word("きしゃ", "記者"), &word("きしゃ", "貴社")).unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "きしゃ\t汽車\n!きしゃ\t記者\nきしゃ\t貴社\r\nきしゃ\t帰社"
    );
    // Its place among the words of its reading is kept.
    assert_eq!(surfaces(&path, "きしゃ", None), ["帰社", "貴社", "汽車"]);
}

#[test]
fn a_word_can_be_edited_into_one_with_okurigana() {
    let path = temp_path("words-edit-okuri.tsv");
    fs::write(&path, "かく\t書く\n").unwrap();
    edit_user_word(&path, &word("かく", "書く"), &word("か*く", "書く")).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "か*く\t書く\n");
    assert_eq!(surfaces(&path, "かく", Some("く")), ["書く"]);
}

#[test]
fn an_edit_changes_nothing_when_the_word_is_gone_or_the_new_one_is_there() {
    let path = temp_path("words-edit-refused.tsv");
    let text = "きしゃ\t記者\nきしゃ\t汽車\n";
    fs::write(&path, text).unwrap();

    let gone = edit_user_word(&path, &word("きしゃ", "貴社"), &word("きしゃ", "帰社"));
    assert!(matches!(gone, Err(WordError::Gone)), "{gone:?}");
    let there = edit_user_word(&path, &word("きしゃ", "記者"), &word("きしゃ", "汽車"));
    assert!(matches!(there, Err(WordError::Exists)), "{there:?}");
    let missing = edit_user_word(
        temp_path("words-edit-missing.tsv"),
        &word("きしゃ", "記者"),
        &word("きしゃ", "汽車"),
    );
    assert!(matches!(missing, Err(WordError::Gone)), "{missing:?}");

    assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn a_removed_word_loses_every_line_of_it_and_nothing_else() {
    let path = temp_path("words-remove.tsv");
    fs::write(
        &path,
        "# 説明\nきしゃ\t記者\n!きしゃ\t記者\nきしゃ\t汽車\nきしゃ\t記者\t\t3\nきしゃ\t記者\n",
    )
    .unwrap();

    remove_user_word(&path, &word("きしゃ", "記者")).unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "# 説明\n!きしゃ\t記者\nきしゃ\t汽車\nきしゃ\t記者\t\t3\n"
    );
    let missing = temp_path("words-remove-missing.tsv");
    remove_user_word(&missing, &word("きしゃ", "記者")).unwrap();
    assert!(!missing.exists());
}
