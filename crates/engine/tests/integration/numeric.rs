use kanaemi_core::{Converter, Effect};
use kanaemi_engine::{Engine, Selections, Slot, TextDictionary};

use crate::common::{Learn, Lines, dictionary};

const COUNTERS: &str = "\
{}こ\t{kanji}個
{}こ\t{wide-num}個
{}こ\t{}個
";

fn engine(text: &str, user: &str) -> (Engine, Lines) {
    let lines = Lines::default();
    let slots = vec![Slot::UserCustom, Slot::Dictionary(dictionary(text))];
    let user = TextDictionary::parse_user_custom(user).0;
    (Engine::new(slots, user, lines.clone()), lines)
}

fn surfaces(engine: &Engine, reading: &str) -> Vec<String> {
    engine
        .convert(reading, None)
        .into_iter()
        .map(|c| c.surface)
        .collect()
}

#[test]
fn a_number_fills_the_placeholders_of_numeric_items() {
    let (e, _) = engine(COUNTERS, "");
    assert_eq!(surfaces(&e, "１こ"), ["1個", "１個", "一個"]);
    assert_eq!(surfaces(&e, "12こ"), ["12個", "１２個", "十二個"]);
}

#[test]
fn each_number_fills_the_placeholder_of_its_place() {
    let (e, _) = engine("{}がつ{}にち\t{kanji}月{wide-num}日", "");
    assert_eq!(surfaces(&e, "１２がつ２５にち"), ["十二月２５日"]);
}

#[test]
fn a_number_a_notation_cannot_write_leaves_out_that_candidate_only() {
    let (e, _) = engine(COUNTERS, "");
    let zeros = 20;
    assert_eq!(
        surfaces(&e, &format!("1{}こ", "0".repeat(zeros))),
        [
            format!("1{}個", "0".repeat(zeros)),
            format!("１{}個", "０".repeat(zeros))
        ]
    );
}

#[test]
fn a_reading_with_a_number_is_also_looked_up_as_typed() {
    let (e, _) = engine("１ばん\t一番\t\t0\n{}ばん\t{}番\t\t1", "");
    assert_eq!(surfaces(&e, "１ばん"), ["一番", "1番"]);
}

#[test]
fn a_surface_from_a_word_and_a_numeric_item_is_one_candidate() {
    let (e, _) = engine("１ばん\t1番\n{}ばん\t{}番", "");
    assert_eq!(surfaces(&e, "１ばん"), ["1番"]);
}

#[test]
fn okurigana_leaves_numeric_items_out() {
    let (e, _) = engine("{}こ\t{}こ", "");
    assert_eq!(e.convert("１こ", Some("こ")), []);
}

#[test]
fn the_notation_committed_in_a_field_comes_first_for_any_number() {
    let (mut e, _) = engine(COUNTERS, "");
    e.commit("１こ", "一個");
    assert_eq!(surfaces(&e, "２こ")[0], "二個");
}

#[test]
fn a_notation_picked_again_and_again_comes_first_everywhere() {
    let (mut e, _) = engine(COUNTERS, "");
    for (reading, surface) in [("１こ", "１個"), ("２こ", "２個"), ("３こ", "３個")] {
        e.commit(reading, surface);
        e.move_focus();
    }
    assert_eq!(surfaces(&e, "５こ")[0], "５個");
}

#[test]
fn picks_of_numeric_candidates_are_recorded_with_their_placeholders() {
    let (mut e, _) = engine(COUNTERS, "");
    e.commit("１こ", "一個");
    let text = e.take_selections().unwrap().to_text();
    assert!(text.contains("\n{}こ\t{kanji}個\t"), "{text}");
    let mut e = engine(COUNTERS, "").0;
    e.replace_selections(Selections::parse(text.replace("1.000", "3.000")));
    assert_eq!(surfaces(&e, "７こ")[0], "七個");
}

#[test]
fn deleting_a_numeric_candidate_hides_its_item_for_every_number() {
    let (mut e, lines) = engine(COUNTERS, "");
    e.delete("１こ", "一個");
    assert_eq!(*lines.0.borrow(), ["!{}こ\t{kanji}個"]);
    assert_eq!(surfaces(&e, "２こ"), ["2個", "２個"]);
}

#[test]
fn hiding_a_numeric_item_leaves_a_word_of_the_same_surface() {
    let (e, _) = engine("１こ\t一個\t\t1\n{}こ\t{kanji}個\t\t0", "!{}こ\t{kanji}個");
    assert_eq!(surfaces(&e, "１こ"), ["一個"]);
}

#[test]
fn a_candidate_converted_with_okurigana_is_never_taken_for_a_numeric_one() {
    let (mut e, lines) = engine("１わ*る\t1割る\n{}わる\t{}割る", "");
    e.learn(&Effect::Forgotten {
        reading: "１わる".to_owned(),
        okurigana: Some("る".to_owned()),
        surface: "1割る".to_owned(),
    });
    assert_eq!(*lines.0.borrow(), ["!１わる\t1割る"]);
}

#[test]
fn registering_with_a_placeholder_writes_a_numeric_item() {
    let (mut e, lines) = engine("", "");
    e.register("１こ", "{kanji}個");
    assert_eq!(*lines.0.borrow(), ["{}こ\t{kanji}個"]);
    assert_eq!(surfaces(&e, "３こ"), ["三個"]);
}

#[test]
fn a_word_registered_with_a_placeholder_enters_the_field_filled() {
    let (e, _) = engine("", "");
    assert_eq!(e.registered_text("１２こ", None, "{kanji}個"), "十二個");
    assert_eq!(e.registered_text("１こ", None, "1個"), "1個");
    assert_eq!(e.registered_text("こ", None, "{kanji}"), "{kanji}");
    assert_eq!(e.registered_text("１こ", Some("こ"), "{}こ"), "{}こ");
}

#[test]
fn the_commit_of_a_registered_numeric_item_is_recorded_with_its_placeholders() {
    let (mut e, _) = engine("", "");
    e.register("１こ", "{kanji}個");
    e.commit("１こ", "一個");
    let text = e.take_selections().unwrap().to_text();
    assert!(text.contains("\n{}こ\t{kanji}個\t"), "{text}");
}

#[test]
fn registering_without_a_placeholder_writes_the_reading_as_typed() {
    let (mut e, lines) = engine("", "");
    e.register("１こ", "1個");
    e.register("こ", "{kanji}");
    assert_eq!(*lines.0.borrow(), ["１こ\t1個", "こ\t\\{kanji}"]);
}
