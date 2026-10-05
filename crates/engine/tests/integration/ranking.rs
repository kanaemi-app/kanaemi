use kanaemi_core::Converter;
use kanaemi_engine::{Engine, Slot, TextDictionary};

use crate::common::{Discard, Learn, dictionary, model};

fn engine(text: &str) -> Engine {
    Engine::new(
        [Slot::UserCustom, Slot::Dictionary(dictionary(text))],
        TextDictionary::parse_user_custom("").0,
        Discard,
    )
}

fn surfaces(engine: &Engine, reading: &str) -> Vec<String> {
    engine
        .convert(reading, None)
        .into_iter()
        .map(|c| c.surface)
        .collect()
}

const KISHA: &str = "きしゃ\t記者\t\t10\nきしゃ\t汽車\t\t20\nきしゃ\t貴社\t\t30\n";

#[test]
fn a_model_ranks_every_candidate_by_its_score() {
    let mut e = engine(KISHA);
    e.set_model(Some(model(
        16,
        &[("s\u{1f}貴社", 2.0), ("s\u{1f}汽車", 1.0)],
    )));
    assert_eq!(surfaces(&e, "きしゃ"), ["貴社", "汽車", "記者"]);
}

#[test]
fn equal_scores_fall_back_to_dictionary_order_and_cost() {
    let mut e = engine(KISHA);
    e.set_model(Some(model(16, &[])));
    assert_eq!(surfaces(&e, "きしゃ"), ["記者", "汽車", "貴社"]);
}

#[test]
fn with_a_model_the_history_counts_only_through_its_weights() {
    let mut e = engine(KISHA);
    e.set_model(Some(model(16, &[])));
    e.commit("きしゃ", "貴社");
    assert_eq!(surfaces(&e, "きしゃ"), ["記者", "汽車", "貴社"]);
    e.set_model(Some(model(16, &[("hl\u{1f}1", 5.0)])));
    assert_eq!(surfaces(&e, "きしゃ")[0], "貴社");
}

#[test]
fn with_a_model_a_number_committed_counts_for_every_number_of_its_item() {
    let mut e = engine("{}こ\t{kanji}個\n{}こ\t{}個\n");
    e.set_model(Some(model(16, &[("hl\u{1f}1", 5.0)])));
    assert_eq!(surfaces(&e, "5こ")[0], "5個");
    e.commit("3こ", "三個");
    assert_eq!(surfaces(&e, "5こ")[0], "五個");
}

#[test]
fn the_committed_text_is_the_context_until_the_focus_moves() {
    let mut e = engine(KISHA);
    e.set_model(Some(model(16, &[("a\u{1f}の\u{1f}汽車", 5.0)])));
    e.type_text("鉄道の");
    assert_eq!(surfaces(&e, "きしゃ")[0], "汽車");
    e.move_focus();
    assert_eq!(surfaces(&e, "きしゃ")[0], "記者");
}

#[test]
fn an_engine_opened_again_without_a_model_ranks_by_the_rules() {
    let mut old = engine(KISHA);
    old.set_model(Some(model(16, &[("s\u{1f}貴社", 2.0)])));
    let mut new = engine(KISHA);
    new.take_over(old);
    assert_eq!(surfaces(&new, "きしゃ"), ["記者", "汽車", "貴社"]);
}

#[test]
fn engines_rank_with_one_model_they_share() {
    let shared = model(16, &[("s\u{1f}貴社", 2.0)]);
    let mut first = engine(KISHA);
    let mut second = engine(KISHA);
    first.set_model(Some(shared.clone()));
    second.set_model(Some(shared));
    assert_eq!(surfaces(&first, "きしゃ")[0], "貴社");
    assert_eq!(surfaces(&second, "きしゃ")[0], "貴社");
}
