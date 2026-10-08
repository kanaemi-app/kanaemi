//! Random sequences of events, across every scene, against what the core
//! promises for any of them. The case count is kept low for `cargo test`;
//! `PROPTEST_CASES` raises it.

mod common;

use common::*;
use kanaemi_core::{
    Action, Binding, Config, Effect, Event, Gesture, Key, KeyEvent, KeyKind, Mode, Modifiers,
    Output,
};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

const CASES: u32 = 64;

/// Shown after the selected candidate while asking whether to forget it.
const ASKING_TO_FORGET: &str = "（候補から除外するには、もう一度同じキーを押してください）";

/// The test configuration, varied in what changes how keys act.
#[derive(Clone, Debug)]
struct Variant {
    /// Space held begins, as in SKK.
    hold_space: bool,
    keep_unfinished_romaji: bool,
    /// Ctrl with an unbound key commits and passes on while typing.
    pass_ctrl: bool,
    mode_indicator: bool,
}

impl Variant {
    fn config(&self) -> Config {
        let mut config = config();
        config.keep_unfinished_romaji = self.keep_unfinished_romaji;
        config.pass_while_composing.ctrl = self.pass_ctrl;
        config.mode_indicator = self.mode_indicator;
        if self.hold_space {
            let hold = Binding {
                from: plain(Key::Space),
                gesture: Gesture::Hold,
                to: Action::Begin,
            };
            let bindings = &mut config.bindings;
            for scene in [
                &mut bindings.kana,
                &mut bindings.reading,
                &mut bindings.candidates,
            ] {
                scene.push(hold);
            }
        }
        config
    }
}

/// One thing a user or a host does, as the events it makes.
#[derive(Clone, Debug)]
enum Step {
    /// A key pressed, and let go unless `release` is false.
    Stroke {
        key: Key,
        mods: Modifiers,
        release: bool,
    },
    Repeat(Key, Modifiers),
    Release(Key),
    /// A modifier key pressed alone and let go after `ms`.
    Tap(Key, u64),
    /// Keys typed one after another, as the romaji of a word.
    Type(&'static str),
    Wait(u64),
    Event(Event),
}

fn modifiers() -> impl Strategy<Value = Modifiers> {
    prop_oneof![
        6 => Just(Modifiers::default()),
        2 => Just(Modifiers { shift: true, ..Default::default() }),
        2 => Just(Modifiers { ctrl: true, ..Default::default() }),
        1 => Just(Modifiers { cmd: true, ..Default::default() }),
        1 => Just(Modifiers { alt: true, ..Default::default() }),
    ]
}

fn character() -> impl Strategy<Value = char> {
    prop_oneof![
        12 => proptest::sample::select("aiueokstnhmyrwgzdbpjfcvxl".chars().collect::<Vec<_>>()),
        3 => Just(';'),
        2 => proptest::sample::select("0123456789-,.'/".chars().collect::<Vec<_>>()),
        1 => proptest::sample::select("KSAT".chars().collect::<Vec<_>>()),
    ]
}

fn named_key() -> impl Strategy<Value = Key> {
    proptest::sample::select(vec![
        Key::Space,
        Key::Enter,
        Key::Esc,
        Key::Backspace,
        Key::Delete,
        Key::Left,
        Key::Right,
        Key::Up,
        Key::Down,
        Key::Home,
        Key::End,
        Key::F(6),
        Key::F(7),
        Key::F(8),
        Key::F(9),
        Key::F(10),
        Key::F(1),
        Key::Eisu,
        Key::Kana,
        Key::Henkan,
        Key::Muhenkan,
        Key::Other,
        Key::Modifier,
    ])
}

fn modifier_key() -> impl Strategy<Value = Key> {
    proptest::sample::select(vec![
        Key::ShiftLeft,
        Key::ShiftRight,
        Key::CtrlLeft,
        Key::CtrlRight,
        Key::CmdLeft,
        Key::CmdRight,
        Key::AltLeft,
        Key::AltRight,
    ])
}

fn any_key() -> impl Strategy<Value = Key> {
    prop_oneof![character().prop_map(Key::Char), named_key(), modifier_key()]
}

/// Words the converter knows, and readings with okurigana, so that
/// conversions, paging and registrations come up often.
const WORDS: &[&str] = &[
    ";kanji", ";kisha", ";kou", ";ka;ku", ";ta;be", ";mo;tta", ";i;tt", ";1ko", ";kana", "kanji",
];

fn event() -> impl Strategy<Value = Event> {
    prop_oneof![
        any::<bool>().prop_map(|password| Event::FocusIn { password }),
        Just(Event::FocusOut),
        Just(Event::Flush),
        (0usize..10).prop_map(Event::Select),
        Just(Event::SetMode(Mode::Kana)),
        Just(Event::SetMode(Mode::Abc)),
        any::<bool>().prop_map(Event::Erased),
        Just(Event::CaretMoved),
    ]
}

fn step() -> impl Strategy<Value = Step> {
    let stroke =
        |key: BoxedStrategy<Key>, mods: BoxedStrategy<Modifiers>| {
            (key, mods, proptest::bool::weighted(0.9))
                .prop_map(|(key, mods, release)| Step::Stroke { key, mods, release })
        };
    prop_oneof![
        20 => stroke(character().prop_map(Key::Char).boxed(), Just(Modifiers::default()).boxed()),
        8 => stroke(named_key().boxed(), modifiers().boxed()),
        3 => stroke(character().prop_map(Key::Char).boxed(), modifiers().boxed()),
        4 => (modifier_key(), 0u64..500).prop_map(|(key, ms)| Step::Tap(key, ms)),
        3 => proptest::sample::select(WORDS).prop_map(Step::Type),
        // Undoing a commit, so that the host is often asked to erase.
        2 => Just(Step::Stroke {
            key: Key::Backspace,
            mods: Modifiers { shift: true, ..Default::default() },
            release: true,
        }),
        2 => (any_key(), modifiers()).prop_map(|(key, mods)| Step::Repeat(key, mods)),
        2 => any_key().prop_map(Step::Release),
        1 => (0u64..1_000).prop_map(Step::Wait),
        3 => event().prop_map(Step::Event),
    ]
}

fn variant() -> impl Strategy<Value = Variant> {
    (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
        |(hold_space, keep_unfinished_romaji, pass_ctrl, mode_indicator)| Variant {
            hold_space,
            keep_unfinished_romaji,
            pass_ctrl,
            mode_indicator,
        },
    )
}

/// The flag a modifier key sets while it is down.
fn own_flag(key: Key) -> Modifiers {
    Modifiers {
        shift: matches!(key, Key::ShiftLeft | Key::ShiftRight),
        ctrl: matches!(key, Key::CtrlLeft | Key::CtrlRight),
        cmd: matches!(key, Key::CmdLeft | Key::CmdRight),
        alt: matches!(key, Key::AltLeft | Key::AltRight),
    }
}

fn is_modifier_key(key: Key) -> bool {
    own_flag(key) != Modifiers::default()
}

/// The events a step makes, each with the milliseconds since the last.
fn events(step: &Step) -> Vec<(u64, Event)> {
    let key = |key, mods, kind| {
        Event::Key(KeyEvent {
            key,
            mods,
            kind,
            time_ms: 0,
        })
    };
    let typed = |c: char| Modifiers {
        shift: c.is_ascii_uppercase(),
        ..Default::default()
    };
    match *step {
        Step::Stroke {
            key: k,
            mods,
            release,
        } => {
            let mods = match k {
                Key::Char(c) if c.is_ascii_uppercase() => Modifiers {
                    shift: true,
                    ..mods
                },
                _ => mods,
            };
            let mut events = vec![(10, key(k, mods, KeyKind::Press))];
            if release {
                events.push((10, key(k, Modifiers::default(), KeyKind::Release)));
            }
            events
        }
        Step::Repeat(k, mods) => vec![(10, key(k, mods, KeyKind::Repeat))],
        Step::Release(k) => vec![(10, key(k, Modifiers::default(), KeyKind::Release))],
        Step::Tap(k, ms) => vec![
            (10, key(k, own_flag(k), KeyKind::Press)),
            (ms, key(k, Modifiers::default(), KeyKind::Release)),
        ],
        Step::Type(word) => word
            .chars()
            .flat_map(|c| {
                [
                    (10, key(Key::Char(c), typed(c), KeyKind::Press)),
                    (
                        10,
                        key(Key::Char(c), Modifiers::default(), KeyKind::Release),
                    ),
                ]
            })
            .collect(),
        Step::Wait(_) => Vec::new(),
        Step::Event(event) => vec![(0, event)],
    }
}

/// What the host knows between events.
struct Host {
    variant: Variant,
    last: Output,
    password: bool,
    /// The core asked for text to be erased and has not heard back.
    erasing: bool,
}

/// The preedit up to the mark of a key not yet known held or pressed alone.
fn without_hold(preedit: &str) -> &str {
    preedit.split('\u{200B}').next().unwrap_or_default()
}

/// A reading, candidates or a registration is shown: the preedit starts
/// with the mark of one.
fn word_shown(preedit: &str) -> bool {
    preedit.starts_with('›') || preedit.starts_with('»')
}

/// Keys that edit the preedit, kept from the application while a word is typed.
fn named(key: Key) -> bool {
    matches!(
        key,
        Key::Space
            | Key::Enter
            | Key::Esc
            | Key::Backspace
            | Key::Delete
            | Key::Left
            | Key::Right
            | Key::Up
            | Key::Down
            | Key::Home
            | Key::End
    )
}

/// What committing the preedit visible before `event` must commit, when it
/// can be told from the preedit: the reading or the candidate without its
/// marks, or for a registration its first reading. `None` when the preedit
/// holds romaji not yet resolved or letters, which commit by the romaji
/// table's rules.
fn visible(preedit: &str) -> Option<String> {
    if preedit.contains('\u{200B}') || preedit.contains(ASKING_TO_FORGET) {
        return None;
    }
    if let Some(registration) = preedit.find(" « ") {
        return Some(preedit[..registration].replace(['»', '*'], ""));
    }
    if preedit.chars().any(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(preedit.replace(['›', '»', '*', '|'], ""))
}

impl Host {
    fn check(&mut self, event: Event, out: &Output) -> Result<(), TestCaseError> {
        let before = &self.last;
        let was_erasing = self.erasing;
        self.erasing = out.erase.is_some()
            || (self.erasing
                && !matches!(
                    event,
                    Event::Erased(_) | Event::FocusIn { .. } | Event::FocusOut | Event::Flush
                ));
        if let Event::FocusIn { password } = event {
            self.password = password;
        }

        prop_assert_eq!(
            out.cursor,
            out.preedit.chars().count(),
            "the cursor is at the end"
        );
        if let Some(commit) = &out.commit {
            prop_assert!(!commit.is_empty());
            prop_assert!(
                out.effects.contains(&Effect::Typed(commit.clone())),
                "a commit is reported as typed"
            );
        }
        if let Some(view) = &out.candidates {
            prop_assert!(
                (1..=9).contains(&view.items.len()),
                "a page shows 9 at most"
            );
            prop_assert!(view.selected < view.items.len());
            let selected = format!("»{}", view.items[view.selected].surface);
            prop_assert!(
                out.preedit.contains(&selected),
                "the selected candidate is the one shown in the preedit"
            );
        }
        let expected_indicator = match event {
            Event::SetMode(_) => None,
            _ => (self.variant.mode_indicator && out.mode != before.mode).then_some(out.mode),
        };
        prop_assert_eq!(
            out.indicator,
            expected_indicator,
            "shown only as the mode changes"
        );
        if self.password {
            prop_assert_eq!(out.mode, Mode::Abc, "a password field stays in ABC mode");
        }
        if out.mode == Mode::Abc {
            let preedit = without_hold(&out.preedit);
            prop_assert!(
                preedit.is_empty() || preedit.contains(" « "),
                "only the text to register is typed in ABC mode"
            );
        }
        if let Some(send) = out.send {
            prop_assert!(out.consumed, "{:?} is sent in place of the key", send);
            // A key held and so pressed alone acts first, and may commit.
            if !before.preedit.contains('\u{200B}') {
                prop_assert_eq!(&before.preedit, "", "keys are sent with nothing typed");
            }
            prop_assert_eq!(&out.preedit, "");
        }

        match event {
            Event::FocusIn { .. } => {
                prop_assert_eq!(out.mode, Mode::Abc, "a field starts in ABC mode");
                prop_assert_eq!(&out.preedit, "");
                prop_assert!(out.candidates.is_none());
                prop_assert!(out.effects.contains(&Effect::FocusMoved));
            }
            Event::FocusOut | Event::Flush => {
                if event == Event::FocusOut {
                    prop_assert_eq!(&out.preedit, "", "leaving commits what is visible");
                } else {
                    prop_assert_eq!(
                        without_hold(&out.preedit),
                        "",
                        "a flush commits what is visible"
                    );
                }
                prop_assert!(out.candidates.is_none());
                prop_assert!(!out.consumed);
                if let Some(visible) = visible(&before.preedit) {
                    let commit = out.commit.clone().unwrap_or_default();
                    if before.preedit.contains(" « ") {
                        prop_assert!(
                            commit.starts_with(&visible),
                            "a registration commits its first reading: {:?} from {:?}",
                            commit,
                            before.preedit
                        );
                    } else {
                        prop_assert_eq!(commit, visible, "from {:?}", before.preedit);
                    }
                }
            }
            Event::Key(key) if !was_erasing => self.check_key(key, out)?,
            Event::Key(_) => prop_assert!(out.consumed, "a key waits while the host erases"),
            _ => {}
        }
        self.last = out.clone();
        Ok(())
    }

    fn check_key(&self, key: KeyEvent, out: &Output) -> Result<(), TestCaseError> {
        let before = &self.last;
        let shortcut = key.mods.ctrl || key.mods.cmd || key.mods.alt;
        let press = key.kind == KeyKind::Press;
        if is_modifier_key(key.key) {
            prop_assert!(
                !out.consumed,
                "a modifier key always reaches the application"
            );
            return Ok(());
        }
        if !press {
            return Ok(());
        }
        let held = before.preedit.contains('\u{200B}');
        if word_shown(&before.preedit) && !held && named(key.key) && !shortcut {
            prop_assert!(
                out.consumed,
                "{:?} is kept from the application while {:?} is typed",
                key.key,
                before.preedit
            );
        }
        if before.preedit.is_empty()
            && before.mode == Mode::Kana
            && key.key == Key::Esc
            && !shortcut
        {
            prop_assert!(!out.consumed, "Esc with nothing typed passes on");
            prop_assert_eq!(out.mode, Mode::Abc, "and goes to ABC mode");
        }
        if before.preedit.is_empty()
            && before.mode == Mode::Abc
            && matches!(key.key, Key::Char(_))
            && !shortcut
        {
            prop_assert!(!out.consumed, "ABC mode types letters as they are");
            prop_assert_eq!(&out.commit, &None);
            prop_assert_eq!(&out.preedit, "");
        }
        if !out.consumed && !held && !before.preedit.contains(ASKING_TO_FORGET) {
            prop_assert!(
                out.preedit.is_empty() || out.preedit == before.preedit,
                "a key passed on leaves the preedit as it was, or commits it: {:?} to {:?}",
                before.preedit,
                out.preedit
            );
        }
        Ok(())
    }
}

fn run(variant: Variant, steps: Vec<Step>) -> Result<(), TestCaseError> {
    let mut t = T::with_config(variant.config());
    let mut now = t.now;
    let mut host = Host {
        last: t.handle(Event::Flush),
        variant,
        password: false,
        erasing: false,
    };
    for step in &steps {
        if let Step::Wait(ms) = step {
            now += ms;
        }
        for (after, mut event) in events(step) {
            now += after;
            if let Event::Key(key) = &mut event {
                key.time_ms = now;
            }
            let out = t.handle(event);
            host.check(event, &out)?;
        }
    }
    Ok(())
}

fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(CASES)
}

proptest! {
    // A failure prints its shrunk sequence, to be kept as a test of its own.
    #![proptest_config(ProptestConfig {
        cases: cases(),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn any_sequence_of_events_keeps_the_promises(
        variant in variant(),
        steps in proptest::collection::vec(step(), 0..150),
    ) {
        run(variant, steps)?;
    }
}
