use kanaemi_config::default_romaji_table;
use kanaemi_core::{Candidate, Config, Converter, Core, Event, Key, KeyEvent, KeyKind, Modifiers};

struct NoWords;

impl Converter for NoWords {
    fn convert(&self, _reading: &str, _okurigana: Option<&str>) -> Vec<Candidate> {
        Vec::new()
    }
}

/// What typing `input` in kana mode with the default table commits.
fn typed(input: &str) -> String {
    let mut core = Core::new(
        NoWords,
        Config {
            romaji: default_romaji_table(),
            ..Config::default()
        },
    );
    core.handle(Event::FocusIn { password: false });
    // A right Shift tap, the default way into kana mode.
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    core.handle(Event::Key(KeyEvent {
        key: Key::ShiftRight,
        mods: shift,
        kind: KeyKind::Press,
        time_ms: 0,
    }));
    core.handle(Event::Key(KeyEvent {
        key: Key::ShiftRight,
        mods: Modifiers::default(),
        kind: KeyKind::Release,
        time_ms: 10,
    }));
    let mut typed = String::new();
    for c in input.chars() {
        let out = core.handle(Event::Key(KeyEvent {
            key: Key::Char(c),
            mods: Modifiers::default(),
            kind: KeyKind::Press,
            time_ms: 0,
        }));
        typed.push_str(out.commit.as_deref().unwrap_or(""));
    }
    typed
}

#[test]
fn the_default_table_types_hepburn_kunrei_and_the_input_aids() {
    let cases = [
        ("kka", "っか"),
        ("nn", "ん"),
        ("nka", "んか"),
        ("n'a", "んあ"),
        ("shi", "し"),
        ("si", "し"),
        ("chi", "ち"),
        ("tsu", "つ"),
        ("xtu", "っ"),
        ("ltu", "っ"),
        ("kya", "きゃ"),
        ("ja", "じゃ"),
        ("fu", "ふ"),
        ("vu", "ゔ"),
        ("dhi", "でぃ"),
        ("wo", "を"),
        ("-", "ー"),
    ];
    for (input, want) in cases {
        assert_eq!(typed(input), want, "romaji {input}");
    }
}

#[test]
fn the_default_table_types_other_ascii_full_width() {
    assert_eq!(typed("1@"), "１＠");
}

#[test]
fn the_default_table_types_japanese_punctuation_over_the_full_width_symbols() {
    assert_eq!(typed(",."), "、。");
}

#[test]
fn the_default_table_types_z_symbols() {
    assert_eq!(typed("zhz."), "←…");
}
