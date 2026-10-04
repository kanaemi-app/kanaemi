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
