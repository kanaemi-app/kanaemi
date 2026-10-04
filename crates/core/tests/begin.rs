mod common;

use common::*;
use kanaemi_core::{Action, Binding, Chord, Gesture, Key, Mode, Modifiers};

#[test]
fn begin_in_kana_mode_with_nothing_typed_starts_a_reading() {
    let mut t = T::new();
    t.kana();
    let out = t.ch(';');
    assert!(out.consumed);
    assert_eq!(out.preedit, "›");
    assert_eq!(t.typ("kanji").1.preedit, "›かんじ");
}

#[test]
fn a_reading_can_start_with_a_digit_or_a_symbol() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ(";1ko").1.preedit, "›１こ");
}

#[test]
fn a_reading_begun_with_a_digit_converts_with_its_number_and_commits_it() {
    let mut t = T::new();
    t.kana();
    t.typ(";1ko");
    assert_eq!(t.key(Key::Space).preedit, "»1個");
    assert_eq!(t.key(Key::Space).preedit, "»１個");
    assert_eq!(t.key(Key::Space).preedit, "»一個");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("一個"));
    assert_eq!(t.converter().okurigana_seen, [None]);
    assert_eq!(
        t.converter().commits,
        [("１こ".to_owned(), "一個".to_owned())]
    );
}

#[test]
fn a_key_bound_to_begin_wins_over_the_romaji_table() {
    let mut t = T::new();
    t.kana();
    let out = t.ch(';');
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›"));
}

#[test]
fn begin_on_an_empty_reading_leaves_it_and_types_the_key_through_the_romaji_table() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    let out = t.ch(';');
    assert!(out.consumed);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("；"), "")
    );
    assert_eq!(out.mode, Mode::Kana);
}

#[test]
fn begin_on_a_reading_emptied_by_backspace_leaves_it_too() {
    let mut t = T::new();
    t.kana();
    t.typ(";k");
    t.key(Key::Backspace);
    assert_eq!(t.ch(';').commit.as_deref(), Some("；"));
}

#[test]
fn begin_inside_a_reading_marks_the_okurigana() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ(";ka;").1.preedit, "›か*");
    assert_eq!(t.typ("ku").1.preedit, "»書く");
}

#[test]
fn begin_turns_pending_romaji_into_kana_of_the_reading_before_marking() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ(";kan;").1.preedit, "›かん*");
    assert_eq!(t.typ("ji").1.preedit, "»感じ");
    assert_eq!(
        t.converter()
            .okurigana_seen
            .last()
            .cloned()
            .flatten()
            .as_deref(),
        Some("じ")
    );
}

#[test]
fn begin_after_the_okurigana_is_marked_does_nothing() {
    let mut t = T::new();
    t.kana();
    t.typ(";ka;");
    let out = t.ch(';');
    assert!(out.consumed);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›か*"));
    assert_eq!(t.typ("t;").1.preedit, "›か*t");
}

#[test]
fn begin_with_the_cursor_not_at_the_end_does_nothing() {
    let mut t = reading_kanji();
    t.key(Key::Left);
    let out = t.ch(';');
    assert!(out.consumed);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›かん|じ"));
}

#[test]
fn begin_on_a_reading_whose_romaji_makes_no_kana_does_nothing() {
    let mut t = T::new();
    t.kana();
    t.typ(";k");
    assert_eq!(t.ch(';').preedit, "›k");
}

#[test]
fn begin_while_choosing_commits_the_candidate_and_starts_the_next_reading() {
    let mut t = reading_kanji();
    t.key(Key::Space);
    let out = t.ch(';');
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("漢字"), "›")
    );
    assert_eq!(t.typ("kisha").1.preedit, "›きしゃ");
    assert_eq!(
        t.converter().commits.last(),
        Some(&("かんじ".to_owned(), "漢字".to_owned()))
    );
}

#[test]
fn romaji_left_over_from_the_okurigana_goes_on_into_the_reading_begin_starts() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tt");
    let out = t.ch(';');
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("持っ"), "›t")
    );
    assert_eq!(t.ch('a').preedit, "›た");
}

#[test]
fn begin_with_only_pending_romaji_commits_it_as_kana_and_starts_a_reading() {
    let mut t = T::new();
    t.kana();
    t.ch('n');
    let out = t.ch(';');
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("ん"), "›")
    );
}

#[test]
fn begin_is_not_bound_in_abc_mode() {
    let mut t = T::new();
    let out = t.ch(';');
    assert!(!out.consumed);
    assert_eq!(out.preedit, "");
}

#[test]
fn begin_in_the_text_to_register_starts_a_reading_inside_it() {
    let mut t = T::new();
    t.kana();
    t.typ(";nunu");
    t.go_past_the_candidates();
    assert_eq!(t.typ(";kanji").1.preedit, "»ぬぬ « ›かんじ");
    assert_eq!(t.key(Key::Space).preedit, "»ぬぬ « »漢字");
}

#[test]
fn begin_in_the_text_to_register_commits_its_pending_romaji_first() {
    let mut t = T::new();
    t.kana();
    t.typ(";nunu");
    t.go_past_the_candidates();
    t.ch('n');
    assert_eq!(t.ch(';').preedit, "»ぬぬ « ん›");
}

#[test]
fn the_key_bound_to_begin_types_itself_in_text_to_register_typed_in_abc_mode() {
    let mut t = T::new();
    t.kana();
    t.typ(";nunu");
    t.go_past_the_candidates();
    t.tap(Key::ShiftLeft);
    assert_eq!(t.typ("a;b").1.preedit, "»ぬぬ « a;b");
}

#[test]
fn unbinding_begin_lets_the_key_type_through_the_romaji_table() {
    let mut config = config();
    config.bindings.kana.retain(|b| b.to != Action::Begin);
    let mut t = T::with_config(config);
    t.kana();
    assert_eq!(t.ch(';').commit.as_deref(), Some("；"));
}

#[test]
fn begin_can_be_bound_to_another_key() {
    let mut config = config();
    for scene in [&mut config.bindings.kana, &mut config.bindings.reading] {
        scene.retain(|b| b.to != Action::Begin);
        scene.push(Binding {
            from: plain(Key::CtrlRight),
            gesture: Gesture::Tap,
            to: Action::Begin,
        });
    }
    let mut t = T::with_config(config);
    t.kana();
    assert_eq!(t.ch(';').commit.as_deref(), Some("；"));
    t.tap(Key::CtrlRight);
    t.typ("ka");
    t.tap(Key::CtrlRight);
    assert_eq!(t.typ("ku").1.preedit, "»書く");
}

#[test]
fn a_begin_key_that_types_no_character_only_leaves_an_empty_reading() {
    let mut config = config();
    for scene in [&mut config.bindings.kana, &mut config.bindings.reading] {
        scene.push(Binding {
            from: plain(Key::CtrlRight),
            gesture: Gesture::Tap,
            to: Action::Begin,
        });
    }
    let mut t = T::with_config(config);
    t.kana();
    t.tap(Key::CtrlRight);
    let out = t.tap(Key::CtrlRight);
    assert_eq!((out.commit, out.preedit.as_str()), (None, ""));
    assert_eq!(out.mode, Mode::Kana);
}

fn ctrl_semicolon() -> Chord {
    Chord {
        key: Key::Char(';'),
        mods: Modifiers {
            ctrl: true,
            ..Default::default()
        },
    }
}

#[test]
fn a_begin_shortcut_on_an_empty_reading_in_a_registration_only_leaves_the_reading() {
    let mut config = config();
    for scene in [
        &mut config.bindings.registration,
        &mut config.bindings.reading,
    ] {
        scene.push(Binding {
            from: ctrl_semicolon(),
            gesture: Gesture::Press,
            to: Action::Begin,
        });
    }
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";nunu");
    t.go_past_the_candidates();
    t.typ("ka");
    assert_eq!(t.ctrl(';').preedit, "»ぬぬ « か›");
    let out = t.ctrl(';');
    assert!(out.consumed);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "»ぬぬ « か"));
    assert_eq!(t.typ("ki").1.preedit, "»ぬぬ « かき", "still registering");
}

#[test]
fn esc_bound_to_begin_on_an_empty_reading_only_leaves_the_reading() {
    let mut config = config();
    for scene in [&mut config.bindings.kana, &mut config.bindings.reading] {
        scene.retain(|b| b.from != plain(Key::Esc));
        scene.push(Binding {
            from: plain(Key::Esc),
            gesture: Gesture::Press,
            to: Action::Begin,
        });
    }
    let mut t = T::with_config(config);
    t.kana();
    assert_eq!(t.key(Key::Esc).preedit, "›");
    let out = t.key(Key::Esc);
    assert!(out.consumed);
    assert_eq!((out.commit, out.preedit.as_str()), (None, ""));
    assert_eq!(out.mode, Mode::Kana);
}
