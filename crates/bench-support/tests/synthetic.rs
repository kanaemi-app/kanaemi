use std::fs;

use kanaemi_bench_support::{fresh_dir, ranking_model, reading, text_dictionary};
use kanaemi_engine::{BinaryDictionary, Dictionary, RankingModel, TextDictionary, convert_text};

const READINGS: usize = 2_000;

#[test]
fn every_line_of_the_text_dictionary_is_valid() {
    let (_, invalid) = TextDictionary::parse(text_dictionary(READINGS));
    assert_eq!(invalid, []);
}

#[test]
fn the_text_dictionary_has_a_word_for_every_reading() {
    let (dictionary, _) = TextDictionary::parse(text_dictionary(READINGS));
    for i in 0..READINGS {
        assert!(!dictionary.lookup(&reading(i)).is_empty(), "{}", reading(i));
    }
}

#[test]
fn readings_differ() {
    let mut readings: Vec<String> = (0..READINGS).map(reading).collect();
    readings.sort();
    readings.dedup();
    assert_eq!(readings.len(), READINGS);
}

#[test]
fn the_text_dictionary_has_the_words_the_benchmarks_type() {
    let (dictionary, _) = TextDictionary::parse(text_dictionary(READINGS));
    let surfaces = |reading: &str| -> Vec<String> {
        dictionary
            .lookup(reading)
            .into_iter()
            .map(|entry| entry.surface)
            .collect()
    };
    assert!(surfaces("かんじ").contains(&"漢字".to_owned()));
    assert!(surfaces("こう").len() > 30, "enough to page through");
    assert!(!dictionary.okuri("か", 'k').is_empty());
}

#[test]
fn the_text_dictionary_converts_to_a_binary_one() {
    let (bytes, invalid) = convert_text(text_dictionary(READINGS));
    assert_eq!(invalid, []);
    let path = fresh_dir("binary").join("words.kdic");
    fs::write(&path, bytes).unwrap();
    let binary = BinaryDictionary::open(&path).unwrap();
    assert!(!binary.lookup(&reading(READINGS - 1)).is_empty());
}

#[test]
fn the_ranking_model_opens() {
    let path = fresh_dir("model").join("ranking.model");
    fs::write(&path, ranking_model(16)).unwrap();
    RankingModel::open(&path).unwrap();
}

#[test]
fn a_fresh_folder_is_empty_each_time() {
    let dir = fresh_dir("fresh");
    fs::write(dir.join("left"), "").unwrap();
    let again = fresh_dir("fresh");
    assert_eq!(again, dir);
    assert_eq!(fs::read_dir(&again).unwrap().count(), 0);
}
