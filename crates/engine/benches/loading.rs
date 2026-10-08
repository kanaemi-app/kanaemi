//! Opening what the engine reads: a dictionary as large as the ones users
//! install, as text and as binary, and the ranking model.

use std::fs;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use kanaemi_bench_support::{fresh_dir, ranking_model, text_dictionary};
use kanaemi_engine::{BinaryDictionary, RankingModel, TextDictionary, convert_text};

const READINGS: usize = 200_000;
const MODEL_BITS: u8 = 20;

fn dictionaries(c: &mut Criterion) {
    let dir = fresh_dir("engine-loading");
    let text = text_dictionary(READINGS);
    let (binary, invalid) = convert_text(&text);
    assert_eq!(invalid, []);
    let binary_path = dir.join("words.kdic");
    fs::write(&binary_path, &binary).unwrap();

    let mut group = c.benchmark_group("open");
    group.sample_size(10);
    group.bench_function(BenchmarkId::new("text", READINGS), |b| {
        b.iter(|| TextDictionary::parse(&text))
    });
    group.bench_function(BenchmarkId::new("binary", READINGS), |b| {
        b.iter(|| BinaryDictionary::open(&binary_path).unwrap())
    });
    group.bench_function(BenchmarkId::new("binary-verified", READINGS), |b| {
        b.iter(|| {
            let dictionary = BinaryDictionary::open(&binary_path).unwrap();
            dictionary.verify_checksums().unwrap();
            dictionary
        })
    });
    group.bench_function(BenchmarkId::new("convert", READINGS), |b| {
        b.iter(|| convert_text(&text))
    });
    group.finish();
}

fn model(c: &mut Criterion) {
    let path = fresh_dir("engine-model").join("ranking.model");
    fs::write(&path, ranking_model(MODEL_BITS)).unwrap();

    let mut group = c.benchmark_group("open");
    group.bench_function(BenchmarkId::new("model", MODEL_BITS), |b| {
        b.iter(|| RankingModel::open(&path).unwrap())
    });
    group.finish();
}

criterion_group!(benches, dictionaries, model);
criterion_main!(benches);
