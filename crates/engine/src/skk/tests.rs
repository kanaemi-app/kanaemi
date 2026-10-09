use encoding_rs::{EUC_JP, Encoding, ISO_2022_JP, SHIFT_JIS, UTF_8};
use proptest::collection::vec;
use proptest::prelude::*;

use super::*;
use crate::{Dictionary, OkuriHead, TextDictionary};

fn named(label: &str) -> &'static Encoding {
    encoding(&format!(";; -*- coding: {label} -*-")).unwrap()
}

fn assert_family(family: &'static Encoding, labels: &[&str]) {
    for label in labels {
        assert_eq!(named(label), family, "{label}");
    }
}

#[test]
fn utf_8_and_its_variants_are_utf_8() {
    assert_family(
        UTF_8,
        &[
            "utf-8",
            "utf8",
            "utf-8-with-signature",
            "utf-8-auto",
            "utf-8-emacs",
            "prefer-utf-8",
            "mule-utf-8",
        ],
    );
}

#[test]
fn shift_jis_and_its_variants_are_shift_jis() {
    assert_family(
        SHIFT_JIS,
        &[
            "sjis",
            "shift_jis",
            "shift-jis",
            "cp932",
            "japanese-shift-jis",
            "japanese-cp932",
            "sjis-mac",
        ],
    );
}

#[test]
fn euc_japan_and_its_variants_are_euc_jp() {
    assert_family(
        EUC_JP,
        &["euc-jp", "euc-japan", "euc-japan-1990", "japanese-iso-8bit"],
    );
}

#[test]
fn iso_2022_jp_and_its_aliases_are_iso_2022_jp() {
    assert_family(ISO_2022_JP, &["iso-2022-jp", "junet", "japanese-iso-7bit"]);
}

#[test]
fn an_end_of_line_suffix_does_not_change_the_encoding() {
    for (label, family) in [
        ("utf-8", UTF_8),
        ("sjis", SHIFT_JIS),
        ("euc-japan", EUC_JP),
        ("junet", ISO_2022_JP),
    ] {
        for eol in ["-unix", "-dos", "-mac"] {
            assert_eq!(named(&format!("{label}{eol}")), family, "{label}{eol}");
        }
    }
}

#[test]
fn a_coding_is_named_in_any_case() {
    assert_family(UTF_8, &["UTF-8", "Utf-8-Unix"]);
    assert_family(SHIFT_JIS, &["Shift_JIS", "CP932-DOS"]);
    assert_family(EUC_JP, &["EUC-JP"]);
    assert_family(ISO_2022_JP, &["ISO-2022-JP"]);
}

#[test]
fn an_unknown_coding_is_euc_jp() {
    assert_family(EUC_JP, &["latin-1", "no-such-coding", "undecided", ""]);
}

#[test]
fn a_first_line_without_a_coding_is_euc_jp() {
    assert_eq!(encoding(";; okuri-ari entries."), Ok(EUC_JP));
    assert_eq!(encoding(""), Ok(EUC_JP));
}

#[test]
fn the_coding_ends_at_a_semicolon_or_a_space() {
    assert_eq!(encoding(";; -*- coding: utf-8; mode: text -*-"), Ok(UTF_8));
    assert_eq!(
        encoding(";; -*- mode: text; coding:sjis-unix -*-"),
        Ok(SHIFT_JIS)
    );
}

#[test]
fn jis_x_0213_codings_are_not_supported() {
    for label in [
        "euc-jis-2004",
        "euc-jisx0213",
        "shift_jis-2004",
        "shift_jisx0213",
        "japanese-shift-jis-2004",
        "iso-2022-jp-2004",
        "iso-2022-jp-3",
        "EUC-JIS-2004-unix",
        "shift_jisx0213-dos",
    ] {
        let found = encoding(&format!(";; -*- coding: {label} -*-"));

        assert_eq!(
            found,
            Err(SkkError::UnsupportedEncoding {
                name: label.to_owned()
            }),
            "{label}"
        );
    }
}

// Robustness: whatever the bytes, importing either fails with a reason or
// gives lines a text dictionary reads whole.

/// Pieces of an SKK line that the format gives a meaning to.
#[rustfmt::skip]
const PIECES: &[&str] = &[
    // Headings and candidates.
    "か", "き", "しゃ", "っ", "記者", "書", "a", "k", "c", "x", "1", "#", "#0", "#1", "#3", "#4",
    // Separators and marks.
    " ", " /", "/", ";", "[", "]", "(", ")", ">", "\t", "\r", "\n", "\r\n", "\u{FEFF}",
    // What a text dictionary gives a meaning to.
    "*", "!", "\\", "{", "}", "{}", "\u{FDD0}", "\u{FDD1}", "e\u{301}",
];

fn skk_text() -> impl Strategy<Value = String> {
    let coding = proptest::sample::select(vec![
        "",
        ";; -*- coding: utf-8 -*-\n",
        ";; -*- coding: euc-jp -*-\n",
        ";; -*- coding: shift_jis -*-\n",
        ";; -*- coding: iso-2022-jp -*-\n",
    ]);
    let line = vec(proptest::sample::select(PIECES), 0..10).prop_map(|pieces| pieces.concat());
    (coding, vec(line, 0..10)).prop_map(|(coding, lines)| format!("{coding}{}", lines.join("\n")))
}

fn assert_reads_whole(text: &str) {
    let (_, invalid) = TextDictionary::parse(text);
    assert_eq!(invalid, [], "{text:?}");
}

proptest! {
    #[test]
    fn any_bytes_import_into_lines_a_text_dictionary_reads(bytes in vec(any::<u8>(), 0..512)) {
        if let Ok(text) = skk_to_text(&bytes) {
            assert_reads_whole(&text);
        }
    }

    #[test]
    fn any_skk_text_imports_into_lines_a_text_dictionary_reads(text in skk_text()) {
        if let Ok(text) = skk_to_text(text.as_bytes()) {
            assert_reads_whole(&text);
        }
    }
}

#[test]
fn a_dictionary_without_coding_is_in_the_encoding_its_writer_uses() {
    let utf8 = ";; okuri-nasi entries.\nきしゃ /記者/\n";
    let read = read_skk_dictionary(utf8, SkkEncoding::Utf8).unwrap();
    assert!(read.text.starts_with("きしゃ\t記者"), "{}", read.text);
    assert!(read_skk_dictionary(utf8, SkkEncoding::EucJp).is_err());
    let marked = format!(";; -*- coding: utf-8 -*-\n{utf8}");
    assert!(
        read_skk_dictionary(&marked, SkkEncoding::EucJp).is_ok(),
        "coding: wins"
    );
}

#[test]
fn what_an_skk_dictionary_loses_is_listed_by_line() {
    let read = read_skk_dictionary(
        ";; -*- coding: utf-8 -*-\n\
         きしゃ /記者/(concat \"a\")/\n\
         かq /書/\n\
         >あ /亜/\n\
         けs /消/[す/消/]/\n",
        SkkEncoding::EucJp,
    )
    .unwrap();
    let skipped: Vec<(usize, &str)> = read
        .skipped
        .iter()
        .map(|s| (s.line, s.text.as_str()))
        .collect();
    assert_eq!(
        skipped,
        [
            (2, "きしゃ /(concat \"a\")/"),
            (3, "かq /書/"),
            (4, ">あ /亜/"),
        ],
        "a strict okurigana block repeats what is there and is not lost"
    );
    assert_eq!(
        read.text,
        skk_to_text(";; -*- coding: utf-8 -*-\nきしゃ /記者/\nけs /消/\n").unwrap()
    );
}

fn user_custom(text: &str) -> TextDictionary {
    let (dictionary, invalid) = TextDictionary::parse_user_custom(text);
    assert_eq!(invalid, [], "{text}");
    dictionary
}

#[test]
fn a_user_custom_dictionary_is_written_as_an_skk_dictionary() {
    let d = user_custom(
        "きしゃ\t記者\n\
         きしゃ\t汽車\n\
         か\t書\t五段-カ行\n\
         たべ\t食べ\t下一段-バ行\n\
         み\t見\t上一段-マ行\n\
         たか\t高\t形容詞\n\
         あい\t愛\tサ行変格\n\
         ほ*s\t干\n\
         か*っ\t勝っ\n\
         {}こ\t{}個\n\
         !ねこ\t猫\n\
         すらっしゅ\ta/b\n",
    );
    let export = write_skk_dictionary(&d, SkkEncoding::Utf8);
    assert_eq!(
        String::from_utf8(export.bytes).unwrap(),
        ";; -*- coding: utf-8 -*-\n\
         ;; okuri-ari entries.\n\
         みr /見/\n\
         ほs /干/\n\
         たかi /高/\n\
         たb /食/\n\
         かt /勝/\n\
         かk /書/\n\
         あいs /愛/\n\
         ;; okuri-nasi entries.\n\
         きしゃ /汽車/記者/\n"
    );
    // {}こ, the hidden pair, and the word with a slash.
    assert_eq!((export.written, export.skipped), (9, 3));
}

#[test]
fn an_skk_dictionary_written_out_comes_back_in() {
    let d = user_custom("きしゃ\t記者\nか\t書\t五段-カ行\nたべ\t食べ\t下一段-バ行\n");
    let text = skk_to_text(write_skk_dictionary(&d, SkkEncoding::Utf8).bytes).unwrap();
    let (back, invalid) = TextDictionary::parse(&text);
    assert_eq!(invalid, [], "{text}");
    assert_eq!(back.lookup("きしゃ")[0].surface, "記者");
    assert_eq!(back.okuri("か", OkuriHead::Row('k'))[0].surface, "書");
    assert_eq!(back.okuri("た", OkuriHead::Row('b'))[0].surface, "食");
}

#[test]
fn an_skk_dictionary_written_in_euc_jp_says_so_and_leaves_out_what_it_cannot_hold() {
    let d = user_custom("きしゃ\t記者\nえもじ\t😀\nつちよし\t𠮷\nえん\t¥\n");
    let export = write_skk_dictionary(&d, SkkEncoding::EucJp);
    let (text, _, errors) = EUC_JP.decode(&export.bytes);
    assert!(!errors);
    assert!(text.starts_with(";; -*- coding: euc-jp -*-\n"), "{text}");
    assert!(text.contains("きしゃ /記者/\n"), "{text}");
    assert_eq!((export.written, export.skipped), (1, 3), "{text}");
    let back = skk_to_text(&export.bytes).unwrap();
    assert!(back.starts_with("きしゃ\t記者"), "{back}");
}

#[test]
fn a_byte_order_mark_tells_the_encoding_over_the_writers_default() {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(
        ";; okuri-nasi entries.\r\nきしゃ /記者/\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes),
    );
    let read = read_skk_dictionary(&bytes, SkkEncoding::Utf8).unwrap();
    assert!(read.text.starts_with("きしゃ\t記者"), "{}", read.text);
}

#[test]
fn a_line_that_is_no_entry_is_listed_as_unreadable() {
    let read = read_skk_dictionary("きしゃ /記者/\nねこ 猫\n\n", SkkEncoding::Utf8).unwrap();
    let skipped: Vec<(usize, SkipReason, &str)> = read
        .skipped
        .iter()
        .map(|s| (s.line, s.reason, s.text.as_str()))
        .collect();
    assert_eq!(skipped, [(2, SkipReason::Unreadable, "ねこ 猫")]);
}

#[test]
fn a_reading_skk_would_read_otherwise_is_not_written() {
    // A number, an affix, and okurigana to SKK.
    let d = user_custom("\\#こ\t個\nあ>\t亜\nかなk\t仮名\nきしゃ\t記者\n");
    let export = write_skk_dictionary(&d, SkkEncoding::Utf8);
    let text = String::from_utf8(export.bytes).unwrap();
    assert_eq!((export.written, export.skipped), (1, 3), "{text}");
}

#[test]
fn a_word_filed_under_the_row_of_small_kana_is_not_written() {
    let d = user_custom("か*x\t亜\nきしゃ\t記者\n");
    let export = write_skk_dictionary(&d, SkkEncoding::Utf8);
    assert_eq!((export.written, export.skipped), (1, 1));
}

#[test]
fn candidates_meeting_under_one_heading_go_cheapest_first() {
    let (d, invalid) = TextDictionary::parse("たべ\t食べ\t下一段-バ行\t0\nた*b\t喰\t\t100\n");
    assert_eq!(invalid, []);
    let text = String::from_utf8(write_skk_dictionary(&d, SkkEncoding::Utf8).bytes).unwrap();
    assert!(text.contains("たb /食/喰/\n"), "{text}");
}
