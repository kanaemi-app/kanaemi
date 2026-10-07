//! Reading the built-in functions, calling each, and converting a reading
//! whose words they fill.

use std::hint::black_box;
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use kanaemi_bench_support::fresh_dir;
use kanaemi_core::Converter;
use kanaemi_engine::{Call, Engine, Functions, LineSink, Slot, TextDictionary};
use kanaemi_functions::LuauFunctions;

/// A call of each built-in function, as a dictionary's placeholders make it:
/// the case's name, then the function's.
const CALLS: &[(&str, &str, &str, Option<&str>)] = &[
    ("kanji", "kanji", "２０２６", None),
    ("daiji", "daiji", "２０２６", None),
    ("kanji-num", "kanji-num", "２０２６", None),
    ("half-num", "half-num", "００７", None),
    ("wide-num", "wide-num", "007", None),
    ("grouped-num", "grouped-num", "１２３４５６７", None),
    ("wareki", "wareki", "２０２６", None),
    ("eto", "eto", "えと", None),
    ("date", "date", "きょう", Some("%Y-%m-%d")),
    (
        "date-shifted",
        "date",
        "あした",
        Some("+1 %E%N年%-m月%-d日"),
    ),
    ("uuid", "uuid", "", None),
    ("ulid", "ulid", "", None),
    ("random", "random", "", Some("1 6")),
    ("choice", "choice", "", Some("表 裏")),
];

const DICTIONARY: &str = "きょう\t{-:date %Y-%m-%d}\n\
きょう\t{-:date %Y/%m/%d}\n\
きょう\t{-:date %-m月%-d日}\n\
きょう\t{-:date %Y年%-m月%-d日}\n\
きょう\t{-:date %E%N年%-m月%-d日}\n\
きょう\t{-:date %-m月%-d日（%a）}\n\
{}ねん\t{}年\n\
{}ねん\t{kanji}年\n\
{}ねん\t{wareki}年\n";

/// Samples taken before a function stopping in each is taken as a fault.
const ATTEMPTS: usize = 5;

struct Discard;

impl LineSink for Discard {
    fn append(&mut self, _line: &str) -> io::Result<()> {
        Ok(())
    }
}

/// Times `iters` runs of `run` on what `make` builds from the functions read
/// afresh, until a sample passes with no function stopped: a stall of the
/// machine past the time limit stops a function for good, and the runs after
/// it would time nothing. Gives up when every attempt stops one, as then the
/// fault is not a stall.
fn sample<T>(
    dir: &Path,
    iters: u64,
    make: impl Fn(Rc<LuauFunctions>) -> T,
    run: impl Fn(&T),
) -> Duration {
    let mut errors = Vec::new();
    for _ in 0..ATTEMPTS {
        let functions = Rc::new(LuauFunctions::open(dir));
        let made = make(functions.clone());
        let start = Instant::now();
        for _ in 0..iters {
            run(&made);
        }
        let elapsed = start.elapsed();
        errors = functions.take_errors();
        if errors.is_empty() {
            return elapsed;
        }
    }
    panic!("a function stopped in every attempt: {errors:?}");
}

fn functions(c: &mut Criterion) {
    let dir = fresh_dir("functions");
    let functions = LuauFunctions::open(&dir);
    let mut group = c.benchmark_group("functions");
    group.bench_function("open", |b| b.iter(|| LuauFunctions::open(&dir)));
    for &(case, name, source, argument) in CALLS {
        let call = Call {
            name,
            source,
            argument,
        };
        assert!(functions.call(&call).is_some(), "{case}");
        group.bench_function(format!("call/{case}"), |b| {
            b.iter_custom(|iters| {
                sample(
                    &dir,
                    iters,
                    |functions| functions,
                    |functions| {
                        black_box(functions.call(black_box(&call)));
                    },
                )
            })
        });
    }
    group.finish();
}

fn engine(functions: Rc<LuauFunctions>) -> Engine {
    let (dictionary, _) = TextDictionary::parse(DICTIONARY);
    let (user, _) = TextDictionary::parse_user_custom("");
    let mut engine = Engine::new([Slot::Dictionary(Box::new(dictionary))], user, Discard);
    engine.set_functions(Some(functions as Rc<dyn Functions>));
    engine
}

fn placeholders(c: &mut Criterion) {
    let dir = fresh_dir("placeholders");
    assert_eq!(TextDictionary::parse(DICTIONARY).1, []);
    let checked = engine(Rc::new(LuauFunctions::open(&dir)));
    let mut group = c.benchmark_group("placeholders");
    for (case, reading, candidates) in [("date", "きょう", 6), ("number", "２０２６ねん", 3)]
    {
        assert_eq!(checked.convert(reading, None).len(), candidates, "{case}");
        group.bench_function(case, |b| {
            b.iter_custom(|iters| {
                sample(&dir, iters, engine, |engine| {
                    black_box(engine.convert(black_box(reading), None));
                })
            })
        });
    }
    group.finish();
}

criterion_group!(benches, functions, placeholders);
criterion_main!(benches);
