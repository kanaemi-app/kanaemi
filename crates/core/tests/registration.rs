mod common;

use common::*;
use kanaemi_core::{Action, Binding, Chord, Event, Gesture, Key, Modifiers};

#[test]
fn registration_collects_what_is_typed_and_registers_on_enter() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    t.go_past_the_candidates();
    t.ch(';');
    t.typ("kanji");
    t.key(Key::Space);
    t.key(Key::Enter);
    let out = t.typ("no").1;
    assert_eq!(out.preedit, "»ぬぬ « 漢字の");
    let out = t.key(Key::Enter);
    assert_eq!(out.commit.as_deref(), Some("漢字の"));
    assert_eq!(
        t.converter().registered,
        [("ぬぬ".to_owned(), "漢字の".to_owned())]
    );
}

#[test]
fn a_registered_word_enters_the_field_as_the_converter_writes_it() {
    let mut t = T::new();
    t.kana();
    t.typ(";2ko");
    t.go_past_the_candidates();
    t.tap(Key::ShiftLeft);
    t.typ("{}");
    let out = t.key(Key::Enter);
    assert_eq!(out.commit.as_deref(), Some("2"));
    assert_eq!(
        t.converter().registered,
        [("２こ".to_owned(), "{}".to_owned())]
    );
    assert_eq!(t.converter().commits, [("２こ".to_owned(), "2".to_owned())]);
}

#[test]
fn empty_enter_or_escape_returns_to_the_reading() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    t.go_past_the_candidates();
    assert_eq!(t.key(Key::Enter).preedit, "›ぬぬ");
    t.key(Key::Space);
    t.key(Key::Space);
    assert_eq!(t.key(Key::Esc).preedit, "›ぬぬ");
    assert!(t.converter().registered.is_empty());
}

#[test]
fn backspace_deletes_from_the_text_being_registered() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    t.go_past_the_candidates();
    t.typ("aiu");
    assert_eq!(t.key(Key::Backspace).preedit, "»ぬぬ « あい");
}

#[test]
fn abc_mode_inside_registration_adds_letters_to_the_text() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    t.go_past_the_candidates();
    t.tap(Key::ShiftLeft);
    let out = t.typ("vim").1;
    assert_eq!(out.preedit, "»ぬぬ « vim");
    assert!(out.consumed);
}

#[test]
fn registration_can_nest() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    t.go_past_the_candidates();
    t.ch(';');
    t.typ("nene");
    let out = t.go_past_the_candidates();
    assert_eq!(out.preedit, "»ぬぬ « »ねね « ");
    t.typ("ne");
    let out = t.key(Key::Enter);
    assert_eq!(out.preedit, "»ぬぬ « ね");
    assert_eq!(
        t.converter().registered,
        [("ねね".to_owned(), "ね".to_owned())]
    );
}

#[test]
fn registration_nests_up_to_three_levels() {
    let mut t = T::new();
    t.kana();
    for reading in ["nunu", "nene", "nono"] {
        t.ch(';');
        t.typ(reading);
        t.go_past_the_candidates();
    }
    t.ch(';');
    t.typ("mumu");
    for _ in 0..4 {
        t.key(Key::Space);
    }
    let out = t.key(Key::Space);
    assert_eq!(
        out.preedit, "»ぬぬ « »ねね « »のの « »ムム",
        "the third level wraps instead of nesting"
    );
}

#[test]
fn inside_registration_the_inner_state_handles_enter_and_escape_first() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    t.go_past_the_candidates();
    t.ch(';');
    t.typ("kanji");
    let out = t.key(Key::Esc);
    assert_eq!(
        out.preedit, "»ぬぬ « ",
        "escape cancels the inner reading, not the registration"
    );
    t.ch(';');
    t.typ("kanji");
    let out = t.key(Key::Enter);
    assert_eq!(
        out.preedit, "»ぬぬ « かんじ",
        "enter commits the inner reading into the text"
    );
}

#[test]
fn registering_an_okurigana_reading_completes_the_okurigana() {
    let mut t = T::new();
    t.kana();
    t.typ(";nu;nu");
    t.go_past_the_candidates();
    t.tap(Key::ShiftLeft);
    t.typ("x");
    let out = t.key(Key::Enter);
    assert_eq!(out.commit.as_deref(), Some("xぬ"));
    assert_eq!(
        t.converter().registered,
        [("ぬ*ぬ".to_owned(), "xぬ".to_owned())]
    );
}

/// Registers `x` for what `typing` leaves, after `then` is pressed.
fn register_x_after(typing: &str, then: &[Key]) -> (Option<String>, T) {
    let mut t = T::new();
    t.kana();
    t.typ(typing);
    for key in then {
        t.key(*key);
    }
    t.typ("tta");
    t.go_past_the_candidates();
    t.tap(Key::ShiftLeft);
    t.typ("x");
    let commit = t.key(Key::Enter).commit;
    (commit, t)
}

#[test]
fn an_okurigana_grown_past_its_first_chunk_is_registered_with_that_chunk() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tta");
    t.go_past_the_candidates();
    t.tap(Key::ShiftLeft);
    t.typ("x");
    let out = t.key(Key::Enter);
    assert_eq!(out.commit.as_deref(), Some("xった"));
    let learned = t.converter();
    assert_eq!(
        learned.registered,
        [("も*った".to_owned(), "xった".to_owned())]
    );
    assert_eq!(learned.registered_heads, [Some("っ".to_owned())]);
}

#[test]
fn the_first_chunk_of_an_okurigana_is_what_is_left_of_it() {
    let (commit, t) = register_x_after(";ka;kya", &[Key::Esc]);
    assert_eq!(commit.as_deref(), Some("xきゃった"));
    assert_eq!(t.converter().registered_heads, [Some("きゃ".to_owned())]);

    let (commit, t) = register_x_after(";ka;kya", &[Key::Esc, Key::Backspace]);
    assert_eq!(commit.as_deref(), Some("xきった"));
    assert_eq!(t.converter().registered_heads, [Some("き".to_owned())]);
}

#[test]
fn the_text_to_register_has_a_cursor_too() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    t.go_past_the_candidates();
    t.typ("kaji");
    let out = t.key(Key::Left);
    assert!(out.consumed);
    assert_eq!(out.preedit, "»ぬぬ « か|じ");
    assert_eq!(t.typ("na").1.preedit, "»ぬぬ « かな|じ");
    assert_eq!(t.key(Key::Delete).preedit, "»ぬぬ « かな");
    assert_eq!(t.key(Key::Home).preedit, "»ぬぬ « |かな");
    assert_eq!(
        t.key(Key::Backspace).preedit,
        "»ぬぬ « |かな",
        "nothing before the cursor"
    );
    t.key(Key::End);
    t.key(Key::Enter);
    assert_eq!(
        t.converter().registered,
        [("ぬぬ".to_owned(), "かな".to_owned())]
    );
}

#[test]
fn the_text_to_register_has_a_cursor_in_abc_mode_too() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    t.go_past_the_candidates();
    t.tap(Key::ShiftLeft);
    t.typ("vm");
    t.key(Key::Left);
    assert_eq!(t.typ("i").1.preedit, "»ぬぬ « vi|m");
}

#[test]
fn unfinished_romaji_waits_at_the_cursor_of_the_text_to_register() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    t.go_past_the_candidates();
    t.typ("kaji");
    t.key(Key::Left);
    assert_eq!(t.ch('k').preedit, "»ぬぬ « かk|じ");
}

#[test]
fn delete_with_unfinished_romaji_and_nothing_to_register_commits_it_and_passes_on() {
    let mut t = T::new();
    t.kana();
    t.ch('n');
    let out = t.key(Key::Delete);
    assert!(!out.consumed);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("ん"), "")
    );
}

#[test]
fn up_and_down_in_the_text_to_register_keep_unfinished_romaji() {
    let mut t = T::new();
    t.kana();
    t.typ(";nunu");
    t.go_past_the_candidates();
    t.ch('k');
    assert!(t.key(Key::Down).consumed);
    assert_eq!(t.ch('a').preedit, "»ぬぬ « か");
}

#[test]
fn keys_without_a_meaning_pass_on_and_keep_unfinished_romaji_in_the_text_to_register() {
    for key in [Key::F(1), Key::Other] {
        let mut t = T::new();
        t.kana();
        t.typ(";nunu");
        t.go_past_the_candidates();
        t.ch('k');
        let out = t.key(key);
        assert!(!out.consumed, "{key:?}");
        assert_eq!(out.commit, None, "{key:?}");
        assert_eq!(t.ch('a').preedit, "»ぬぬ « か", "{key:?}");
    }
}

#[test]
fn zero_in_candidate_mode_registers_the_reading_straight_away() {
    let mut t = reading_kanji();
    t.key(Key::Space);
    let out = t.ch('0');
    assert!(out.consumed);
    assert_eq!(out.preedit, "»かんじ « ");
}

#[test]
fn a_reading_has_no_registration_key_by_default() {
    let mut t = reading_kanji();
    assert_eq!(t.ctrl('r').preedit, "›かんじ");
}

#[test]
fn the_registration_key_can_be_changed() {
    let mut config = config();
    config
        .bindings
        .candidates
        .retain(|b| b.to != Action::Register);
    config.bindings.reading.push(Binding {
        from: Chord {
            key: Key::Char('r'),
            mods: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        },
        gesture: Gesture::Press,
        to: Action::Register,
    });
    let mut t = T::with_config(config.clone());
    t.kana();
    t.typ(";kanji");
    assert_eq!(t.ctrl('r').preedit, "»かんじ « ");

    let mut t = T::with_config(config);
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    assert_eq!(
        t.ch('0').commit.as_deref(),
        Some("漢字０"),
        "0 is typed again"
    );
}

/// Candidates for `;pdf`, a reading of letters that made no kana, chosen
/// again after F9 committed it in full-width letters.
fn choosing_pdf_again() -> T {
    let mut t = T::new();
    t.kana();
    t.typ(";pdf");
    assert_eq!(t.key(Key::F(9)).commit.as_deref(), Some("ｐｄｆ"));
    t.press(
        Key::Backspace,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(t.handle(Event::Erased(true)).preedit, "»ｐｄｆ");
    t
}

#[test]
fn past_the_last_candidate_of_a_reading_without_kana_is_the_first() {
    let mut t = choosing_pdf_again();
    assert_eq!(t.key(Key::Space).preedit, "»pdf");
    assert_eq!(t.key(Key::Space).preedit, "»ｐｄｆ");
    t.typ("a");
    t.key(Key::Enter);
    assert!(t.converter().registered.is_empty());
}

#[test]
fn the_registration_key_does_nothing_for_a_reading_without_kana() {
    let mut t = choosing_pdf_again();
    assert_eq!(t.ch('0').preedit, "»ｐｄｆ");
    assert!(t.converter().registered.is_empty());
}

#[test]
fn a_reading_without_kana_does_not_start_a_registration() {
    let mut config = config();
    config.bindings.reading.push(Binding {
        from: Chord {
            key: Key::Char('r'),
            mods: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        },
        gesture: Gesture::Press,
        to: Action::Register,
    });
    let mut t = T::with_config(config);
    t.kana();
    let typed = t.typ(";pdf").1.preedit;
    assert_eq!(t.ctrl('r').preedit, typed);
    t.typ("a");
    t.key(Key::Enter);
    assert!(t.converter().registered.is_empty());
}
