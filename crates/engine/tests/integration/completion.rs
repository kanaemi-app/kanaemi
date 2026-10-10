use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use kanaemi_config::default_romaji_table;
use kanaemi_core::{
    Candidate, Config, Converter, Core, Event, Key, KeyEvent, KeyKind, Modifiers, Output,
};
use kanaemi_engine::{Dictionary, Engine, Selections, Slot, TextDictionary, convert_text};

use crate::common::{Discard, Learn, Notations, dictionary, model, temp_path};

/// Words of readings going on from かく, with costs, beside a word of
/// かく itself, a conjugating stem, an okurigana word and a numeric item.
const WORDS: &str = "\
かく\t核\t\t300
かくご\t覚悟\t\t200
かくにんしょ\t確認書\t\t100
かくにん\t確認\t\t100
かくし\t隠し\t\t400
かくれ\t隠\t下一段-ラ行\t10
かく*s\t隠す
かく{}ばん\t{}番
きしゃ\t記者
";

fn text_dictionary() -> Box<dyn Dictionary> {
    dictionary(WORDS)
}

/// The words converted to a binary dictionary, opened as the input method
/// opens one.
fn binary_dictionary() -> Box<dyn Dictionary> {
    binary(WORDS)
}

fn engine_with(dictionary: Box<dyn Dictionary>, user: &str) -> Engine {
    Engine::new(
        [Slot::UserCustom, Slot::Dictionary(dictionary)],
        TextDictionary::parse_user_custom(user).0,
        Discard,
    )
}

#[test]
fn a_reading_completes_with_readings_of_words_cheapest_then_shortest_first() {
    for dictionary in [text_dictionary(), binary_dictionary()] {
        let e = engine_with(dictionary, "");
        assert_eq!(
            e.complete("かく"),
            ["かくにん", "かくにんしょ", "かくご", "かくし"],
            "no stem, okurigana word, placeholder or the reading itself"
        );
        assert_eq!(e.complete("かくにん"), ["かくにんしょ"]);
        assert_eq!(e.complete("さ"), Vec::<String>::new());
        assert_eq!(e.complete(""), Vec::<String>::new());
    }
}

#[test]
fn the_user_custom_dictionary_completes_first_and_hides_what_it_hides() {
    for dictionary in [text_dictionary(), binary_dictionary()] {
        let e = engine_with(dictionary, "かくまう\t匿う\n!かくし\t隠し\n");
        assert_eq!(
            e.complete("かく"),
            ["かくまう", "かくにん", "かくにんしょ", "かくご"]
        );
    }
}

#[test]
fn a_word_registered_now_completes_at_once() {
    let mut e = engine_with(text_dictionary(), "");
    e.register("かくりつ", "確率");
    assert_eq!(e.complete("かくり"), ["かくりつ"]);
}

#[test]
fn readings_committed_in_the_field_come_first_latest_first() {
    let mut e = engine_with(binary_dictionary(), "");
    e.commit("かくし", "隠し");
    e.commit("かくご", "覚悟");
    assert_eq!(
        e.complete("かく"),
        ["かくご", "かくし", "かくにん", "かくにんしょ"]
    );
    e.move_focus();
    assert_eq!(
        e.complete("かく"),
        ["かくにん", "かくにんしょ", "かくご", "かくし"],
        "the field's history goes with it"
    );
}

#[test]
fn readings_picked_again_and_again_come_next_but_a_single_pick_changes_nothing() {
    let mut e = engine_with(text_dictionary(), "");
    e.replace_selections(Selections::default());
    e.commit("かくし", "隠し");
    e.move_focus();
    assert_eq!(e.complete("かく")[0], "かくにん");
    for _ in 0..2 {
        e.commit("かくし", "隠し");
        e.move_focus();
    }
    assert_eq!(
        e.complete("かく"),
        ["かくし", "かくにん", "かくにんしょ", "かくご"]
    );
    e.commit("かくご", "覚悟");
    assert_eq!(
        e.complete("かく"),
        ["かくご", "かくし", "かくにん", "かくにんしょ"],
        "this field's commit first"
    );
}

#[test]
fn a_reading_is_completed_as_typed_whatever_its_normalization() {
    let e = engine_with(text_dictionary(), "");
    // か followed by a combining voiced mark reads as が, and the reading
    // stays as typed before what completes it, which the core looks for.
    let (dictionary, _) = TextDictionary::parse("がくせい\t学生\n");
    let e2 = engine_with(Box::new(dictionary), "");
    assert_eq!(e2.complete("か\u{3099}く"), ["か\u{3099}くせい"]);
    assert_eq!(e.complete("き"), ["きしゃ"]);
}

/// The engine, shared with the test that teaches it what the core reports,
/// as the input method shares its engine with each field.
#[derive(Clone)]
struct Shared(Rc<RefCell<Engine>>);

impl Converter for Shared {
    fn convert(&self, reading: &str, okurigana: Option<&str>) -> Vec<Candidate> {
        self.0.borrow().convert(reading, okurigana)
    }

    fn complete(&self, reading: &str) -> Vec<String> {
        self.0.borrow().complete(reading)
    }
}

struct Field {
    core: Core<Shared>,
    engine: Rc<RefCell<Engine>>,
    now: u64,
}

impl Field {
    fn new(engine: Engine) -> Self {
        let engine = Rc::new(RefCell::new(engine));
        let config = Config {
            romaji: default_romaji_table(),
            ..Config::default()
        };
        let mut field = Self {
            core: Core::new(Shared(engine.clone()), config),
            engine,
            now: 0,
        };
        field.handle(Event::FocusIn { password: false });
        field.press(Key::ShiftRight, true);
        field.release(Key::ShiftRight);
        field
    }

    fn handle(&mut self, event: Event) -> Output {
        let out = self.core.handle(event);
        for effect in &out.effects {
            self.engine.borrow_mut().learn(effect);
        }
        out
    }

    fn press(&mut self, key: Key, shift: bool) -> Output {
        self.now += 10;
        self.handle(Event::Key(KeyEvent {
            key,
            mods: Modifiers {
                shift,
                ..Modifiers::default()
            },
            kind: KeyKind::Press,
            time_ms: self.now,
        }))
    }

    fn release(&mut self, key: Key) -> Output {
        self.now += 10;
        self.handle(Event::Key(KeyEvent {
            key,
            mods: Modifiers::default(),
            kind: KeyKind::Release,
            time_ms: self.now,
        }))
    }

    fn key(&mut self, key: Key) -> Output {
        let out = self.press(key, false);
        self.release(key);
        out
    }

    fn typ(&mut self, input: &str) -> Output {
        let mut last = None;
        for c in input.chars() {
            last = Some(self.key(Key::Char(c)));
        }
        last.expect("something typed")
    }
}

#[test]
fn tab_completes_the_reading_from_the_dictionaries_and_converts_it() {
    for dictionary in [text_dictionary(), binary_dictionary()] {
        let mut field = Field::new(engine_with(dictionary, ""));
        assert_eq!(field.typ(";kaku").preedit, "›かく");
        assert_eq!(field.key(Key::Tab).preedit, "›かくにん");
        assert_eq!(field.key(Key::Tab).preedit, "›かくにんしょ");
        assert_eq!(field.press(Key::Tab, true).preedit, "›かくにん");
        assert_eq!(field.key(Key::Space).preedit, "»確認");
        assert_eq!(field.key(Key::Enter).commit.as_deref(), Some("確認"));
        // The commit comes first the next time.
        field.typ(";kaku");
        field.key(Key::Tab);
        field.key(Key::Tab);
        assert_eq!(field.key(Key::Tab).preedit, "›かくご");
        assert_eq!(field.typ("ga").preedit, "›かくごが", "ends the completion");
        assert_eq!(field.key(Key::Esc).preedit, "");
        field.typ(";kaku");
        assert_eq!(field.key(Key::Tab).preedit, "›かくにん");
    }
}

#[test]
fn a_reading_nothing_goes_on_from_stays_as_typed() {
    let mut field = Field::new(engine_with(text_dictionary(), ""));
    field.typ(";sakana");
    let out = field.key(Key::Tab);
    assert!(out.consumed);
    assert_eq!(out.preedit, "›さかな");
}

/// `count` readings of かく and two or more kana after it, in the order of
/// their characters, each with `cost`.
fn readings_of_kaku(count: usize, cost: u32) -> Vec<String> {
    const KANA: [char; 10] = ['あ', 'い', 'う', 'え', 'お', 'か', 'き', 'く', 'け', 'こ'];
    (0..count)
        .map(|i| {
            let digits = format!("{i:04}");
            let kana: String = digits
                .chars()
                .map(|d| KANA[d.to_digit(10).unwrap() as usize])
                .collect();
            format!("かく{kana}\t語\t\t{cost}\n")
        })
        .collect()
}

#[test]
fn readings_of_the_same_cost_and_length_complete_in_the_order_of_their_characters() {
    let words = "かくい\t語\t\t5\nかくあ\t語\t\t5\n";
    for dictionary in [dictionary(words), binary(words)] {
        let e = engine_with(dictionary, "");
        assert_eq!(e.complete("かく"), ["かくあ", "かくい"]);
    }
}

#[test]
fn at_most_a_hundred_readings_complete() {
    let words = readings_of_kaku(150, 10).concat();
    let e = engine_with(dictionary(&words), "");
    assert_eq!(e.complete("かく").len(), 100);
}

#[test]
fn each_dictionary_looks_at_its_first_5000_readings_in_the_order_of_their_characters() {
    let mut lines = readings_of_kaku(5001, 10);
    let past = lines.pop().unwrap().replace("\t10\n", "\t1\n");
    let past_reading = past.split('\t').next().unwrap().to_owned();
    let last = lines.pop().unwrap().replace("\t10\n", "\t2\n");
    let last_reading = last.split('\t').next().unwrap().to_owned();
    let words = [lines.concat(), last, past].concat();
    for dictionary in [dictionary(&words), binary(&words)] {
        let completed = engine_with(dictionary, "").complete("かく");
        assert_eq!(completed[0], last_reading, "the 5000th is looked at");
        assert!(!completed.contains(&past_reading), "the 5001st is not");
    }
}

#[test]
fn the_first_5000_readings_are_counted_for_each_dictionary_on_its_own() {
    // Stems complete nothing, so the first dictionary leaves the list empty
    // while it takes up what it may look at.
    let first = readings_of_kaku(5000, 10)
        .concat()
        .replace("\t語\t\t", "\t書\t五段-カ行\t");
    let mut second: Vec<String> = readings_of_kaku(5000, 10)
        .into_iter()
        .map(|line| line.replacen("かく", "かくん", 1))
        .collect();
    let last = second.pop().unwrap().replace("\t10\n", "\t1\n");
    let last_reading = last.split('\t').next().unwrap().to_owned();
    second.push(last);
    let first = dictionary(&first);
    assert_eq!(first.readings_from("かく", usize::MAX).len(), 5000);
    let e = Engine::new(
        [
            Slot::UserCustom,
            Slot::Dictionary(first),
            Slot::Dictionary(dictionary(&second.concat())),
        ],
        TextDictionary::parse_user_custom("").0,
        Discard,
    );
    assert_eq!(e.complete("かく")[0], last_reading);
}

#[test]
fn readings_committed_or_picked_complete_though_no_dictionary_has_them() {
    let mut e = engine_with(text_dictionary(), "");
    e.replace_selections(Selections::default());
    e.commit("かくめい", "革命");
    assert_eq!(e.complete("かく")[0], "かくめい", "committed in the field");
    for _ in 0..2 {
        e.move_focus();
        e.commit("かくめい", "革命");
    }
    e.move_focus();
    assert_eq!(e.complete("かく")[0], "かくめい", "picked again and again");
}

#[test]
fn a_reading_with_a_placeholder_committed_does_not_complete() {
    let mut e = engine_with(dictionary("かく{}ばん\t{}番\n"), "");
    e.set_functions(Notations::shared());
    assert_eq!(surfaces_of(&e, "かく１ばん"), ["１番"]);
    e.commit("かく１ばん", "１番");
    assert_eq!(e.complete("かく"), Vec::<String>::new());
}

#[test]
fn the_ranking_model_does_not_order_the_readings_completed() {
    let mut e = engine_with(text_dictionary(), "");
    let without = e.complete("かく");
    e.set_model(Some(model(
        16,
        &[("s\u{1f}隠し", 9.0), ("s\u{1f}覚悟", 5.0)],
    )));
    assert_eq!(e.complete("かく"), without);
}

/// `words` converted to a binary dictionary, opened as the input method
/// opens one.
fn binary(words: &str) -> Box<dyn Dictionary> {
    let (bytes, invalid) = convert_text(words);
    assert_eq!(invalid, []);
    let path = temp_path("completion.kdic");
    fs::write(&path, bytes).unwrap();
    kanaemi_engine::open_dictionary(&path).unwrap().0
}

fn surfaces_of(engine: &Engine, reading: &str) -> Vec<String> {
    engine
        .convert(reading, None)
        .into_iter()
        .map(|c| c.surface)
        .collect()
}
