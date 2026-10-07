//! How often the engine puts first what the user means: the cases in
//! `tests/accuracy`, ranked by the rules and by a ranking model.
//!
//! With the small dictionary and the tiny model there, the misses must be the
//! ones recorded there, so a change that loses a case fails, and one that wins
//! a case updates the record on purpose. With real dictionaries the misses are
//! only reported.

use std::collections::BTreeSet;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use kanaemi_config::{binary_name, dictionary_files};
use kanaemi_core::Converter;
use kanaemi_engine::{
    Dictionary, Engine, RankingModel, Selections, Slot, TextDictionary, open_dictionary,
};
use kanaemi_functions::LuauFunctions;

use crate::common::{Discard, Learn, model};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/accuracy");
const DICTIONARY: &str = "dictionary.tsv";

/// As many candidates as a page of the candidate window shows.
const TOP_N: usize = 9;

/// What happens in the field before the reading is converted.
enum Step {
    Commit(String, String),
    Pick(String, String),
    Type(String),
}

struct Case {
    /// The case as the record of misses writes it.
    line: String,
    reading: String,
    okurigana: Option<String>,
    surface: String,
    before: Vec<Step>,
}

/// The lines of a fixture that are neither empty nor a comment.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

fn read(name: &str) -> String {
    std::fs::read_to_string(format!("{FIXTURES}/{name}")).unwrap()
}

fn cases() -> Vec<Case> {
    lines(&read("cases.tsv")).map(case).collect()
}

fn case(line: &str) -> Case {
    let mut fields = line.split('\t');
    let (Some(written), Some(surface)) = (fields.next(), fields.next()) else {
        panic!("a case without a surface: {line:?}");
    };
    let (reading, okurigana) = match written.split_once('*') {
        Some((stem, okurigana)) => (format!("{stem}{okurigana}"), Some(okurigana.to_owned())),
        None => (written.to_owned(), None),
    };
    let before = fields
        .next()
        .unwrap_or("")
        .split(';')
        .map(str::trim)
        .filter(|step| !step.is_empty())
        .map(|step| match step.split(' ').collect::<Vec<_>>()[..] {
            ["commit", reading, surface] => Step::Commit(reading.into(), surface.into()),
            ["pick", reading, surface] => Step::Pick(reading.into(), surface.into()),
            ["type", text] => Step::Type(text.into()),
            _ => panic!("an unknown step {step:?}: {line:?}"),
        })
        .collect();
    Case {
        line: line.to_owned(),
        reading,
        okurigana,
        surface: surface.to_owned(),
        before,
    }
}

/// The candidates for the case, on an engine that forgets what earlier cases
/// taught it.
fn convert(engine: &mut Engine, case: &Case) -> Vec<String> {
    engine.move_focus();
    engine.replace_selections(Selections::default());
    for step in &case.before {
        match step {
            Step::Commit(reading, surface) => commit(engine, reading, surface),
            Step::Pick(reading, surface) => {
                commit(engine, reading, surface);
                engine.move_focus();
            }
            Step::Type(text) => engine.type_text(text),
        }
    }
    engine
        .convert(&case.reading, case.okurigana.as_deref())
        .into_iter()
        .map(|c| c.surface)
        .collect()
}

/// Converts and commits as the core reports a commit: the pair committed, then
/// the surface typed into the field.
fn commit(engine: &mut Engine, reading: &str, surface: &str) {
    engine.convert(reading, None);
    engine.commit(reading, surface);
    engine.type_text(surface);
}

/// Ranks every case, prints how many came first and within [`TOP_N`] and
/// each miss, and gives the misses as the record of misses writes them.
fn measure(
    dictionaries: &str,
    ranking: &str,
    engine: &mut Engine,
    cases: &[Case],
) -> BTreeSet<String> {
    let mut misses = BTreeSet::new();
    let mut report = String::new();
    let (mut top_1, mut top_n) = (0, 0);
    for case in cases {
        let candidates = convert(engine, case);
        let at = candidates.iter().position(|c| *c == case.surface);
        for (measure, hit, count) in [
            ("top-1", at == Some(0), &mut top_1),
            ("top-N", at.is_some_and(|at| at < TOP_N), &mut top_n),
        ] {
            if hit {
                *count += 1;
                continue;
            }
            let first = candidates.first().map_or("nothing", String::as_str);
            report += &format!("  {measure} miss: {} (first: {first})\n", case.line);
            misses.insert(format!("{ranking}\t{measure}\t{}", case.line));
        }
    }
    let percent = |hits: usize| 100.0 * hits as f64 / cases.len().max(1) as f64;
    println!(
        "{dictionaries}, {ranking}: top-1 {top_1}/{total} ({:.1}%), top-{TOP_N} {top_n}/{total} ({:.1}%)\n{report}",
        percent(top_1),
        percent(top_n),
        total = cases.len(),
    );
    misses
}

/// An engine with the built-in functions, as the IME fills placeholders; the
/// fixtures hold no functions of their own.
fn engine(dictionaries: impl IntoIterator<Item = Box<dyn Dictionary>>) -> Engine {
    let slots =
        std::iter::once(Slot::UserCustom).chain(dictionaries.into_iter().map(Slot::Dictionary));
    let mut engine = Engine::new(slots, TextDictionary::parse_user_custom("").0, Discard);
    engine.set_functions(Some(Rc::new(LuauFunctions::open(FIXTURES))));
    engine
}

fn open(path: &Path) -> Box<dyn Dictionary> {
    let (dictionary, invalid) = open_dictionary(path).unwrap();
    assert_eq!(invalid, [], "{}", path.display());
    dictionary
}

/// The model `model.tsv` writes, a feature's parts joined by `|`.
fn tiny_model() -> Arc<RankingModel> {
    let weights: Vec<(String, f32)> = lines(&read("model.tsv"))
        .map(|line| {
            let (feature, weight) = line.split_once('\t').unwrap();
            (feature.replace('|', "\u{1f}"), weight.parse().unwrap())
        })
        .collect();
    let weights: Vec<(&str, f32)> = weights.iter().map(|(f, w)| (f.as_str(), *w)).collect();
    model(16, &weights)
}

#[test]
fn the_cases_miss_only_as_recorded() {
    let cases = cases();
    let mut e = engine([open(Path::new(&format!("{FIXTURES}/{DICTIONARY}")))]);
    let mut misses = measure(DICTIONARY, "rules", &mut e, &cases);
    e.set_model(Some(tiny_model()));
    misses.append(&mut measure(DICTIONARY, "model", &mut e, &cases));

    let recorded: BTreeSet<String> = lines(&read("baseline.tsv")).map(str::to_owned).collect();
    let list = |lines: Vec<&String>| lines.iter().map(|l| format!("  {l}\n")).collect::<String>();
    let regressed = list(misses.difference(&recorded).collect());
    let improved = list(recorded.difference(&misses).collect());
    assert!(
        regressed.is_empty() && improved.is_empty(),
        "the misses differ from tests/accuracy/baseline.tsv\n\
         regressed, missed now but not recorded:\n{regressed}\
         improved, recorded but hit now:\n{improved}",
    );
}

/// The dictionaries in the folder `KANAEMI_ACCURACY_DICTIONARIES`, read as the
/// IME reads its dictionary folder without a list, and the ranking model
/// `KANAEMI_ACCURACY_MODEL` too when it is set. Skipped without the folder.
#[test]
fn the_cases_with_the_dictionaries_given() {
    let Some(folder) = std::env::var_os("KANAEMI_ACCURACY_DICTIONARIES") else {
        return;
    };
    let folder = Path::new(&folder);
    let shown = folder.display().to_string();
    let files = dictionary_files(folder);
    let mut e = engine(
        files
            .iter()
            .filter(|name| {
                let binary = binary_name(name);
                **name == binary || !files.contains(&binary)
            })
            .map(|name| open_dictionary(folder.join(name)).unwrap().0),
    );
    let cases = cases();
    measure(&shown, "rules", &mut e, &cases);
    if let Some(path) = std::env::var_os("KANAEMI_ACCURACY_MODEL") {
        e.set_model(Some(Arc::new(RankingModel::open(path).unwrap())));
        measure(&shown, "model", &mut e, &cases);
    }
}
