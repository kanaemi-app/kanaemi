//! Lines of a text dictionary the IME skips, to show where a hand edit went
//! wrong.

use std::fs;
use std::path::Path;

use kanaemi_engine::{InvalidLine, InvalidReason, TextDictionary};

use crate::cache::FileCache;

/// How many invalid lines are named; the rest are only counted.
const NAMED: usize = 3;

/// What to say about the invalid lines of a text dictionary, or `None` when
/// every line is read.
pub fn invalid_lines(path: &Path, user_custom: bool) -> Option<String> {
    thread_local! {
        static KNOWN: FileCache<Option<String>> = FileCache::default();
    }
    KNOWN.with(|known| known.get(&[path.to_owned()], || read_invalid(path, user_custom)))
}

fn read_invalid(path: &Path, user_custom: bool) -> Option<String> {
    let (_, invalid) = if user_custom {
        TextDictionary::read_user_custom(path).ok()?
    } else {
        TextDictionary::parse(fs::read(path).ok()?)
    };
    describe(&invalid)
}

fn describe(invalid: &[InvalidLine]) -> Option<String> {
    if invalid.is_empty() {
        return None;
    }
    let named: Vec<String> = invalid
        .iter()
        .take(NAMED)
        .map(|l| format!("{} 行目：{}", l.line, reason(l.reason)))
        .collect();
    let more = if invalid.len() > NAMED { "、…" } else { "" };
    Some(format!(
        "読めない行が {} 行あります（{}{more}）",
        invalid.len(),
        named.join("、")
    ))
}

fn reason(reason: InvalidReason) -> &'static str {
    match reason {
        InvalidReason::Encoding => "UTF-8 でない",
        InvalidReason::FieldCount => "列の数が違う",
        InvalidReason::Empty => "読みか表記が空",
        InvalidReason::Escape => "\\ の書き方が違う",
        InvalidReason::Okurigana => "送り仮名の書き方が違う",
        InvalidReason::Hide => "! の行はユーザー辞書だけで使える",
        InvalidReason::ConjugationType => "知らない活用型",
        InvalidReason::Cost => "コストが 0 以上の整数でない",
        InvalidReason::Placeholder => "数の置き場所の書き方が違う",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kanaemi-settings-checks-{}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn a_dictionary_whose_lines_all_read_has_nothing_to_say() {
        let path = temp_path("fine.tsv");
        fs::write(&path, "# 説明\nきしゃ\t記者\n").unwrap();
        assert_eq!(invalid_lines(&path, false), None);
    }

    #[test]
    fn invalid_lines_are_counted_and_the_first_are_named() {
        let path = temp_path("broken.tsv");
        fs::write(
            &path,
            "きしゃ\n!きしゃ\t汽車\nきしゃ\t記者\t\t-1\nか\t書\t謎\na\tb\tc\td\te\tf\n",
        )
        .unwrap();
        assert_eq!(
            invalid_lines(&path, false).unwrap(),
            "読めない行が 5 行あります（1 行目：列の数が違う、2 行目：! の行はユーザー辞書だけで使える、3 行目：コストが 0 以上の整数でない、…）"
        );
    }

    #[test]
    fn a_line_that_is_not_utf_8_is_named_as_such() {
        let path = temp_path("encoding.tsv");
        fs::write(&path, b"\xa4\xad\xa4\xb7\xa4\xe3\t\xb5\xad\xbc\xd4\n").unwrap();

        let said = invalid_lines(&path, false);

        assert_eq!(
            said.as_deref(),
            Some("読めない行が 1 行あります（1 行目：UTF-8 でない）")
        );
    }

    #[test]
    fn hide_lines_are_fine_in_the_user_custom_dictionary() {
        let path = temp_path("custom.tsv");
        fs::write(&path, "!きしゃ\t汽車\n").unwrap();
        assert_eq!(invalid_lines(&path, true), None);
    }
}
