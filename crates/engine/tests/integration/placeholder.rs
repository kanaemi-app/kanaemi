use std::cell::Cell;
use std::rc::Rc;

use kanaemi_core::{Converter, Effect};
use kanaemi_engine::{Call, Engine, Functions, Slot, TextDictionary};

use crate::common::{Learn, Lines, dictionary};

/// `count` gives the next number each time it is called; `echo` gives its
/// source and argument.
#[derive(Default)]
struct Test {
    counted: Cell<usize>,
}

impl Functions for Test {
    fn has(&self, name: &str) -> bool {
        matches!(name, "count" | "echo" | "decomposed" | "kanji")
    }

    fn call(&self, call: &Call) -> Option<String> {
        match call.name {
            "count" => {
                self.counted.set(self.counted.get() + 1);
                Some(self.counted.get().to_string())
            }
            "decomposed" => Some("か\u{3099}".to_owned()),
            "echo" => Some(format!(
                "{}{}",
                call.source,
                call.argument.unwrap_or_default()
            )),
            _ => None,
        }
    }
}

fn engine(text: &str) -> (Engine, Lines) {
    let lines = Lines::default();
    let slots = vec![Slot::UserCustom, Slot::Dictionary(dictionary(text))];
    let user = TextDictionary::parse_user_custom("").0;
    let mut engine = Engine::new(slots, user, lines.clone());
    engine.set_functions(Some(Rc::new(Test::default())));
    (engine, lines)
}

/// An engine opened again on `old`'s functions, as a reload opens it.
fn opened_again(old: Engine, functions: &Rc<Test>, text: &str) -> Engine {
    let (mut new, _) = engine(text);
    new.set_functions(Some(functions.clone() as Rc<dyn Functions>));
    new.take_over(old);
    new
}

fn surfaces(engine: &Engine, reading: &str) -> Vec<String> {
    engine
        .convert(reading, None)
        .into_iter()
        .map(|c| c.surface)
        .collect()
}

#[test]
fn a_word_s_placeholder_hands_the_reading_to_its_function() {
    let (e, _) = engine("よみ\t「{-:echo !}」");
    assert_eq!(surfaces(&e, "よみ"), ["「よみ!」"]);
}

#[test]
fn a_function_is_called_each_time_the_word_is_converted() {
    let (e, _) = engine("かず\t{-:count}");
    assert_eq!(surfaces(&e, "かず"), ["1"]);
    assert_eq!(surfaces(&e, "かず"), ["2"]);
}

#[test]
fn a_user_function_goes_in_place_of_the_builtin_of_its_name() {
    let (e, _) = engine("{}こ\t{kanji}個\n{}こ\t{echo}個");
    assert_eq!(surfaces(&e, "3こ"), ["3個"]);
}

#[test]
fn a_placeholder_whose_function_gives_nothing_leaves_out_its_word_only() {
    let (e, _) = engine("よみ\t{-:missing}\nよみ\t読み");
    assert_eq!(surfaces(&e, "よみ"), ["読み"]);
}

#[test]
fn okurigana_leaves_words_with_placeholders_out() {
    let (e, _) = engine("よむ\t{-:echo}む");
    assert_eq!(e.convert("よむ", Some("む")), []);
}

#[test]
fn a_commit_is_recorded_by_the_item_even_when_its_value_changes() {
    let (mut e, _) = engine("かず\t{-:count}");
    assert_eq!(surfaces(&e, "かず"), ["1"]);
    e.commit("かず", "1");
    let text = e.take_selections().unwrap().to_text();
    assert!(text.contains("\nかず\t{-:count}\t"), "{text}");
}

#[test]
fn deleting_a_plain_word_leaves_a_filled_item_that_would_now_give_it() {
    let (mut e, lines) = engine("かず\t{-:count}\t\t0\nかず\t2\t\t1");
    assert_eq!(surfaces(&e, "かず"), ["1", "2"]);
    e.delete("かず", "2");
    assert_eq!(*lines.0.borrow(), ["!かず\t2"]);
}

#[test]
fn the_commit_of_a_word_registered_with_a_changing_value_is_recorded_by_its_item() {
    let (mut e, _) = engine("");
    assert_eq!(surfaces(&e, "かず"), Vec::<String>::new());
    let filled = e.registered_text("かず", None, "{-:count}");
    e.register("かず", "{-:count}");
    e.commit("かず", &filled);
    let text = e.take_selections().unwrap().to_text();
    assert!(text.contains("\nかず\t{-:count}\t"), "{text}");
}

#[test]
fn trying_a_registration_runs_no_function_of_the_user() {
    let (e, _) = engine("");
    let tried = e.without_functions(|| e.registered_text("かず", None, "{-:count}"));
    assert_eq!(tried, "{-:count}");
    assert_eq!(e.registered_text("かず", None, "{-:count}"), "1");
}

#[test]
fn the_commit_of_a_plain_word_registered_after_converting_is_recorded_as_typed() {
    let (mut e, _) = engine("よみ\t{-:echo}");
    assert_eq!(surfaces(&e, "よみ"), ["よみ"]);
    let text = e.registered_text("よみ", None, "よみ");
    e.register("よみ", "よみ");
    e.commit("よみ", &text);
    let text = e.take_selections().unwrap().to_text();
    assert!(text.contains("\nよみ\tよみ\t"), "{text}");
}

#[test]
fn withdrawing_a_commit_after_converting_again_withdraws_its_item() {
    let (mut e, _) = engine("かず\t{-:count}");
    assert_eq!(surfaces(&e, "かず"), ["1"]);
    e.commit("かず", "1");
    assert_eq!(surfaces(&e, "かず"), ["2"]);
    e.withdraw("かず", "1");
    assert_eq!(
        e.take_selections()
            .unwrap()
            .to_text()
            .matches("{-:count}")
            .count(),
        0
    );
}

#[test]
fn withdrawing_a_commit_made_before_opening_the_engine_again_withdraws_its_item() {
    let functions = Rc::new(Test::default());
    let (mut old, _) = engine("かず\t{-:count}");
    old.set_functions(Some(functions.clone() as Rc<dyn Functions>));
    assert_eq!(surfaces(&old, "かず"), ["1"]);
    old.commit("かず", "1");
    let mut new = opened_again(old, &functions, "かず\t{-:count}");
    new.withdraw("かず", "1");
    let text = new.take_selections().unwrap().to_text();
    assert_eq!(text.matches("{-:count}").count(), 0, "{text}");
}

#[test]
fn a_commit_converted_before_opening_the_engine_again_is_recorded_by_its_item() {
    let functions = Rc::new(Test::default());
    let (mut old, _) = engine("かず\t{-:count}");
    old.set_functions(Some(functions.clone() as Rc<dyn Functions>));
    assert_eq!(surfaces(&old, "かず"), ["1"]);
    let mut new = opened_again(old, &functions, "かず\t{-:count}");
    new.commit("かず", "1");
    let text = new.take_selections().unwrap().to_text();
    assert!(text.contains("\nかず\t{-:count}\t"), "{text}");
}

#[test]
fn a_hidden_item_runs_no_function() {
    let lines = Lines::default();
    let slots = vec![
        Slot::UserCustom,
        Slot::Dictionary(dictionary("かず\t{-:count}\nかず2\t{-:count}")),
    ];
    let user = TextDictionary::parse_user_custom("!かず\t{-:count}").0;
    let mut e = Engine::new(slots, user, lines);
    e.set_functions(Some(Rc::new(Test::default())));
    assert_eq!(surfaces(&e, "かず"), Vec::<String>::new());
    assert_eq!(surfaces(&e, "かず2"), ["1"]);
}

#[test]
fn deleting_a_filled_candidate_hides_its_item() {
    let (mut e, lines) = engine("よみ\t{-:echo}");
    assert_eq!(surfaces(&e, "よみ"), ["よみ"]);
    e.learn(&Effect::Forgotten {
        reading: "よみ".to_owned(),
        okurigana: None,
        surface: "よみ".to_owned(),
    });
    assert_eq!(*lines.0.borrow(), ["!よみ\t{-:echo}"]);
    assert_eq!(surfaces(&e, "よみ"), Vec::<String>::new());
}

#[test]
fn what_a_function_gives_is_normalized_so_deleting_it_hides_its_item() {
    let (mut e, lines) = engine("が\t{-:decomposed}");
    assert_eq!(surfaces(&e, "が"), ["が"]);
    e.delete("が", "が");
    assert_eq!(*lines.0.borrow(), ["!が\t{-:decomposed}"]);
}

#[test]
fn registering_a_placeholder_that_takes_the_reading_writes_it_as_typed() {
    let (mut e, lines) = engine("");
    e.register("よみ", "<{-:echo}>");
    assert_eq!(*lines.0.borrow(), ["よみ\t<{-:echo}>"]);
    assert_eq!(e.registered_text("よみ", None, "<{-:echo}>"), "<よみ>");
    assert_eq!(surfaces(&e, "よみ"), ["<よみ>"]);
}

#[test]
fn registering_a_placeholder_the_reading_cannot_fill_writes_it_literally() {
    let (mut e, lines) = engine("");
    e.register("よみ", "{echo}");
    assert_eq!(*lines.0.borrow(), ["よみ\t\\{echo}"]);
}
