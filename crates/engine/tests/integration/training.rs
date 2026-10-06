use kanaemi_core::Converter;
use kanaemi_engine::{CandidateFacts, Engine, RankingInput, Slot, TextDictionary, feature_indices};

use crate::common::{Discard, Learn, Notations, dictionary, model};

fn engine(slots: Vec<Slot>) -> Engine {
    let mut engine = Engine::new(slots, TextDictionary::parse_user_custom("").0, Discard);
    engine.set_functions(Notations::shared());
    engine
}

fn text(text: &str) -> Slot {
    Slot::Dictionary(dictionary(text))
}

fn facts(surface: &str, dictionary: usize, cost: u32, built: bool) -> CandidateFacts {
    CandidateFacts {
        surface: surface.to_owned(),
        dictionary,
        cost,
        built,
        template: None,
    }
}

fn by_surface(mut facts: Vec<CandidateFacts>) -> Vec<CandidateFacts> {
    facts.sort_by(|a, b| a.surface.cmp(&b.surface));
    facts
}

const KISHA: &str = "きしゃ\t記者\t\t10\nきしゃ\t汽車\t\t20\nきしゃ\t貴社\t\t30\n";

#[test]
fn candidate_facts_are_every_candidate_before_it_is_ranked() {
    let mut e = engine(vec![Slot::UserCustom, text(KISHA)]);
    let before = e.candidate_facts("きしゃ", None);
    assert_eq!(
        by_surface(before.clone()),
        by_surface(vec![
            facts("記者", 1, 10, false),
            facts("汽車", 1, 20, false),
            facts("貴社", 1, 30, false),
        ])
    );

    e.commit("きしゃ", "貴社");
    e.set_model(Some(model(16, &[("s\u{1f}汽車", 2.0)])));
    assert_eq!(e.candidate_facts("きしゃ", None), before);

    let mut ranked: Vec<String> = e
        .convert("きしゃ", None)
        .into_iter()
        .map(|c| c.surface)
        .collect();
    ranked.sort();
    let mut gathered: Vec<String> = before.into_iter().map(|f| f.surface).collect();
    gathered.sort();
    assert_eq!(ranked, gathered);
}

#[test]
fn candidate_facts_with_okurigana_are_the_candidates_split_at_the_mark() {
    let e = engine(vec![
        Slot::UserCustom,
        text("か\t書\t五段-カ行\t100"),
        text("かく\t画く\t\t0\nか*く\t掻く\t\t1\nかく\t角\t\t2"),
    ]);
    assert_eq!(
        by_surface(e.candidate_facts("かく", Some("く"))),
        by_surface(vec![
            facts("書く", 1, 600, true),
            facts("掻く", 2, 1, false),
            facts("画く", 2, 0, false),
        ])
    );
}

#[test]
fn candidate_facts_look_the_reading_up_composed() {
    let e = engine(vec![Slot::UserCustom, text("かぐ\t家具\t\t5")]);
    assert_eq!(
        e.candidate_facts("か\u{304f}\u{3099}", None),
        [facts("家具", 1, 5, false)]
    );
}

#[test]
fn candidate_facts_of_a_number_carry_the_numeric_item_the_history_records() {
    let e = engine(vec![
        Slot::UserCustom,
        text("{}こ\t{kanji}個\nごこ\t五個\n"),
    ]);
    let item = ("\u{FDD0}\u{FDD1}こ", "\u{FDD0}kanji\u{FDD1}個");
    let filled = e.candidate_facts("5こ", None);
    assert_eq!(
        filled,
        [CandidateFacts {
            template: Some((item.0.to_owned(), item.1.to_owned())),
            ..facts("五個", 1, 0, false)
        }]
    );
    assert_eq!(filled[0].recorded("5こ"), item);

    let found = e.candidate_facts("ごこ", None);
    assert_eq!(found, [facts("五個", 1, 0, false)]);
    assert_eq!(found[0].recorded("ごこ"), ("ごこ", "五個"));
}

#[test]
fn feature_indices_point_at_the_weights_a_model_scores_with() {
    let input = RankingInput {
        reading: "きしゃ",
        history: &[],
        context: "",
    };
    let surface = xxhash_rust::xxh3::xxh3_64("s\u{1f}貴社".as_bytes()) & ((1 << 16) - 1);
    assert!(
        feature_indices(&input, &facts("貴社", 1, 30, false), 16).contains(&(surface as usize))
    );
}
