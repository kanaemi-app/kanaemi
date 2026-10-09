use kanaemi_core::Converter;
use kanaemi_engine::{Engine, Selections, Slot, TextDictionary};

use crate::common::{Discard, Learn, dictionary};

fn engine(selections: Selections) -> Engine {
    let mut e = Engine::new(
        vec![
            Slot::UserCustom,
            Slot::Dictionary(dictionary(
                "きしゃ\t記者\t\t10\nきしゃ\t汽車\t\t20\nきしゃ\t貴社\t\t30\n",
            )),
        ],
        TextDictionary::parse_user_custom("").0,
        Discard,
    );
    e.replace_selections(selections);
    e
}

fn first(e: &Engine) -> String {
    e.convert("きしゃ", None).remove(0).surface
}

/// Picks `surface` in a field of its own each time, as across applications.
fn pick(e: &mut Engine, surface: &str, times: usize) {
    for _ in 0..times {
        e.commit("きしゃ", surface);
        e.move_focus();
    }
}

#[test]
fn one_pick_in_another_field_changes_nothing() {
    let mut e = engine(Selections::default());
    pick(&mut e, "貴社", 1);
    assert_eq!(first(&e), "記者");
}

#[test]
fn a_surface_picked_again_and_again_comes_first_everywhere() {
    let mut e = engine(Selections::default());
    pick(&mut e, "貴社", 3);
    assert_eq!(first(&e), "貴社");
}

#[test]
fn the_field_s_last_pick_still_comes_before_a_favorite() {
    let mut e = engine(Selections::default());
    pick(&mut e, "貴社", 3);
    e.commit("きしゃ", "汽車");
    assert_eq!(first(&e), "汽車");
}

#[test]
fn old_picks_fade() {
    let mut e = engine(Selections::default());
    pick(&mut e, "貴社", 3);
    for i in 0..3000 {
        e.commit(&format!("ほか{i}"), "他");
    }
    assert_eq!(first(&e), "記者");
}

#[test]
fn the_engine_hands_the_record_over_only_after_a_change() {
    let mut e = engine(Selections::default());
    assert_eq!(e.take_selections(), None);
    e.commit("きしゃ", "貴社");
    let saved = e.take_selections().unwrap();
    assert!(saved.to_text().contains("\nきしゃ\t貴社\t"));
    assert_eq!(e.take_selections(), None);
}

#[test]
fn an_engine_opened_again_keeps_the_picks_not_yet_saved() {
    let mut old = engine(Selections::default());
    pick(&mut old, "貴社", 3);
    let mut new = engine(Selections::default());
    new.take_over(old);
    assert_eq!(first(&new), "貴社");
}

#[test]
fn picks_not_yet_written_are_made_again_on_a_record_written_elsewhere() {
    let mut e = engine(Selections::default());
    pick(&mut e, "貴社", 1);
    e.take_selections();
    e.merge_selections(Selections::parse("きしゃ\t貴社\t2.000\n"));
    assert_eq!(first(&e), "貴社");
    assert!(e.take_selections().is_some(), "to be written");
}

#[test]
fn picks_written_are_not_made_again_on_a_record_read_later() {
    let mut e = engine(Selections::default());
    pick(&mut e, "貴社", 2);
    let written = e.take_selections().unwrap();
    e.selections_written(&written);
    e.merge_selections(Selections::parse(written.to_text()));
    assert_eq!(first(&e), "記者");
    assert_eq!(e.take_selections(), None);
}

#[test]
fn picks_made_after_the_record_written_was_taken_are_made_again() {
    let mut e = engine(Selections::default());
    pick(&mut e, "貴社", 2);
    let written = e.take_selections().unwrap();
    pick(&mut e, "記者", 1);
    e.selections_written(&written);
    e.merge_selections(Selections::parse(written.to_text()));
    let merged = e.take_selections().expect("to be written");
    assert!(merged.to_text().contains("\nきしゃ\t記者\t"));
}

#[test]
fn a_candidate_committed_for_an_empty_reading_is_not_recorded() {
    let mut e = engine(Selections::default());
    e.commit("", "ｐｄｆ");
    e.withdraw("", "ｐｄｆ");
    assert_eq!(e.take_selections(), None);
}
