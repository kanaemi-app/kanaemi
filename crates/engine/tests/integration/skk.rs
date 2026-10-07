use kanaemi_engine::OkuriHead::Row;
use kanaemi_engine::{Dictionary, Entry, SkkError, TextDictionary, skk_to_text};

fn read(skk: impl AsRef<[u8]>) -> TextDictionary {
    let text = skk_to_text(skk).unwrap();
    let (dictionary, invalid) = TextDictionary::parse(&text);
    assert_eq!(invalid, [], "{text}");
    dictionary
}

fn entry(surface: &str, cost: u32) -> Entry {
    Entry {
        surface: surface.to_owned(),
        conjugation: None,
        cost,
    }
}

const UTF8: &str = ";; -*- coding: utf-8 -*-\n";

#[test]
fn an_okuri_nasi_heading_becomes_words_costed_by_their_order() {
    let d = read(format!(
        "{UTF8};; okuri-nasi entries.\nきしゃ /記者/汽車/貴社/\n"
    ));

    assert_eq!(
        d.lookup("きしゃ"),
        [entry("記者", 0), entry("汽車", 1), entry("貴社", 2)]
    );
}

#[test]
fn what_follows_a_semicolon_is_dropped() {
    let text = skk_to_text(format!("{UTF8}きしゃ /記者;報道/\n")).unwrap();

    assert_eq!(text, "きしゃ\t記者\t\t0\n");
}

#[test]
fn an_okuri_ari_heading_becomes_okurigana_words_of_its_row() {
    let d = read(format!(
        "{UTF8};; okuri-ari entries.\nかk /書/欠/\nおもt /思/\n"
    ));

    assert_eq!(d.okuri("か", Row('k')), [entry("書", 0), entry("欠", 1)]);
    assert_eq!(d.okuri("おも", Row('t')), [entry("思", 0)]);
}

#[test]
fn an_okuri_ari_heading_is_written_with_its_row_and_bare_surfaces() {
    let text = skk_to_text(format!("{UTF8}かk /書/欠/\n")).unwrap();

    assert_eq!(text, "か*k\t書\t\t0\nか*k\t欠\t\t1\n");
}

#[test]
fn the_c_of_ち_is_read_as_t() {
    let d = read(format!("{UTF8}もc /持/\n"));

    assert_eq!(d.okuri("も", Row('t')), [entry("持", 0)]);
}

#[test]
fn an_okuri_ari_heading_whose_letter_has_no_row_is_left_out() {
    let text = skk_to_text(format!("{UTF8}かq /書/\nきしゃ /記者/\n")).unwrap();

    assert_eq!(text, "きしゃ\t記者\t\t0\n");
}

#[test]
fn an_okuri_ari_heading_of_x_is_left_out() {
    let text = skk_to_text(format!("{UTF8}かx /書/\nきしゃ /記者/\n")).unwrap();

    assert_eq!(text, "きしゃ\t記者\t\t0\n");
}

#[test]
fn s_expressions_strict_okurigana_and_affix_headings_are_left_out() {
    let d = read(format!(
        "{UTF8}かk /書/[け/描/]/\nよる /(concat \"夜\\057\")/夜/\nお> /御/\n>さん /さん/\n"
    ));

    assert_eq!(d.okuri("か", Row('k')), [entry("書", 0)]);
    assert_eq!(d.lookup("よる"), [entry("夜", 0)]);
    assert_eq!(d.lookup("お>"), []);
    assert_eq!(d.lookup(">さん"), []);
}

#[test]
fn a_repeated_candidate_keeps_its_first_place() {
    let d = read(format!("{UTF8}きしゃ /記者/汽車/記者/\n"));

    assert_eq!(d.lookup("きしゃ"), [entry("記者", 0), entry("汽車", 1)]);
}

#[test]
fn a_heading_with_a_number_becomes_numeric_items_named_for_each_notation() {
    let text = skk_to_text(format!("{UTF8}#こ /#0個/#1個/#2個/#3個/#5個/#8個/\n")).unwrap();

    assert_eq!(
        text,
        "{}こ\t{}個\t\t0\n\
         {}こ\t{wide-num}個\t\t1\n\
         {}こ\t{kanji-num}個\t\t2\n\
         {}こ\t{kanji}個\t\t3\n\
         {}こ\t{daiji}個\t\t4\n\
         {}こ\t{grouped-num}個\t\t5\n"
    );
    assert_eq!(TextDictionary::parse(&text).1, [], "{text}");
}

#[test]
fn a_candidate_with_a_number_no_notation_writes_is_left_out() {
    let text = skk_to_text(format!("{UTF8}#ばん /#4番/#9番/第#1/#1と#1/番号/C#/\n")).unwrap();

    assert_eq!(
        text,
        "{}ばん\t第{wide-num}\t\t0\n{}ばん\t番号\t\t1\n{}ばん\tC#\t\t2\n"
    );
}

#[test]
fn an_okuri_ari_heading_with_a_number_is_left_out() {
    let text = skk_to_text(format!("{UTF8}#かk /#1書/\nきしゃ /記者/\n")).unwrap();

    assert_eq!(text, "きしゃ\t記者\t\t0\n");
}

#[test]
fn braces_and_number_signs_outside_numeric_headings_stay_as_they_are() {
    let text = skk_to_text(format!("{UTF8}かっこ /{{/#1/\nしゃーぷ{{ /}}/\n")).unwrap();

    assert_eq!(
        text,
        "かっこ\t\\{\t\t0\nかっこ\t#1\t\t1\nしゃーぷ\\{\t}\t\t0\n"
    );
    assert_eq!(TextDictionary::parse(&text).1, [], "{text}");
}

#[test]
fn a_heading_or_candidate_holding_a_placeholder_mark_is_left_out() {
    let text = skk_to_text(format!(
        "{UTF8}\u{FDD0} /か/\nか /\u{FDD0}kanji\u{FDD1}/蚊/\n"
    ))
    .unwrap();

    assert_eq!(text, "か\t蚊\t\t0\n");
}

#[test]
fn a_heading_starting_with_a_byte_order_mark_is_left_out() {
    let text = skk_to_text(format!("{UTF8}\u{FEFF}か /蚊/\nか /化/\n")).unwrap();

    assert_eq!(text, "か\t化\t\t0\n");
}

#[test]
fn comments_and_broken_lines_are_skipped() {
    let d = read(format!("{UTF8};; a comment\nnot an entry\nきしゃ /記者/\n"));

    assert_eq!(d.lookup("きしゃ"), [entry("記者", 0)]);
}

/// きしゃ /記者/ in EUC-JP.
const KISHA_EUC_JP: [u8; 14] = [
    0xa4, 0xad, 0xa4, 0xb7, 0xa4, 0xe3, b' ', b'/', 0xb5, 0xad, 0xbc, 0xd4, b'/', b'\n',
];

/// きしゃ /記者/ in Shift_JIS.
const KISHA_SHIFT_JIS: [u8; 14] = [
    0x82, 0xab, 0x82, 0xb5, 0x82, 0xe1, b' ', b'/', 0x8b, 0x4c, 0x8e, 0xd2, b'/', b'\n',
];

/// きしゃ /記者/ in ISO-2022-JP.
const KISHA_ISO_2022_JP: &[u8] = b"\x1b$B$-$7$c\x1b(B /\x1b$B5-<T\x1b(B/\n";

fn with_coding(first_line: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes = first_line.as_bytes().to_vec();
    bytes.push(b'\n');
    bytes.extend_from_slice(body);
    bytes
}

#[test]
fn a_dictionary_without_a_coding_is_read_as_euc_jp() {
    let d = read(KISHA_EUC_JP);

    assert_eq!(d.lookup("きしゃ"), [entry("記者", 0)]);
}

#[test]
fn the_coding_of_the_first_line_names_the_encoding_as_emacs_does() {
    let cases: [(&str, &[u8]); 5] = [
        (";; -*- coding: euc-japan-unix -*-", &KISHA_EUC_JP),
        (
            ";; -*- coding: japanese-shift-jis-dos -*-",
            &KISHA_SHIFT_JIS,
        ),
        (";; -*- coding: cp932 -*-", &KISHA_SHIFT_JIS),
        (";; -*- coding: junet-unix -*-", KISHA_ISO_2022_JP),
        (
            ";; -*- mode: fundamental; coding: utf-8-unix; -*-",
            "きしゃ /記者/\n".as_bytes(),
        ),
    ];
    for (first_line, body) in cases {
        let d = read(with_coding(first_line, body));

        assert_eq!(d.lookup("きしゃ"), [entry("記者", 0)], "{first_line}");
    }
}

#[test]
fn an_unknown_coding_is_read_as_euc_jp() {
    let d = read(with_coding(
        ";; -*- coding: no-such-coding -*-",
        &KISHA_EUC_JP,
    ));

    assert_eq!(d.lookup("きしゃ"), [entry("記者", 0)]);
}

#[test]
fn lines_ending_in_a_lone_carriage_return_are_split_like_any_other() {
    let d = read(";; -*- coding: utf-8-mac -*-\rきしゃ /記者/\r");

    assert_eq!(d.lookup("きしゃ"), [entry("記者", 0)]);
}

#[test]
fn lines_ending_in_crlf_are_split_like_any_other() {
    let d = read(";; -*- coding: utf-8-dos -*-\r\nきしゃ /記者/\r\nかk /書/\r\n");

    assert_eq!(d.lookup("きしゃ"), [entry("記者", 0)]);
}

#[test]
fn bytes_the_named_encoding_cannot_decode_fail_the_import_with_their_line() {
    // Row 14 of JIS X 0208, where AE A1 falls, holds no character.
    let skk = with_coding(
        ";; -*- coding: euc-jp -*-",
        b"\xa4\xaf\r\xa4\xaf /\xae\xa1/\n",
    );

    let imported = skk_to_text(skk);

    assert_eq!(
        imported,
        Err(SkkError::Undecodable {
            encoding: "EUC-JP",
            line: 3
        })
    );
}

#[test]
fn an_import_never_holds_a_replacement_character_it_made() {
    let skk = with_coding(";; -*- coding: sjis -*-", b"\x82\xa9 /\xfc\xfc/\n");

    assert!(skk_to_text(skk).is_err());
}

#[test]
fn a_jis_x_0213_dictionary_is_not_imported_rather_than_read_as_other_kanji() {
    // ED 40 is 硃 in Shift_JIS-2004 but 纊 in Windows-31J.
    let skk = with_coding(
        ";; -*- coding: shift_jis-2004 -*-",
        b"\x82\xa9 /\xed\x40/\n",
    );

    let imported = skk_to_text(skk);

    assert_eq!(
        imported,
        Err(SkkError::UnsupportedEncoding {
            name: "shift_jis-2004".to_owned()
        })
    );
}

#[test]
fn an_euc_jis_2004_dictionary_is_not_imported_even_where_its_bytes_read_as_euc_jp() {
    let skk = with_coding(";; -*- coding: euc-jis-2004 -*-", &KISHA_EUC_JP);

    assert_eq!(
        skk_to_text(skk),
        Err(SkkError::UnsupportedEncoding {
            name: "euc-jis-2004".to_owned()
        })
    );
}
