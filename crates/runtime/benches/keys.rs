//! From a key to what the field shows, through the profile every platform's
//! input method uses: a dictionary as large as the ones users install and a
//! ranking model in the settings folder, the default settings and the
//! built-in dictionaries and functions.

use std::fs;
use std::io;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use kanaemi_bench_support::{fresh_dir, ranking_model, text_dictionary};
use kanaemi_config::{DICTIONARY_DIR, MODEL_FILE};
use kanaemi_core::{Event, Key, KeyEvent, KeyKind, Mode, Modifiers, Output};
use kanaemi_engine::convert_text;
use kanaemi_runtime::{Access, Field, LineSink, Profile};

const READINGS: usize = 200_000;
const MODEL_BITS: u8 = 20;
/// Syllables typed for readings no dictionary has, to register.
const SYLLABLES: [&str; 8] = ["nu", "mu", "ru", "he", "me", "ne", "re", "yu"];
/// Spaces past which registering has not started, and never will.
const MAX_PAGES: usize = 100;

struct Discard;

impl LineSink for Discard {
    fn append(&mut self, _line: &str) -> io::Result<()> {
        Ok(())
    }
}

/// Sandboxed, so registering writes no file and every run starts alike.
fn discard() -> Box<dyn LineSink> {
    Box::new(Discard)
}

/// A field with the focus, typed into as a host reports keys.
struct Typist {
    profile: Profile,
    field: Field,
    now: u64,
    /// Counts keys sent, presses and releases alike.
    events: u64,
}

impl Typist {
    fn new() -> Self {
        let dir = fresh_dir("runtime-keys");
        fs::create_dir_all(dir.join(DICTIONARY_DIR)).unwrap();
        let (binary, invalid) = convert_text(text_dictionary(READINGS));
        assert_eq!(invalid, []);
        fs::write(dir.join(DICTIONARY_DIR).join("words.kdic"), binary).unwrap();
        fs::write(dir.join(MODEL_FILE), ranking_model(MODEL_BITS)).unwrap();
        let mut profile = Profile::open_with(&dir, Access::Sandboxed(discard));
        let mut field = Field::new(&profile);
        field.handle(&mut profile, Event::FocusIn { password: false });
        field.handle(&mut profile, Event::SetMode(Mode::Kana));
        Self {
            profile,
            field,
            now: 0,
            events: 0,
        }
    }

    fn event(&mut self, key: Key, kind: KeyKind) -> Output {
        self.now += 10;
        self.events += 1;
        let shift = matches!(key, Key::Char(c) if c.is_ascii_uppercase());
        let event = Event::Key(KeyEvent {
            key,
            mods: Modifiers {
                shift,
                ..Modifiers::default()
            },
            kind,
            time_ms: self.now,
        });
        self.field.handle(&mut self.profile, event)
    }

    /// Presses `key` and lets it go; what it committed, and what it shows.
    fn key(&mut self, key: Key) -> (String, Output) {
        let pressed = self.event(key, KeyKind::Press);
        let released = self.event(key, KeyKind::Release);
        let commit = [pressed.commit, released.commit.clone()]
            .into_iter()
            .flatten()
            .collect();
        (commit, released)
    }

    fn type_text(&mut self, text: &str) -> (String, Output) {
        let mut committed = String::new();
        let mut last = None;
        for c in text.chars() {
            let (commit, output) = self.key(Key::Char(c));
            committed.push_str(&commit);
            last = Some(output);
        }
        (committed, last.expect("a key"))
    }

    fn sentence(&mut self) -> String {
        self.type_text("kyouhaiitenkidesune.").0
    }

    fn convert(&mut self) -> String {
        self.type_text(";kanji");
        self.key(Key::Space);
        self.key(Key::Enter).0
    }

    fn okurigana(&mut self) -> String {
        self.type_text(";ka;ku");
        self.key(Key::Enter).0
    }

    /// Goes through the first pages of candidates and commits the one there.
    fn page(&mut self) -> String {
        self.type_text(";kou");
        for _ in 0..20 {
            self.key(Key::Space);
        }
        self.key(Key::Enter).0
    }

    /// Registers a word for a reading never registered before, past its
    /// candidates.
    fn register(&mut self, word: usize) -> String {
        let mut reading = String::from(";");
        let mut n = word;
        for _ in 0..7 {
            reading.push_str(SYLLABLES[n % SYLLABLES.len()]);
            n /= SYLLABLES.len();
        }
        self.type_text(&reading);
        let mut output = self.key(Key::Space).1;
        let mut pages = 1;
        while !output.preedit.ends_with(" « ") {
            assert!(pages < MAX_PAGES, "no registering past {reading}");
            output = self.key(Key::Space).1;
            pages += 1;
        }
        self.type_text("aiu");
        self.key(Key::Enter).0
    }
}

/// Benches `run` on `typist`, per sequence of keys, with its throughput in
/// key events.
fn bench(c: &mut Criterion, typist: &mut Typist, name: &str, mut run: impl FnMut(&mut Typist)) {
    let before = typist.events;
    run(typist);
    let mut group = c.benchmark_group("keys");
    group.throughput(Throughput::Elements(typist.events - before));
    group.bench_function(name, |b| b.iter(|| run(typist)));
    group.finish();
}

fn keys(c: &mut Criterion) {
    let mut typist = Typist::new();
    assert_eq!(typist.sentence(), "きょうはいいてんきですね。");
    // The model ranks at random, so any of the words may come first.
    assert!(["漢字", "感じ", "幹事", "監事", "完治"].contains(&typist.convert().as_str()));
    assert!(["書く", "欠く"].contains(&typist.okurigana().as_str()));
    assert!(!typist.page().is_empty());
    assert_eq!(typist.register(0), "あいう");

    bench(c, &mut typist, "sentence", |t| {
        t.sentence();
    });
    bench(c, &mut typist, "convert", |t| {
        t.convert();
    });
    bench(c, &mut typist, "okurigana", |t| {
        t.okurigana();
    });
    bench(c, &mut typist, "page", |t| {
        t.page();
    });
    let mut word = 1;
    bench(c, &mut typist, "register", |t| {
        t.register(word);
        word += 1;
    });
}

criterion_group!(benches, keys);
criterion_main!(benches);
