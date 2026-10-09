//! Helpers the core's test files share. Each test file is a crate of its own and uses only some of them.
#![allow(dead_code)]

use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use kanaemi_core::{
    Action, Binding, Candidate, Chord, Config, Converter, Core, Effect, Event, Form, Gesture, Key,
    KeyEvent, KeyKind, Modifiers, Output, RomajiTable,
};

/// Plain syllables by row, each a consonant and the kana it makes with
/// a, i, u, e and o.
const ROWS: &[(&str, &str)] = &[
    ("", "あいうえお"),
    ("k", "かきくけこ"),
    ("s", "さしすせそ"),
    ("t", "たちつてと"),
    ("n", "なにぬねの"),
    ("h", "はひふへほ"),
    ("m", "まみむめも"),
    ("r", "らりるれろ"),
    ("g", "がぎぐげご"),
    ("z", "ざじずぜぞ"),
    ("d", "だぢづでど"),
    ("b", "ばびぶべぼ"),
    ("p", "ぱぴぷぺぽ"),
];

/// Syllables off the plain rows, and the punctuation typed over the
/// full-width symbols.
const OTHERS: &str = "ya\tや\nyu\tゆ\nyo\tよ\nwa\tわ\nwo\tを\nnn\tん\nn'\tん\n\
shi\tし\nchi\tち\ntsu\tつ\nfu\tふ\nji\tじ\nja\tじゃ\nkya\tきゃ\nkyu\tきゅ\nkyo\tきょ\n\
sha\tしゃ\nxa\tぁ\nxtu\tっ\nltu\tっ\nvu\tゔ\ndhi\tでぃ\n\
-\tー\n,\t、\n.\t。";

/// The romaji table the tests type with: printable ASCII but letters in
/// full width, then the syllables and punctuation stacked over it.
pub fn romaji() -> RomajiTable {
    let mut full_width = String::new();
    for c in ('!'..='~').filter(|c| !c.is_ascii_alphabetic()) {
        let wide = char::from_u32(c as u32 + 0xFEE0).expect("a full-width form");
        let input = match c {
            '#' | '!' => format!("\\{c}"),
            '\\' => "\\\\".to_owned(),
            c => c.to_string(),
        };
        full_width.push_str(&format!("{input}\t{wide}\n"));
    }
    let mut syllables = String::new();
    for (consonant, kana) in ROWS {
        for (vowel, kana) in "aiueo".chars().zip(kana.chars()) {
            syllables.push_str(&format!("{consonant}{vowel}\t{kana}\n"));
        }
    }
    table(&[&full_width, &syllables, OTHERS])
}

/// The default configuration, typing with [`romaji`].
pub fn config() -> Config {
    Config {
        romaji: romaji(),
        ..Config::default()
    }
}

/// What the host learned from the core's effects.
#[derive(Default)]
pub struct Learned {
    pub okurigana_seen: Vec<Option<String>>,
    pub commits: Vec<(String, String)>,
    pub texts: Vec<String>,
    /// Readings mark okurigana with `*`, as a text dictionary writes them.
    pub registered: Vec<(String, String)>,
    /// The okurigana's first chunk of each word in `registered`.
    pub registered_heads: Vec<Option<String>>,
    pub deleted: Vec<(String, String)>,
    pub withdrawn: Vec<(String, String)>,
    pub erased: Vec<String>,
    pub resets: usize,
}

impl Learned {
    pub fn learn(&mut self, effect: &Effect) {
        match effect.clone() {
            Effect::Committed {
                reading, surface, ..
            } => self.commits.push((reading, surface)),
            Effect::Registered {
                reading,
                okurigana,
                okurigana_head,
                surface,
            } => {
                let reading = match okurigana {
                    Some(okurigana) => format!("{reading}*{okurigana}"),
                    None => reading,
                };
                self.registered.push((reading, surface));
                self.registered_heads.push(okurigana_head);
            }
            Effect::Forgotten {
                reading, surface, ..
            } => self.deleted.push((reading, surface)),
            Effect::Withdrawn {
                reading, surface, ..
            } => self.withdrawn.push((reading, surface)),
            Effect::Typed(text) => self.texts.push(text),
            Effect::Erased(text) => self.erased.push(text),
            Effect::FocusMoved => self.resets += 1,
        }
    }
}

#[derive(Clone, Default)]
pub struct Fake {
    pub table: HashMap<&'static str, Vec<&'static str>>,
    pub learned: Rc<RefCell<Learned>>,
}

impl Converter for Fake {
    fn convert(&self, reading: &str, okurigana: Option<&str>) -> Vec<Candidate> {
        let mut learned = self.learned.borrow_mut();
        learned.okurigana_seen.push(okurigana.map(str::to_owned));
        self.table.get(reading).map_or_else(Vec::new, |surfaces| {
            surfaces
                .iter()
                .filter(|s| okurigana.is_none_or(|o| s.ends_with(o)))
                .filter(|s| !learned.deleted.iter().any(|(r, d)| r == reading && d == *s))
                .map(|s| Candidate {
                    surface: s.to_string(),
                })
                .collect()
        })
    }

    /// The readings of the table that go on from `reading`, shortest first.
    fn complete(&self, reading: &str) -> Vec<String> {
        let mut readings: Vec<String> = self
            .table
            .keys()
            .filter(|r| r.len() > reading.len() && r.starts_with(reading))
            .map(|r| r.to_string())
            .collect();
        readings.sort_by_key(|r| (r.chars().count(), r.clone()));
        readings
    }

    /// Puts the reading's full-width digits, in ASCII, where the word has `{}`.
    fn registered_text(&self, reading: &str, _okurigana: Option<&str>, surface: &str) -> String {
        let digits: String = reading
            .chars()
            .filter(|c| ('０'..='９').contains(c))
            .filter_map(|c| char::from_u32(c as u32 - 0xFEE0))
            .collect();
        surface.replace("{}", &digits)
    }
}

pub struct T {
    pub core: Core<Fake>,
    pub learned: Rc<RefCell<Learned>>,
    pub now: u64,
}

impl T {
    pub fn new() -> Self {
        Self::with_config(config())
    }

    pub fn with_config(config: Config) -> Self {
        let mut fake = Fake::default();
        fake.table.insert("かんじ", vec!["漢字", "感じ", "幹事"]);
        fake.table.insert("きしゃ", vec!["記者", "汽車"]);
        fake.table.insert("かく", vec!["角", "書く", "核"]);
        fake.table.insert("たべ", vec!["食べ"]);
        fake.table.insert("かな", vec!["カナ", "仮名"]);
        fake.table.insert("もっ", vec!["持っ"]);
        fake.table.insert("もった", vec!["持った"]);
        fake.table.insert("いっ", vec!["行っ", "言っ"]);
        fake.table.insert("いった", vec!["行った", "言った"]);
        fake.table.insert("うっ", vec!["売っ", "うっ"]);
        fake.table.insert("うった", vec!["売った"]);
        // As numeric items fill their placeholders with the typed number.
        fake.table.insert("１こ", vec!["1個", "１個", "一個"]);
        fake.table.insert(
            "こう",
            vec![
                "高", "校", "行", "考", "効", "項", "構", "講", "公", "工", "功", "孝",
            ],
        );
        let learned = fake.learned.clone();
        let mut t = T {
            core: Core::new(fake, config),
            learned,
            now: 1_000,
        };
        t.handle(Event::FocusIn { password: false });
        t
    }

    /// Handles an event the way a host does: what it teaches is learned.
    pub fn handle(&mut self, event: Event) -> Output {
        let out = self.core.handle(event);
        for effect in &out.effects {
            self.learned.borrow_mut().learn(effect);
        }
        out
    }

    pub fn press(&mut self, key: Key, mods: Modifiers) -> Output {
        self.now += 10;
        self.handle(Event::Key(KeyEvent {
            key,
            mods,
            kind: KeyKind::Press,
            time_ms: self.now,
        }))
    }

    /// The OS pressing `key` again while it is held.
    pub fn repeat(&mut self, key: Key, mods: Modifiers) -> Output {
        self.now += 10;
        self.handle(Event::Key(KeyEvent {
            key,
            mods,
            kind: KeyKind::Repeat,
            time_ms: self.now,
        }))
    }

    pub fn release(&mut self, key: Key) -> Output {
        self.now += 10;
        self.handle(Event::Key(KeyEvent {
            key,
            mods: Modifiers::default(),
            kind: KeyKind::Release,
            time_ms: self.now,
        }))
    }

    /// Presses a key and lets it go, as a host reports one keystroke; what
    /// the two events gave, together.
    pub fn key(&mut self, key: Key) -> Output {
        let pressed = self.press(key, Modifiers::default());
        let released = self.release(key);
        let commit = match (pressed.commit, released.commit) {
            (Some(a), Some(b)) => Some(a + &b),
            (a, b) => a.or(b),
        };
        Output {
            consumed: pressed.consumed,
            commit,
            indicator: pressed.indicator.or(released.indicator),
            send: pressed.send.or(released.send),
            effects: [pressed.effects, released.effects.clone()].concat(),
            ..released
        }
    }

    /// Presses a key and keeps it down.
    pub fn down(&mut self, key: Key) -> Output {
        self.press(key, Modifiers::default())
    }

    /// Converts a reading the dictionary does not know and goes past its
    /// candidates — the reading's own forms — into registration.
    pub fn go_past_the_candidates(&mut self) -> Output {
        let mut out = self.key(Key::Space);
        for _ in 0..8 {
            if out.preedit.ends_with(" « ") {
                break;
            }
            out = self.key(Key::Space);
        }
        out
    }

    /// Shift+Delete twice: asks, then forgets the selected candidate. No
    /// release comes between, as a host may send none.
    pub fn forget(&mut self) -> Output {
        self.shifted(Key::Delete);
        self.shifted(Key::Delete)
    }

    pub fn shifted(&mut self, key: Key) -> Output {
        self.press(
            key,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        )
    }

    /// Presses a modifier key alone and lets it go, with its own flag held
    /// while it is down as hosts report it.
    pub fn tap(&mut self, modifier: Key) -> Output {
        let mods = match modifier {
            Key::CtrlLeft | Key::CtrlRight => Modifiers {
                ctrl: true,
                ..Default::default()
            },
            Key::CmdLeft | Key::CmdRight => Modifiers {
                cmd: true,
                ..Default::default()
            },
            Key::AltLeft | Key::AltRight => Modifiers {
                alt: true,
                ..Default::default()
            },
            _ => Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        self.press(modifier, mods);
        self.release(modifier)
    }

    pub fn ch(&mut self, c: char) -> Output {
        let shift = c.is_ascii_uppercase();
        self.press(
            Key::Char(c),
            Modifiers {
                shift,
                ..Default::default()
            },
        )
    }

    /// Types every char; returns the concatenated commits and the last output.
    pub fn typ(&mut self, s: &str) -> (String, Output) {
        let mut commits = String::new();
        let mut last = None;
        for c in s.chars() {
            let out = self.ch(c);
            commits.push_str(out.commit.as_deref().unwrap_or(""));
            last = Some(out);
        }
        (commits, last.expect("at least one char"))
    }

    pub fn ctrl(&mut self, c: char) -> Output {
        self.press(
            Key::Char(c),
            Modifiers {
                ctrl: true,
                ..Default::default()
            },
        )
    }

    pub fn kana(&mut self) {
        self.tap(Key::ShiftRight);
    }

    pub fn converter(&self) -> Ref<'_, Learned> {
        self.learned.borrow()
    }
}

pub fn surfaces(out: &Output) -> Vec<String> {
    out.candidates.as_ref().map_or_else(Vec::new, |v| {
        v.items.iter().map(|c| c.surface.clone()).collect()
    })
}

pub fn typed_with(romaji: RomajiTable, input: &str) -> String {
    let mut t = T::with_config(Config { romaji, ..config() });
    t.kana();
    t.typ(input).0
}

pub fn table(layers: &[&str]) -> RomajiTable {
    let mut table = RomajiTable::empty();
    for layer in layers {
        assert_eq!(table.apply(layer), Vec::<usize>::new(), "{layer}");
    }
    table
}

pub fn reading_kanji() -> T {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    t
}

pub fn plain(key: Key) -> Chord {
    Chord {
        key,
        mods: Modifiers::default(),
    }
}

/// F6 to F10 bound to the forms that select without committing.
pub fn selecting_forms() -> T {
    let mut config = config();
    for (n, action) in [
        (6, Action::Form(Form::Hiragana)),
        (7, Action::Form(Form::Katakana)),
        (8, Action::Form(Form::HalfKatakana)),
        (9, Action::Form(Form::FullAlphanumeric)),
        (10, Action::Form(Form::Alphanumeric)),
    ] {
        for scene in [
            &mut config.bindings.reading,
            &mut config.bindings.candidates,
        ] {
            scene.retain(|b| b.from.key != Key::F(n));
            scene.push(Binding {
                from: plain(Key::F(n)),
                gesture: Gesture::Press,
                to: action,
            });
        }
    }
    T::with_config(config)
}
