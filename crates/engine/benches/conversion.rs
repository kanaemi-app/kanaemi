//! Looking a reading up and ranking its candidates, on a dictionary as large
//! as the ones users install, with a ranking model.

use std::fs;
use std::hint::black_box;
use std::io;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use kanaemi_bench_support::{fresh_dir, ranking_model, reading, text_dictionary};
use kanaemi_core::Converter;
use kanaemi_engine::{
    BinaryDictionary, Dictionary, Engine, LineSink, RankingModel, Slot, TextDictionary,
    convert_text,
};

const READINGS: usize = 200_000;
const MODEL_BITS: u8 = 20;
/// Readings of the dictionary looked up one after another, so no single
/// reading's luck decides the result.
const SPREAD: usize = 64;

struct Discard;

impl LineSink for Discard {
    fn append(&mut self, _line: &str) -> io::Result<()> {
        Ok(())
    }
}

fn engine(dictionary: Box<dyn Dictionary>, model: &Arc<RankingModel>) -> Engine {
    let (user, _) = TextDictionary::parse_user_custom("");
    let mut engine = Engine::new([Slot::Dictionary(dictionary)], user, Discard);
    engine.set_model(Some(model.clone()));
    engine
}

fn conversion(c: &mut Criterion) {
    let dir = fresh_dir("engine-conversion");
    let text = text_dictionary(READINGS);
    let binary_path = dir.join("words.kdic");
    fs::write(&binary_path, convert_text(&text).0).unwrap();
    let model_path = dir.join("ranking.model");
    fs::write(&model_path, ranking_model(MODEL_BITS)).unwrap();
    let model = Arc::new(RankingModel::open(&model_path).unwrap());
    let engines = [
        (
            "text",
            engine(Box::new(TextDictionary::parse(&text).0), &model),
        ),
        (
            "binary",
            engine(
                Box::new(BinaryDictionary::open(&binary_path).unwrap()),
                &model,
            ),
        ),
    ];
    let spread: Vec<String> = (0..SPREAD)
        .map(|i| reading(i * (READINGS / SPREAD)))
        .collect();

    for (kind, engine) in &engines {
        // Each case measures what its name says only while it finds that.
        assert!(engine.convert("こう", None).len() > 30, "{kind}");
        assert!(!engine.convert("かく", Some("く")).is_empty(), "{kind}");
        assert!(!engine.convert("１２こ", None).is_empty(), "{kind}");
        assert!(spread.iter().all(|r| !engine.convert(r, None).is_empty()));

        let mut group = c.benchmark_group(format!("convert/{kind}"));
        group.bench_function("few", |b| {
            b.iter(|| engine.convert(black_box("かんじ"), None))
        });
        group.bench_function("many", |b| {
            b.iter(|| engine.convert(black_box("こう"), None))
        });
        group.bench_function("none", |b| {
            b.iter(|| engine.convert(black_box("ぬぬぬぬぬ"), None))
        });
        group.bench_function("okurigana", |b| {
            b.iter(|| engine.convert(black_box("かく"), Some("く")))
        });
        group.bench_function("number", |b| {
            b.iter(|| engine.convert(black_box("１２こ"), None))
        });
        group.bench_function(format!("spread-{SPREAD}"), |b| {
            b.iter(|| {
                for reading in &spread {
                    black_box(engine.convert(reading, None));
                }
            })
        });
        group.finish();
    }
}

criterion_group!(benches, conversion);
criterion_main!(benches);
