use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use kanaemi_core::{Converter, Effect};
use kanaemi_engine::{Engine, LineSink, Slot, TextDictionary};

use crate::common::{Learn, Lines, dictionary};

struct Broken;

impl LineSink for Broken {
    fn append(&mut self, _: &str) -> io::Result<()> {
        Err(io::Error::other("disk full"))
    }
}

fn text(text: &str) -> Slot {
    Slot::Dictionary(dictionary(text))
}

fn engine(slots: Vec<Slot>, user: &str) -> (Engine, Lines) {
    let lines = Lines::default();
    let user = TextDictionary::parse_user_custom(user).0;
    (Engine::new(slots, user, lines.clone()), lines)
}

fn surfaces(engine: &Engine, reading: &str, okurigana: Option<&str>) -> Vec<String> {
    engine
        .convert(reading, okurigana)
        .into_iter()
        .map(|c| c.surface)
        .collect()
}

#[test]
fn candidates_come_in_cost_order() {
    let (e, _) = engine(vec![Slot::UserCustom, text("かく\t角\nかく\t書く")], "");
    assert_eq!(surfaces(&e, "かく", None), ["書く", "角"]);
}

const TACHA: &str = "たちゃ\t立ちゃ\t\t1\nたちゃ\t達ゃ\t\t3\nたち*ゃ\t達ゃ\t\t3";

#[test]
fn okurigana_words_come_before_words_of_the_whole_reading() {
    let (e, _) = engine(vec![Slot::UserCustom, text(TACHA)], "");
    assert_eq!(surfaces(&e, "たちゃ", Some("ゃ")), ["達ゃ", "立ちゃ"]);
    assert_eq!(surfaces(&e, "たちゃ", None), ["立ちゃ", "達ゃ"]);
}

#[test]
fn conjugated_words_split_at_the_mark_keep_their_order_among_okurigana_words() {
    let (e, _) = engine(
        vec![
            Slot::UserCustom,
            text("か\t書\t五段-カ行"),
            text("かく\t画く\t\t0\nか*く\t掻く\t\t1"),
        ],
        "",
    );
    assert_eq!(surfaces(&e, "かく", Some("く")), ["書く", "掻く", "画く"]);
}

#[test]
fn a_commit_of_the_reading_keeps_its_order_over_okurigana_words() {
    let (mut e, _) = engine(vec![Slot::UserCustom, text(TACHA)], "");
    e.commit("たちゃ", "立ちゃ");
    assert_eq!(surfaces(&e, "たちゃ", Some("ゃ")), ["立ちゃ", "達ゃ"]);
}

#[test]
fn a_reading_without_entries_gives_nothing() {
    let (e, _) = engine(vec![Slot::UserCustom, text("かく\t角")], "");
    assert_eq!(surfaces(&e, "ぬぬ", None), Vec::<String>::new());
}

#[test]
fn earlier_dictionaries_come_first() {
    let (e, _) = engine(
        vec![
            text("きしゃ\t汽車\nきしゃ\t帰社"),
            Slot::UserCustom,
            text("きしゃ\t記者"),
        ],
        "きしゃ\t貴社",
    );
    assert_eq!(
        surfaces(&e, "きしゃ", None),
        ["帰社", "汽車", "貴社", "記者"]
    );
}

#[test]
fn the_user_custom_dictionary_is_first_unless_placed() {
    let (e, _) = engine(vec![text("きしゃ\t記者")], "きしゃ\t貴社");
    assert_eq!(surfaces(&e, "きしゃ", None), ["貴社", "記者"]);
}

#[test]
fn a_surface_in_several_dictionaries_is_kept_where_it_ranks_highest() {
    let (e, _) = engine(
        vec![
            Slot::UserCustom,
            text("きしゃ\t記者\nきしゃ\t汽車"),
            text("きしゃ\t記者"),
        ],
        "",
    );
    assert_eq!(surfaces(&e, "きしゃ", None), ["汽車", "記者"]);
}

#[test]
fn okurigana_written_with_its_row_picks_entries_by_any_kana_of_the_row() {
    let (e, _) = engine(
        vec![
            Slot::UserCustom,
            text("か*k\t書\nか*k\t欠\nか*t\t勝\nかき\t柿"),
        ],
        "",
    );
    assert_eq!(surfaces(&e, "かき", Some("き")), ["欠き", "書き"]);
    assert_eq!(surfaces(&e, "かく", Some("く")), ["欠く", "書く"]);
    assert_eq!(surfaces(&e, "かっ", Some("っ")), ["勝っ"]);
}

#[test]
fn okurigana_written_with_its_kana_picks_entries_by_that_kana_only() {
    let (e, _) = engine(
        vec![
            Slot::UserCustom,
            text(
                "き*た\t着た\nき*て\t来て\nき*て\t著て\nき*と\t祈と\nき*て\t衣て\n\
                 き*た\t黄た\nき*っ\t切っ\nき*ら\t切ら",
            ),
        ],
        "",
    );
    assert_eq!(surfaces(&e, "きった", Some("った")), ["切った"]);
    assert_eq!(surfaces(&e, "きた", Some("た")), ["黄た", "着た"]);
    assert_eq!(surfaces(&e, "きち", Some("ち")), Vec::<String>::new());
}

#[test]
fn okurigana_picks_entries_by_their_kana_and_by_their_row_alike() {
    let (e, _) = engine(
        vec![Slot::UserCustom, text("か*く\t書く\t\t2\nか*k\t欠\t\t1")],
        "",
    );
    assert_eq!(surfaces(&e, "かく", Some("く")), ["欠く", "書く"]);
    assert_eq!(surfaces(&e, "かけ", Some("け")), ["欠け"]);
}

#[test]
fn okurigana_grown_past_its_first_kana_finds_words_by_that_kana() {
    let (e, _) = engine(
        vec![
            Slot::UserCustom,
            text("か*っ\t勝っ\nか\t書\t五段-カ行\nき\t切\t五段-ラ行"),
        ],
        "",
    );
    assert_eq!(surfaces(&e, "かった", Some("った")), ["勝った"]);
    assert_eq!(surfaces(&e, "きった", Some("った")), ["切った"]);
}

#[test]
fn okurigana_keeps_only_surfaces_ending_in_it() {
    let (e, _) = engine(vec![Slot::UserCustom, text("かく\t角\nかく\t書く")], "");
    assert_eq!(surfaces(&e, "かく", Some("く")), ["書く"]);
}

#[test]
fn a_word_and_an_okurigana_entry_for_one_surface_merge() {
    let (e, _) = engine(vec![Slot::UserCustom, text("か*く\t書く\nかく\t書く")], "");
    assert_eq!(surfaces(&e, "かく", Some("く")), ["書く"]);
}

#[test]
fn hidden_pairs_are_left_out_of_every_dictionary() {
    let (e, _) = engine(
        vec![Slot::UserCustom, text("きしゃ\t記者\nきしゃ\t汽車")],
        "!きしゃ\t汽車",
    );
    assert_eq!(surfaces(&e, "きしゃ", None), ["記者"]);
}

#[test]
fn the_last_commit_for_the_reading_comes_first_then_recent_commits() {
    let (mut e, _) = engine(
        vec![
            Slot::UserCustom,
            text("かんじ\t幹事\nかんじ\t感じ\nかんじ\t漢字"),
        ],
        "",
    );
    e.commit("かんじ", "幹事");
    e.commit("かんじ", "感じ");
    assert_eq!(surfaces(&e, "かんじ", None), ["感じ", "幹事", "漢字"]);
}

#[test]
fn a_surface_committed_under_another_reading_counts_as_recent() {
    let (mut e, _) = engine(
        vec![Slot::UserCustom, text("かんじ\t幹事\nかんじ\t漢字")],
        "",
    );
    e.commit("かんじる", "幹事");
    assert_eq!(surfaces(&e, "かんじ", None), ["幹事", "漢字"]);
}

#[test]
fn only_the_last_300_commits_count() {
    let (mut e, _) = engine(
        vec![Slot::UserCustom, text("かんじ\t幹事\nかんじ\t漢字")],
        "",
    );
    e.commit("かんじる", "幹事");
    for _ in 0..300 {
        e.commit("ほか", "他");
    }
    assert_eq!(surfaces(&e, "かんじ", None), ["漢字", "幹事"]);
}

#[test]
fn reset_forgets_the_commits() {
    let (mut e, _) = engine(
        vec![Slot::UserCustom, text("かんじ\t幹事\nかんじ\t漢字")],
        "",
    );
    e.commit("かんじ", "幹事");
    e.move_focus();
    assert_eq!(surfaces(&e, "かんじ", None), ["漢字", "幹事"]);
}

#[test]
fn registering_writes_a_line_and_offers_the_word() {
    let (mut e, lines) = engine(vec![Slot::UserCustom], "");
    e.register("ぬ*ぬ", "xぬ");
    e.register("きしゃ", "記者");
    assert_eq!(*lines.0.borrow(), ["ぬ*ぬ\txぬ", "きしゃ\t記者"]);
    assert_eq!(surfaces(&e, "ぬぬ", Some("ぬ")), ["xぬ"]);
    assert_eq!(surfaces(&e, "きしゃ", None), ["記者"]);
}

#[test]
fn an_okurigana_grown_past_its_first_chunk_is_written_under_that_chunk_if_it_can_be() {
    let (mut e, lines) = engine(vec![Slot::UserCustom], "");
    let grown = |head: &str, okurigana: &str, surface: &str| Effect::Registered {
        reading: "か".to_owned(),
        okurigana: Some(okurigana.to_owned()),
        okurigana_head: Some(head.to_owned()),
        surface: surface.to_owned(),
    };
    e.learn(&grown("っ", "った", "勝った"));
    e.learn(&grown("ー", "ーった", "xーった"));
    e.learn(&grown("きゃ", "きゃった", "xきゃった"));
    assert_eq!(
        *lines.0.borrow(),
        ["か*っ\t勝っ", "かーった\txーった", "かきゃった\txきゃった"]
    );
    assert_eq!(surfaces(&e, "かって", Some("って")), ["勝って"]);
    assert_eq!(surfaces(&e, "かーった", Some("ーった")), ["xーった"]);
}

#[test]
fn deleting_writes_a_hide_line_and_stops_offering_the_pair() {
    let (mut e, lines) = engine(
        vec![Slot::UserCustom, text("きしゃ\t記者\nきしゃ\t汽車")],
        "",
    );
    e.delete("きしゃ", "汽車");
    assert_eq!(*lines.0.borrow(), ["!きしゃ\t汽車"]);
    assert_eq!(surfaces(&e, "きしゃ", None), ["記者"]);
}

#[test]
fn registering_a_deleted_pair_offers_it_again() {
    let (mut e, _) = engine(vec![Slot::UserCustom, text("きしゃ\t汽車")], "");
    e.delete("きしゃ", "汽車");
    e.register("きしゃ", "汽車");
    assert_eq!(surfaces(&e, "きしゃ", None), ["汽車"]);
}

#[test]
fn a_failed_write_still_applies_in_memory_and_is_reported() {
    let user = TextDictionary::parse_user_custom("").0;
    let mut e = Engine::new(vec![Slot::UserCustom], user, Broken);
    e.register("きしゃ", "記者");
    assert_eq!(surfaces(&e, "きしゃ", None), ["記者"]);
    let errors = e.take_write_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].to_string(), "disk full");
    assert!(e.take_write_errors().is_empty());
}

#[test]
fn okurigana_rows_follow_the_okuri_index() {
    let (e, _) = engine(
        vec![
            Slot::UserCustom,
            text("おぼ*え\t覚え\nかん*じ\t感じ\nおも*ふ\t思ふ"),
        ],
        "",
    );
    assert_eq!(surfaces(&e, "おぼい", Some("い")), Vec::<String>::new());
    assert_eq!(surfaces(&e, "おぼえ", Some("え")), ["覚え"]);
    assert_eq!(surfaces(&e, "かんざ", Some("ざ")), Vec::<String>::new());
    assert_eq!(surfaces(&e, "かんじ", Some("じ")), ["感じ"]);
    assert_eq!(surfaces(&e, "おもは", Some("は")), Vec::<String>::new());
}

#[test]
fn okurigana_of_several_kana_is_registered_without_the_mark() {
    let (mut e, lines) = engine(vec![Slot::UserCustom], "");
    e.register("か*きゃ", "課きゃ");
    assert_eq!(*lines.0.borrow(), ["かきゃ\t課きゃ"]);
    assert_eq!(surfaces(&e, "かきゃ", None), ["課きゃ"]);
    assert!(e.take_write_errors().is_empty());
}

#[test]
fn okurigana_outside_the_okuri_table_is_registered_without_the_mark() {
    let (mut e, lines) = engine(vec![Slot::UserCustom], "");
    e.register("か*ー", "課ー");
    assert_eq!(*lines.0.borrow(), ["かー\t課ー"]);
    assert_eq!(surfaces(&e, "かー", None), ["課ー"]);
    assert!(e.take_write_errors().is_empty());
}

#[test]
fn a_small_kana_okurigana_is_registered_with_the_mark() {
    let (mut e, lines) = engine(vec![Slot::UserCustom], "");
    e.register("たち*ゃ", "達ゃ");
    assert_eq!(*lines.0.borrow(), ["たち*ゃ\t達ゃ"]);
    assert_eq!(surfaces(&e, "たちゃ", Some("ゃ")), ["達ゃ"]);
    assert!(e.take_write_errors().is_empty());
}

#[test]
fn a_reading_with_a_literal_asterisk_can_be_deleted() {
    let (mut e, lines) = engine(vec![Slot::UserCustom, text("あ\\*\t亜")], "");
    e.delete("あ*", "亜");
    assert_eq!(*lines.0.borrow(), ["!あ\\*\t亜"]);
    assert_eq!(surfaces(&e, "あ*", None), Vec::<String>::new());
}

#[test]
fn registering_a_deleted_okurigana_word_offers_it_again() {
    let (mut e, _) = engine(vec![Slot::UserCustom, text("か*く\t書く")], "");
    e.delete("かく", "書く");
    e.register("か*く", "書く");
    assert_eq!(surfaces(&e, "かく", Some("く")), ["書く"]);
}

#[test]
fn readings_are_looked_up_in_nfc() {
    let (mut e, _) = engine(vec![Slot::UserCustom, text("が\t蛾\nが\t我")], "");
    e.commit("か\u{3099}", "蛾");
    assert_eq!(surfaces(&e, "か\u{3099}", None), ["蛾", "我"]);
}

#[test]
fn registered_and_committed_text_is_compared_in_nfc() {
    let (mut e, _) = engine(vec![text("が\t我\nが\tガ")], "");
    e.register("およ*か\u{3099}", "泳か\u{3099}");
    assert_eq!(surfaces(&e, "およが", Some("が")), ["泳が"]);
    e.commit("ほか", "カ\u{3099}");
    assert_eq!(surfaces(&e, "が", None), ["ガ", "我"]);
}

#[test]
fn conjugated_forms_are_built_from_stems_and_cost_more() {
    let (e, _) = engine(
        vec![
            Slot::UserCustom,
            text("か\t書\t五段-カ行\nかいた\t下位た\nかく\t角"),
        ],
        "",
    );
    assert_eq!(surfaces(&e, "かいた", None), ["下位た", "書いた"]);
    assert_eq!(surfaces(&e, "かく", None), ["角", "書く"]);
    assert_eq!(surfaces(&e, "かかなかった", None), Vec::<String>::new());
}

#[test]
fn a_verb_takes_ている_and_its_forms_within_four_kana() {
    let (e, _) = engine(
        vec![
            Slot::UserCustom,
            text("たべ\t食べ\t下一段-バ行\nか\t書\t五段-カ行\nおよ\t泳\t五段-ガ行"),
        ],
        "",
    );
    for (reading, surface) in [
        ("たべている", "食べている"),
        ("たべていた", "食べていた"),
        ("たべていない", "食べていない"),
        ("かいている", "書いている"),
        ("およいでいた", "泳いでいた"),
    ] {
        assert_eq!(surfaces(&e, reading, None), [surface], "{reading}");
    }
    assert_eq!(surfaces(&e, "たべていました", None), Vec::<String>::new());
}

#[test]
fn common_auxiliaries_attach_to_verbs_and_adjectives() {
    let (e, _) = engine(
        vec![
            Slot::UserCustom,
            text("たべ\t食べ\t下一段-バ行\nたか\t高\t形容詞"),
        ],
        "",
    );
    for (reading, surface) in [
        ("たべそう", "食べそう"),
        ("たかそう", "高そう"),
        ("たべるそうだ", "食べるそうだ"),
        ("たべるらしい", "食べるらしい"),
        ("たべるみたい", "食べるみたい"),
        ("たかくても", "高くても"),
    ] {
        assert_eq!(surfaces(&e, reading, None), [surface], "{reading}");
    }
}

#[test]
fn with_okurigana_the_stem_ends_where_the_okurigana_starts() {
    let (e, _) = engine(
        vec![
            Slot::UserCustom,
            text("か\t書\t五段-カ行\nかい\t買\t五段-ワア行"),
        ],
        "",
    );
    assert_eq!(surfaces(&e, "かい", Some("い")), ["書い"]);
}

#[test]
fn with_okurigana_an_ichidan_stem_may_include_it() {
    let (e, _) = engine(vec![Slot::UserCustom, text("たべ\t食べ\t下一段-バ行")], "");
    assert_eq!(surfaces(&e, "たべ", Some("べ")), ["食べ"]);
}

/// Fails until `heal` is called, then writes like [`Lines`].
#[derive(Clone, Default)]
struct Flaky {
    lines: Lines,
    healed: Rc<RefCell<bool>>,
}

impl LineSink for Flaky {
    fn append(&mut self, line: &str) -> io::Result<()> {
        if !*self.healed.borrow() {
            return Err(io::Error::other("disk full"));
        }
        self.lines.append(line)
    }
}

#[test]
fn a_failed_line_is_written_before_the_next_one() {
    let sink = Flaky::default();
    let user = TextDictionary::parse_user_custom("").0;
    let mut e = Engine::new(vec![Slot::UserCustom], user, sink.clone());
    e.register("きしゃ", "記者");
    *sink.healed.borrow_mut() = true;
    e.delete("きしゃ", "汽車");
    assert_eq!(*sink.lines.0.borrow(), ["きしゃ\t記者", "!きしゃ\t汽車"]);
    assert_eq!(e.take_write_errors().len(), 1);
}

#[test]
fn reading_the_user_custom_dictionary_again_keeps_unwritten_lines() {
    let user = TextDictionary::parse_user_custom("").0;
    let mut e = Engine::new(vec![Slot::UserCustom], user, Broken);
    e.register("きしゃ", "記者");
    e.replace_user(TextDictionary::parse_user_custom("きしゃ\t汽車").0);
    assert_eq!(surfaces(&e, "きしゃ", None), ["記者", "汽車"]);
}

#[test]
fn an_engine_opened_again_takes_over_the_history_and_unwritten_lines() {
    let user = TextDictionary::parse_user_custom("").0;
    let mut old = Engine::new(vec![Slot::UserCustom], user, Broken);
    old.register("きしゃ", "記者");
    old.commit("かんじ", "幹事");
    let lines = Lines::default();
    let mut new = Engine::new(
        vec![Slot::UserCustom, text("かんじ\t幹事\nかんじ\t漢字")],
        TextDictionary::parse_user_custom("").0,
        lines.clone(),
    );
    new.take_over(old);
    assert_eq!(surfaces(&new, "きしゃ", None), ["記者"]);
    assert_eq!(surfaces(&new, "かんじ", None), ["幹事", "漢字"]);
    new.register("かく", "書く");
    assert_eq!(*lines.0.borrow(), ["きしゃ\t記者", "かく\t書く"]);
}

#[test]
fn costs_from_the_dictionary_rank_candidates_of_different_readings() {
    let (e, _) = engine(
        vec![
            Slot::UserCustom,
            text("おも*t\tお持\t\t900\nおも*っ\t思っ\t\t200"),
        ],
        "",
    );
    assert_eq!(surfaces(&e, "おもっ", Some("っ")), ["思っ", "お持っ"]);
}

#[test]
fn without_costs_each_reading_starts_from_zero_and_ties_go_by_surface() {
    let (e, _) = engine(
        vec![Slot::UserCustom, text("おも*t\tお持\nおも*っ\t思っ")],
        "",
    );
    assert_eq!(surfaces(&e, "おもっ", Some("っ")), ["お持っ", "思っ"]);
}

#[test]
fn a_write_error_does_not_carry_the_words() {
    let (mut e, _) = engine(vec![Slot::UserCustom], "");
    e.register("ひみつ", "");
    let errors = e.take_write_errors();
    assert_eq!(errors.len(), 1);
    assert!(!errors[0].to_string().contains("ひみつ"), "{}", errors[0]);
}

#[test]
fn a_shared_dictionary_and_a_boxed_sink_make_an_engine() {
    let shared = std::sync::Arc::new(TextDictionary::parse("きしゃ\t記者\n").0);
    let lines = Lines::default();
    let sink: Box<dyn LineSink> = Box::new(lines.clone());
    let mut e = Engine::new(
        [Slot::Dictionary(Box::new(shared.clone()))],
        TextDictionary::parse_user_custom("").0,
        sink,
    );
    assert_eq!(surfaces(&e, "きしゃ", None), ["記者"]);
    e.register("きしゃ", "汽車");
    assert_eq!(*lines.0.borrow(), ["きしゃ\t汽車"]);
}

#[test]
fn the_user_dictionary_in_memory_carries_registrations_into_another_engine() {
    let (mut e, _) = engine(vec![], "");
    e.register("きしゃ", "汽車");
    let (mut other, _) = engine(vec![], "");
    other.replace_user(e.user_dictionary().clone());
    assert_eq!(surfaces(&other, "きしゃ", None), ["汽車"]);
}
