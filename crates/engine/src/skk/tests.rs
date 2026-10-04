use encoding_rs::{EUC_JP, Encoding, ISO_2022_JP, SHIFT_JIS, UTF_8};

use super::*;

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
