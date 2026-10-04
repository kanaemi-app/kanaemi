use super::*;
use crate::numeric::{CLOSE, OPEN};
use crate::test_support::{Words, entry, stems};

fn parse(text: &str) -> TextDictionary {
    let (dictionary, invalid) = TextDictionary::parse(text);
    assert_eq!(invalid, [], "{text:?}");
    dictionary
}

fn invalid(text: &str) -> Vec<InvalidReason> {
    TextDictionary::parse(text)
        .1
        .into_iter()
        .map(|line| line.reason)
        .collect()
}

fn surfaces(dictionary: &TextDictionary, reading: &str) -> Vec<String> {
    dictionary
        .words(reading)
        .into_iter()
        .map(|e| e.surface)
        .collect()
}

#[test]
fn a_word_line_gives_its_surface() {
    let d = parse("きしゃ\t記者\nきしゃ\t汽車");
    assert_eq!(d.words("きしゃ"), [entry("汽車", 0), entry("記者", 1)]);
}

#[test]
fn later_lines_cost_less_within_a_reading() {
    let d = parse("かく\t角\nかく\t書く\nかく\t核");
    assert_eq!(surfaces(&d, "かく"), ["核", "書く", "角"]);
}

#[test]
fn a_repeated_line_counts_once_and_takes_its_last_place() {
    let d = parse("かく\t角\t\t30\nかく\t核\nかく\t角");
    assert_eq!(d.words("かく"), [entry("角", 0), entry("核", 1)]);
}

#[test]
fn blank_lines_comments_bom_and_crlf_are_accepted() {
    let d = parse("\u{feff}# words\r\n\r\nきしゃ\t記者\r\n");
    assert_eq!(surfaces(&d, "きしゃ"), ["記者"]);
}

#[test]
fn escapes() {
    let d = parse("\\#\tハッシュ\n\\!\tビックリ\nあ\\*\tア\\tイ\\nウ\\\\");
    assert_eq!(surfaces(&d, "#"), ["ハッシュ"]);
    assert_eq!(surfaces(&d, "!"), ["ビックリ"]);
    assert_eq!(surfaces(&d, "あ*"), ["ア\tイ\nウ\\"]);
}

#[test]
fn readings_and_surfaces_are_normalized_to_nfc() {
    let d = parse("か\u{3099}\tカ\u{3099}");
    assert_eq!(surfaces(&d, "が"), ["ガ"]);
}

#[test]
fn an_okurigana_line_is_found_by_its_stem() {
    let d = parse("か*く\t書く\nか*く\t欠く\nか*け\t掛け\nか*つ\t勝つ");
    assert_eq!(
        d.okuri("か", 'k'),
        [entry("欠", 0), entry("掛", 0), entry("書", 1)]
    );
    assert_eq!(d.okuri("か", 't'), [entry("勝", 0)]);
    assert_eq!(d.okuri("か", 's'), []);
    assert_eq!(d.words("かく"), []);
}

#[test]
fn invalid_lines_are_skipped_and_reported_with_their_line_numbers() {
    let (d, invalid) = TextDictionary::parse("きしゃ\n\nきしゃ\t記者\n\t空");
    assert_eq!(
        invalid,
        [
            InvalidLine {
                line: 1,
                reason: InvalidReason::FieldCount
            },
            InvalidLine {
                line: 4,
                reason: InvalidReason::Empty
            },
        ]
    );
    assert_eq!(surfaces(&d, "きしゃ"), ["記者"]);
}

#[test]
fn reasons_for_invalid_lines() {
    use InvalidReason::*;
    assert_eq!(invalid("a\tb\tc\td\te"), [FieldCount]);
    assert_eq!(invalid("きしゃ\t"), [Empty]);
    assert_eq!(invalid("き\\qしゃ\t記者"), [Escape]);
    assert_eq!(invalid("き\\#しゃ\t記者"), [Escape]);
    assert_eq!(invalid("きしゃ\t記\\*者"), [Escape]);
    assert_eq!(invalid("か*く*\t書く"), [Okurigana]);
    assert_eq!(invalid("か*くく\t書くく"), [Okurigana]);
    assert_eq!(invalid("か*\t書く"), [Okurigana]);
    assert_eq!(invalid("か*く\t書け"), [Okurigana]);
    assert_eq!(invalid("か*く\t書く\t五段-カ行"), [Okurigana]);
    assert_eq!(invalid("!きしゃ\t汽車"), [Hide]);
}

#[test]
fn a_conjugating_line_with_an_unknown_type_is_invalid() {
    assert_eq!(
        invalid("か\t書\t存在しない型"),
        [InvalidReason::ConjugationType]
    );
}

#[test]
fn the_user_custom_dictionary_hides_pairs() {
    let (d, invalid) = TextDictionary::parse_user_custom("!きしゃ\t汽車\n!かく\t書く");
    assert_eq!(invalid, []);
    assert!(d.is_hidden("きしゃ", "汽車"));
    assert!(d.is_hidden("かく", "書く"));
    assert!(!d.is_hidden("きしゃ", "記者"));
}

#[test]
fn a_hide_line_cannot_mark_okurigana() {
    let (_, invalid) = TextDictionary::parse_user_custom("!か*く\t書く");
    assert_eq!(invalid[0].reason, InvalidReason::Okurigana);
}

#[test]
fn a_hide_line_and_a_word_line_for_the_same_pair_follow_the_last_one() {
    let (d, _) = TextDictionary::parse_user_custom("きしゃ\t汽車\n!きしゃ\t汽車");
    assert!(d.is_hidden("きしゃ", "汽車"));
    assert_eq!(surfaces(&d, "きしゃ"), Vec::<String>::new());

    let (d, _) = TextDictionary::parse_user_custom("!きしゃ\t汽車\nきしゃ\t汽車");
    assert!(!d.is_hidden("きしゃ", "汽車"));
    assert_eq!(surfaces(&d, "きしゃ"), ["汽車"]);
}

#[test]
fn a_hide_line_stays_after_a_conjugating_line_of_the_same_reading_and_surface() {
    let (d, invalid) = TextDictionary::parse_user_custom("!か\t書\nか\t書\t五段-カ行");
    assert_eq!(invalid, []);
    assert!(d.is_hidden("か", "書"));
    assert_eq!(stems(&d, "か")[0].surface, "書");
}

#[test]
fn a_later_okurigana_line_of_the_hidden_pair_clears_the_hide_line() {
    let (d, _) = TextDictionary::parse_user_custom("!かく\t書く\nか*く\t書く");
    assert!(!d.is_hidden("かく", "書く"));
}

#[test]
fn okurigana_must_start_with_a_kana_of_the_okuri_table() {
    for line in ["か*ク\t書ク", "か*ゔぁ\t書ゔぁ"] {
        assert_eq!(invalid(line), [InvalidReason::Okurigana], "{line}");
    }
    let d = parse("か*っ\t勝っ\nい*ゐ\t居ゐ\nう*ゔ\t鵜ゔ");
    assert_eq!(d.okuri("か", 't'), [entry("勝", 0)]);
    assert_eq!(d.okuri("い", 'w'), [entry("居", 0)]);
    assert_eq!(d.okuri("う", 'v'), [entry("鵜", 0)]);
}

#[test]
fn small_kana_start_okurigana_of_their_own_row() {
    let d = parse("たち*ゃ\t達ゃ\nたち*や\t立ちや");
    assert_eq!(d.okuri("たち", 'x'), [entry("達", 0)]);
    assert_eq!(d.okuri("たち", 'y'), [entry("立ち", 0)]);
    for line in ["か*ぁ\t書ぁ", "か*ょ\t書ょ", "か*ゎ\t書ゎ", "か*ゖ\t書ゖ"] {
        assert_eq!(invalid(line), [], "{line}");
    }
}

#[test]
fn a_line_is_formatted_with_escapes() {
    let line = |reading, surface| ItemLine {
        reading,
        surface,
        ..ItemLine::default()
    };
    assert_eq!(line("#あ", "ア\tイ\\").to_string(), "\\#あ\tア\\tイ\\\\");
    assert_eq!(line("!あ", "亜").to_string(), "\\!あ\t亜");
    assert_eq!(line("あ*い", "亜").to_string(), "あ\\*い\t亜");
}

#[test]
fn a_line_has_columns_up_to_its_last_given_field() {
    let base = ItemLine {
        reading: "きしゃ",
        surface: "記者",
        ..ItemLine::default()
    };
    assert_eq!(base.to_string(), "きしゃ\t記者");
    let conjugating = ItemLine {
        reading: "か",
        surface: "書",
        conjugation: Some("五段-カ行"),
        ..ItemLine::default()
    };
    assert_eq!(conjugating.to_string(), "か\t書\t五段-カ行");
    let costed = ItemLine {
        cost: Some(40),
        ..base.clone()
    };
    assert_eq!(costed.to_string(), "きしゃ\t記者\t\t40");
    let okurigana = ItemLine {
        reading: "か",
        okurigana: Some("く"),
        surface: "書く",
        cost: Some(7),
        ..ItemLine::default()
    };
    assert_eq!(okurigana.to_string(), "か*く\t書く\t\t7");
}

#[test]
fn every_line_reads_back_as_it_was_written() {
    let tricky = [
        "#あ", "!あ", "あ\\い", "あ\tい", "あ\nい", "あ*い", "\\#", "*", "{", "}", "\\{",
    ];
    for reading in tricky {
        for surface in tricky {
            let line = ItemLine {
                reading,
                surface,
                cost: Some(3),
                ..ItemLine::default()
            };
            let (d, invalid) = TextDictionary::parse(line.to_string());
            assert_eq!(invalid, [], "{line}");
            assert_eq!(d.words(reading), [entry(surface, 3)], "{line}");
        }
        let stem = ItemLine {
            reading,
            surface: "書",
            conjugation: Some("五段-カ行"),
            cost: Some(5),
            ..ItemLine::default()
        };
        let (d, invalid) = TextDictionary::parse(stem.to_string());
        assert_eq!(invalid, [], "{stem}");
        assert_eq!(stems(&d, reading)[0].cost, 5, "{stem}");
        let okuri = ItemLine {
            reading,
            okurigana: Some("く"),
            surface: "書く",
            ..ItemLine::default()
        };
        let (d, invalid) = TextDictionary::parse(okuri.to_string());
        assert_eq!(invalid, [], "{okuri}");
        assert_eq!(d.okuri(reading, 'k'), [entry("書", 0)], "{okuri}");
    }
}

#[test]
fn a_conjugating_line_is_found_by_its_stem_and_shares_costs_with_its_reading() {
    let d = parse("か\t蚊\nか\t書\t五段-カ行");
    assert_eq!(
        stems(&d, "か"),
        [Entry {
            conjugation: Some("五段-カ行".to_owned()),
            ..entry("書", 0)
        }]
    );
    assert_eq!(d.words("か"), [entry("蚊", 1)]);
}

#[test]
fn the_same_surface_with_another_conjugation_type_is_another_entry() {
    let d = parse("か\t書\t五段-カ行\nか\t書");
    assert_eq!(d.words("か"), [entry("書", 0)]);
    assert_eq!(stems(&d, "か")[0].cost, 1);
}

#[test]
fn a_cost_column_sets_the_cost() {
    let d = parse("きしゃ\t記者\t\t120\nきしゃ\t汽車\t\t800");
    assert_eq!(d.words("きしゃ"), [entry("記者", 120), entry("汽車", 800)]);
}

#[test]
fn a_cost_column_applies_to_stems_and_okurigana_lines() {
    let d = parse("か\t書\t五段-カ行\t300\nか*く\t欠く\t\t40");
    assert_eq!(stems(&d, "か")[0].cost, 300);
    assert_eq!(d.okuri("か", 'k')[0].cost, 40);
}

#[test]
fn a_line_without_a_cost_takes_its_place_in_the_reading() {
    let d = parse("きしゃ\t記者\t\t120\nきしゃ\t汽車\t\nきしゃ\t帰社");
    assert_eq!(
        d.words("きしゃ"),
        [entry("帰社", 0), entry("汽車", 1), entry("記者", 120)]
    );
}

#[test]
fn an_empty_conjugation_type_is_no_conjugation() {
    let d = parse("きしゃ\t記者\t");
    assert_eq!(d.words("きしゃ"), [entry("記者", 0)]);
}

#[test]
fn the_last_line_of_a_pair_gives_its_cost() {
    let d = parse("きしゃ\t記者\t\t120\nきしゃ\t記者\t\t30");
    assert_eq!(d.words("きしゃ"), [entry("記者", 30)]);
}

#[test]
fn a_cost_is_a_non_negative_integer_that_fits_32_bits() {
    assert_eq!(invalid("きしゃ\t記者\t\t4294967295"), []);
    for cost in ["-1", "1.5", "+3", " 3", "x", "4294967296", "１"] {
        assert_eq!(
            invalid(&format!("きしゃ\t記者\t\t{cost}")),
            [InvalidReason::Cost],
            "{cost}"
        );
    }
}

#[test]
fn appending_a_line_behaves_as_if_it_had_been_in_the_file() {
    let (mut d, _) = TextDictionary::parse_user_custom("きしゃ\t記者");
    d.append("きしゃ\t汽車").unwrap();
    assert_eq!(surfaces(&d, "きしゃ"), ["汽車", "記者"]);
    d.append("!きしゃ\t記者").unwrap();
    assert_eq!(surfaces(&d, "きしゃ"), ["汽車"]);
    assert_eq!(d.append("きしゃ"), Err(InvalidReason::FieldCount));
}

#[test]
fn a_hide_line_escapes_its_reading_as_a_literal_one() {
    assert_eq!(TextDictionary::hide_line("きしゃ", "汽車"), "!きしゃ\t汽車");
    assert_eq!(TextDictionary::hide_line("あ*", "亜"), "!あ\\*\t亜");
    assert_eq!(TextDictionary::hide_line("#あ", "亜"), "!\\#あ\t亜");
}

fn placeholder(name: &str) -> String {
    format!("{OPEN}{name}{CLOSE}")
}

#[test]
fn a_numeric_line_keeps_its_placeholders_apart_from_literal_braces() {
    let d = parse("{}こ\t{kanji}個\n{}\\{\t\\{{}}");
    assert_eq!(
        surfaces(&d, &format!("{}こ", placeholder(""))),
        [format!("{}個", placeholder("kanji"))]
    );
    assert_eq!(
        surfaces(&d, &format!("{}{{", placeholder(""))),
        [format!("{{{}}}", placeholder(""))]
    );
}

#[test]
fn reasons_for_invalid_numeric_lines() {
    for line in [
        "{}こ\t{}と{}",
        "こ\t{}個",
        "{}こ\t{roman}個",
        "{}こ\t{kanji個",
        "{kanji}こ\t個",
        "{こ\t個",
        "{}\t{}\t五段-カ行",
        "{}*こ\t{}こ",
        "\u{FDD0}\u{FDD1}こ\t個",
        "こ\t\u{FDD0}\u{FDD1}個",
    ] {
        assert_eq!(invalid(line), [InvalidReason::Placeholder], "{line:?}");
    }
}

#[test]
fn a_hide_line_hides_a_numeric_pair_with_its_placeholders() {
    let (d, invalid) = TextDictionary::parse_user_custom("!{}こ\t{kanji}個");
    assert_eq!(invalid, []);
    assert!(d.is_hidden(
        &format!("{}こ", placeholder("")),
        &format!("{}個", placeholder("kanji"))
    ));
    let line = TextDictionary::hide_line(
        &format!("{}{{", placeholder("")),
        &format!("{{{}", placeholder("kanji")),
    );
    assert_eq!(line, "!{}\\{\t\\{{kanji}");
}

#[test]
fn placeholders_are_shown_as_they_are_written() {
    let reading = format!("{}こ{{", placeholder(""));
    assert_eq!(show_placeholders(reading), "{}こ{");
    assert_eq!(show_placeholders(placeholder("kanji")), "{kanji}");
}

#[test]
fn a_numeric_line_reads_back_as_it_was_written() {
    let reading = format!("{}がつ{{{}", placeholder(""), placeholder(""));
    let surface = format!("{}月}}{}", placeholder("kanji"), placeholder("daiji"));
    let line = ItemLine {
        reading: &reading,
        surface: &surface,
        cost: Some(2),
        ..ItemLine::default()
    };
    assert_eq!(line.to_string(), "{}がつ\\{{}\t{kanji}月}{daiji}\t\t2");
    assert_eq!(
        parse(&line.to_string()).words(&reading),
        [entry(&surface, 2)]
    );
}
