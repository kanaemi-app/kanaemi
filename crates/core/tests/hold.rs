mod common;

use common::*;
use kanaemi_core::{Action, Binding, Config, Event, Gesture, Key, Mode, Modifiers};

/// The test configuration with no key bound to be held.
fn unheld() -> Config {
    let mut config = common::config();
    config.marks.hold = "_".to_owned();
    let bindings = &mut config.bindings;
    for scene in [
        &mut bindings.kana,
        &mut bindings.abc,
        &mut bindings.reading,
        &mut bindings.candidates,
        &mut bindings.registration,
    ] {
        scene.retain(|b| b.gesture != Gesture::Hold);
    }
    config
}

/// Space held begins a reading, marks the okurigana, or begins the next
/// reading, as SKK users press Shift with their thumb.
fn config() -> Config {
    let mut config = unheld();
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
    config
}

fn kana() -> T {
    let mut t = T::with_config(config());
    t.kana();
    t
}

#[test]
fn a_letter_typed_and_let_go_while_space_is_held_begins_a_reading() {
    let mut t = kana();
    let out = t.down(Key::Space);
    assert!(out.consumed);
    assert_eq!(out.commit, None);
    let out = t.ch('k');
    assert!(out.consumed);
    assert_eq!(out.preedit, "_k", "not known yet");
    let out = t.release(Key::Char('k'));
    assert_eq!(out.preedit, "›k");
    let out = t.release(Key::Space);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›k"));
    let (_, out) = t.typ("anji");
    assert_eq!(out.preedit, "›かんじ");
}

#[test]
fn bindings_tell_whether_a_key_is_bound_to_be_held() {
    assert!(config().bindings.hold_a_key());
    assert!(!unheld().bindings.hold_a_key());
}

#[test]
fn a_held_key_shows_its_mark_until_it_is_known_held_or_alone() {
    let mut t = kana();
    // An application that sees no preedit takes a kept key for its own.
    assert_eq!(t.down(Key::Space).preedit, "_");
    assert_eq!(t.ch('k').preedit, "_k");
    assert_eq!(t.release(Key::Char('k')).preedit, "›k");
    assert_eq!(t.release(Key::Space).preedit, "›k");
}

#[test]
fn the_mark_of_a_held_key_follows_the_preedit() {
    let mut t = kana();
    t.typ(";ka");
    assert_eq!(t.down(Key::Space).preedit, "›か_");
    assert_eq!(t.ch('k').preedit, "›か_k");
}

#[test]
fn the_mark_of_a_held_key_is_gone_once_it_is_pressed_alone() {
    let mut t = kana();
    t.down(Key::Space);
    let out = t.release(Key::Space);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some(" "), "")
    );
}

#[test]
fn an_empty_mark_of_a_held_key_is_a_zero_width_space_as_by_default() {
    assert_eq!(Config::default().marks.hold, "");
    let mut config = config();
    config.marks.hold = String::new();
    let mut t = T::with_config(config);
    t.kana();
    // Still a preedit, so the application leaves the kept key to the IME.
    assert_eq!(t.down(Key::Space).preedit, "\u{200B}");
    assert_eq!(t.ch('k').preedit, "\u{200B}k");
    assert_eq!(t.release(Key::Char('k')).preedit, "›k");
}

#[test]
fn the_mark_of_a_held_key_is_set_by_the_configuration() {
    let mut config = config();
    config.marks.hold = "▼".to_owned();
    let mut t = T::with_config(config);
    t.kana();
    assert_eq!(t.down(Key::Space).preedit, "▼");
}

#[test]
fn a_letter_typed_after_space_has_been_held_long_begins_a_reading_at_once() {
    let mut t = kana();
    t.down(Key::Space);
    t.now += 400;
    assert_eq!(t.ch('k').preedit, "›k");
}

#[test]
fn a_syllable_typed_while_space_is_held_begins_the_reading_once() {
    let mut t = kana();
    t.down(Key::Space);
    t.ch('n');
    // Its romaji is not finished: Space acts before the syllable, not each letter.
    assert_eq!(t.ch('a').preedit, "›な");
    t.release(Key::Char('n'));
    t.release(Key::Char('a'));
    t.release(Key::Space);
    t.down(Key::Space);
    t.ch('i');
    t.release(Key::Char('i'));
    let out = t.release(Key::Space);
    assert_eq!(out.preedit, "»ナイ", "converted at the okurigana");
}

#[test]
fn a_held_key_bound_to_another_function_acts_in_a_syllable_too() {
    let mut config = config();
    config.bindings.reading.push(Binding {
        from: plain(Key::Space),
        gesture: Gesture::Hold,
        to: Action::Abc,
    });
    config.bindings.reading.retain(|b| {
        !(b.from == plain(Key::Space) && b.gesture == Gesture::Hold && b.to == Action::Begin)
    });
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";k");
    t.down(Key::Space);
    t.now += 400;
    assert_eq!(
        t.ch('a').mode,
        Mode::Abc,
        "only begin waits for the syllable"
    );
}

#[test]
fn a_syllable_goes_on_alike_whichever_key_is_let_go_first() {
    let mut t = kana();
    t.typ(";n");
    t.down(Key::Space);
    t.ch('a');
    // Let go before Space: held, as when Space stays down past it.
    assert_eq!(t.release(Key::Char('a')).preedit, "›な");
}

#[test]
fn a_syllable_of_three_letters_typed_while_space_is_held_begins_the_reading_once() {
    let mut t = kana();
    t.down(Key::Space);
    for c in ['k', 'y', 'u'] {
        t.ch(c);
    }
    for c in ['k', 'y', 'u'] {
        t.release(Key::Char(c));
    }
    t.release(Key::Space);
    let (_, out) = t.typ("uri");
    assert_eq!(out.preedit, "›きゅうり");
}

#[test]
fn romaji_left_after_kana_is_made_kana_when_space_held_marks_the_okurigana() {
    let mut t = kana();
    t.typ(";kan");
    t.down(Key::Space);
    t.ch('j');
    // As SKK reads kanJI: the okurigana starts, and converts at its kana.
    let out = t.ch('i');
    assert!(out.preedit.starts_with('»'), "{}", out.preedit);
}

#[test]
fn a_syllable_typed_while_space_is_held_after_the_okurigana_starts_goes_on() {
    let mut t = kana();
    t.typ(";ka");
    t.down(Key::Space);
    t.ch('k');
    assert_eq!(t.ch('u').preedit, "»書く");
}

#[test]
fn a_letter_typed_while_space_is_held_in_a_reading_marks_the_okurigana() {
    let mut t = kana();
    t.typ(";ka");
    t.down(Key::Space);
    t.ch('k');
    t.release(Key::Char('k'));
    t.release(Key::Space);
    assert_eq!(t.ch('u').preedit, "»書く");
}

#[test]
fn space_let_go_before_the_letter_is_a_space_and_then_the_letter() {
    let mut t = kana();
    t.typ(";kanji");
    t.down(Key::Space);
    t.ch('a');
    let out = t.release(Key::Space);
    assert_eq!(out.commit.as_deref(), Some("漢字あ"));
    assert_eq!(out.preedit, "");
}

#[test]
fn space_tapped_does_what_it_is_bound_to_as_it_is_let_go() {
    let mut t = kana();
    t.typ(";kanji");
    let out = t.down(Key::Space);
    assert_eq!(out.preedit, "›かんじ_", "not yet");
    assert_eq!(t.release(Key::Space).preedit, "»漢字");
}

#[test]
fn space_tapped_with_nothing_typed_types_a_space() {
    let mut t = kana();
    t.down(Key::Space);
    let out = t.release(Key::Space);
    assert_eq!(out.commit.as_deref(), Some(" "));
}

#[test]
fn space_held_long_and_let_go_alone_does_nothing() {
    let mut t = kana();
    t.typ(";kanji");
    t.down(Key::Space);
    t.now += 400;
    let out = t.release(Key::Space);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›かんじ"));
}

#[test]
fn space_repeating_while_held_is_one_press() {
    let mut t = kana();
    t.typ(";kanji");
    t.down(Key::Space);
    t.down(Key::Space);
    t.down(Key::Space);
    assert_eq!(t.release(Key::Space).preedit, "»漢字");
}

#[test]
fn a_key_that_types_nothing_pressed_while_space_is_held_follows_a_space_tap() {
    let mut t = kana();
    t.typ(";kanji");
    t.down(Key::Space);
    let out = t.down(Key::Enter);
    assert_eq!(out.commit.as_deref(), Some("漢字"));
    let out = t.release(Key::Space);
    assert_eq!((out.commit, out.preedit.as_str()), (None, ""));
}

#[test]
fn a_shortcut_pressed_while_space_is_held_is_not_a_letter() {
    let mut t = kana();
    t.down(Key::Space);
    let out = t.ctrl('j');
    assert_eq!(out.commit.as_deref(), Some(" "));
}

#[test]
fn each_letter_while_space_is_held_does_what_space_held_does() {
    let mut config = unheld();
    config.bindings.abc.push(Binding {
        from: plain(Key::Space),
        gesture: Gesture::Hold,
        to: Action::Kana,
    });
    config.bindings.kana.push(Binding {
        from: plain(Key::Space),
        gesture: Gesture::Hold,
        to: Action::Abc,
    });
    let mut t = T::with_config(config);
    t.down(Key::Space);
    t.now += 400;
    assert_eq!(t.ch('a').commit.as_deref(), Some("あ"), "into kana mode");
    let out = t.ch('a');
    assert_eq!(out.mode, kanaemi_core::Mode::Abc, "back out of it");
    assert!(!out.consumed, "a goes on in ABC mode");
}

#[test]
fn a_second_letter_while_the_first_waits_decides_that_space_is_held() {
    let mut t = kana();
    t.down(Key::Space);
    t.ch('k');
    assert_eq!(t.ch('a').preedit, "›か");
}

#[test]
fn letters_typed_while_space_is_held_are_the_letters_each_after_begin() {
    let mut held = kana();
    held.down(Key::Space);
    held.now += 400;
    held.ch('k');
    let held = held.ch('a');

    let mut typed = kana();
    let (_, typed) = typed.typ(";k;a");

    assert_eq!((held.commit, held.preedit), (typed.commit, typed.preedit));
}

#[test]
fn space_held_types_nothing_of_its_own_in_text_registered_in_abc_mode() {
    let mut config = common::config();
    config.bindings.registration.push(Binding {
        from: plain(Key::Space),
        gesture: Gesture::Hold,
        to: Action::Begin,
    });
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";nunu");
    t.go_past_the_candidates();
    t.tap(Key::ShiftLeft);
    t.down(Key::Space);
    t.now += 400;
    t.ch('a');
    let out = t.ch('b');
    assert_eq!(out.preedit, "»ぬぬ « ab");
}

#[test]
fn a_modifier_pressed_while_a_letter_waits_follows_a_space_tap_and_the_letter() {
    let mut t = kana();
    t.down(Key::Space);
    t.ch('a');
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let out = t.press(Key::ShiftLeft, shift);
    assert_eq!(out.commit.as_deref(), Some(" あ"));
    assert_eq!(t.release(Key::ShiftLeft).mode, kanaemi_core::Mode::Abc);
}

/// `;` held goes to ABC mode; pressed alone, it still begins.
fn semicolon_held() -> T {
    let mut config = common::config();
    config.marks.hold = "_".to_owned();
    config.bindings.kana.push(Binding {
        from: plain(Key::Char(';')),
        gesture: Gesture::Hold,
        to: Action::Abc,
    });
    let mut t = T::with_config(config);
    t.kana();
    t
}

#[test]
fn a_character_key_held_acts_before_what_is_typed() {
    let mut t = semicolon_held();
    t.ch(';');
    t.now += 400;
    let out = t.ch('a');
    assert_eq!(out.mode, kanaemi_core::Mode::Abc);
    assert!(!out.consumed, "a goes on in ABC mode");
}

#[test]
fn a_character_key_pressed_alone_does_what_it_does_as_it_is_let_go() {
    let mut t = semicolon_held();
    assert_eq!(t.ch(';').preedit, "_", "not yet");
    assert_eq!(t.release(Key::Char(';')).preedit, "›");
}

#[test]
fn a_character_key_let_go_before_the_next_types_both() {
    let mut t = semicolon_held();
    t.ch(';');
    t.ch('k');
    t.release(Key::Char(';'));
    assert_eq!(t.ch('a').preedit, "›か");
}

#[test]
fn space_typed_while_a_character_key_is_held_is_a_character_too() {
    let mut config = common::config();
    config.bindings.abc.push(Binding {
        from: plain(Key::Char(';')),
        gesture: Gesture::Hold,
        to: Action::Kana,
    });
    let mut t = T::with_config(config);
    t.ch(';');
    t.now += 400;
    let out = t.down(Key::Space);
    assert_eq!(out.mode, kanaemi_core::Mode::Kana);
}

#[test]
fn a_character_key_tapped_where_it_passes_on_types_its_character() {
    let mut config = common::config();
    config.bindings.abc.push(Binding {
        from: plain(Key::Char(';')),
        gesture: Gesture::Hold,
        to: Action::Kana,
    });
    let mut t = T::with_config(config);
    t.ch(';');
    let out = t.release(Key::Char(';'));
    assert_eq!(out.commit.as_deref(), Some(";"));
}

#[test]
fn a_key_with_no_hold_binding_acts_as_it_is_pressed() {
    let mut t = T::with_config(unheld());
    t.kana();
    t.typ(";kanji");
    assert_eq!(t.down(Key::Space).preedit, "»漢字");
}

#[test]
fn space_with_shift_is_another_key_than_space_held() {
    let mut t = kana();
    t.typ(";kanji");
    let out = t.press(
        Key::Space,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(out.preedit, "»kanji", "the last candidate, not a hold");
}

#[test]
fn a_held_space_is_forgotten_when_the_focus_moves() {
    let mut t = kana();
    t.down(Key::Space);
    t.ch('k');
    t.handle(Event::FocusOut);
    t.handle(Event::FocusIn { password: false });
    let out = t.release(Key::Space);
    assert_eq!((out.commit, out.preedit.as_str()), (None, ""));
}

#[test]
fn a_held_space_is_forgotten_when_the_mode_changes() {
    let mut t = kana();
    t.down(Key::Space);
    t.ch('k');
    t.ch('a');
    // Its release never came: switching the mode is a way out.
    t.handle(Event::SetMode(Mode::Abc));
    t.kana();
    let (commit, out) = t.typ("ka");
    assert_eq!((commit.as_str(), out.preedit.as_str()), ("か", ""));
}

#[test]
fn a_held_space_is_kept_when_the_mode_asked_for_is_the_mode_already() {
    let mut t = kana();
    t.down(Key::Space);
    t.ch('k');
    assert_eq!(t.handle(Event::SetMode(Mode::Kana)).preedit, "_k");
    assert_eq!(t.release(Key::Char('k')).preedit, "›k");
}

#[test]
fn a_letter_waiting_when_the_mode_changes_is_typed_first() {
    let mut t = kana();
    t.down(Key::Space);
    t.ch('a');
    let out = t.handle(Event::SetMode(Mode::Abc));
    assert_eq!(out.commit.as_deref(), Some(" あ"));
}

#[test]
fn a_letter_waiting_when_the_focus_moves_is_typed_after_a_space() {
    let mut t = kana();
    t.down(Key::Space);
    t.ch('a');
    let out = t.handle(Event::FocusOut);
    assert_eq!(out.commit.as_deref(), Some(" あ"));
}

#[test]
fn a_letter_waiting_when_the_preedit_is_flushed_is_typed_there() {
    let mut t = kana();
    t.down(Key::Space);
    t.ch('a');
    let out = t.handle(Event::Flush);
    assert_eq!(out.commit.as_deref(), Some(" あ"));
    let out = t.release(Key::Space);
    assert_eq!((out.commit, out.preedit.as_str()), (None, ""));
}

#[test]
fn a_key_kept_while_space_was_held_types_its_character_rather_than_a_remap() {
    let mut config = config();
    config.bindings.application.push(kanaemi_core::Remap {
        from: plain(Key::Space),
        to: plain(Key::Left),
    });
    let mut t = T::with_config(config);
    t.kana();
    t.down(Key::Space);
    let out = t.release(Key::Space);
    assert_eq!((out.commit.as_deref(), out.send), (Some(" "), None));
}

#[test]
fn a_letter_let_go_with_no_key_held_does_nothing() {
    let mut t = kana();
    let out = t.release(Key::Char('k'));
    assert_eq!((out.commit, out.preedit.as_str()), (None, ""));
}
