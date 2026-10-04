mod common;

use common::*;
use kanaemi_core::{Config, Event, Key, KeyEvent, KeyKind, Mode, Modifiers};

#[test]
fn the_same_events_give_the_same_outputs() {
    let run = || {
        let mut t = T::new();
        t.kana();
        t.ch(';');
        let mut outs = vec![t.typ("kanji").1];
        outs.push(t.key(Key::Space));
        outs.push(t.key(Key::Space));
        outs.push(t.key(Key::Enter));
        outs
    };
    assert_eq!(run(), run());
}

#[test]
fn committing_what_is_visible_drops_marks_and_pending_romaji() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kan");
    let out = t.handle(Event::Flush);
    assert_eq!(out.commit.as_deref(), Some("かん"));
    assert_eq!(out.mode, Mode::Kana);

    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kak");
    assert_eq!(t.handle(Event::Flush).commit.as_deref(), Some("か"));

    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    t.key(Key::Space);
    assert_eq!(t.handle(Event::Flush).commit.as_deref(), Some("漢字"));
}

#[test]
fn committing_what_is_visible_during_registration_commits_the_first_reading() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    t.go_past_the_candidates();
    t.typ("a");
    let out = t.handle(Event::FocusOut);
    assert_eq!(out.commit.as_deref(), Some("ぬぬ"));
    assert_eq!(out.preedit, "");
}

#[test]
fn abc_mode_passes_characters_to_the_application() {
    let mut t = T::new();
    let out = t.ch('a');
    assert!(!out.consumed);
    assert_eq!(out.mode, Mode::Abc);
}

#[test]
fn left_shift_commits_the_kana_and_returns_to_abc_mode() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    let out = t.tap(Key::ShiftLeft);
    assert_eq!(out.commit.as_deref(), Some("かんじ"));
    assert_eq!(out.mode, Mode::Abc);
    assert_eq!(out.indicator, Some(Mode::Abc));
}

#[test]
fn kana_mode_commits_kana_as_it_is_typed() {
    let mut t = T::new();
    t.kana();
    let (commits, out) = t.typ("watashiha");
    assert_eq!(commits, "わたしは");
    assert_eq!(out.preedit, "");
}

#[test]
fn pending_romaji_is_shown_until_it_completes() {
    let mut t = T::new();
    t.kana();
    let out = t.ch('k');
    assert_eq!(out.preedit, "k");
    assert_eq!(t.ch('a').commit.as_deref(), Some("か"));
}

#[test]
fn an_uppercase_letter_in_kana_mode_is_typed_through_the_romaji_table_like_any_other() {
    let mut t = T::new();
    t.kana();
    let (commits, out) = t.typ("Kanji");
    assert_eq!((commits.as_str(), out.preedit.as_str()), ("Kあんじ", ""));
}

#[test]
fn right_shift_in_abc_mode_only_enters_kana_mode() {
    let mut t = T::new();
    let out = t.tap(Key::ShiftRight);
    assert_eq!(out.mode, Mode::Kana);
    assert_eq!(out.indicator, Some(Mode::Kana));
    assert_eq!(out.preedit, "");
    assert!(!out.consumed, "the shift key itself is never consumed");
}

#[test]
fn right_shift_in_kana_mode_does_nothing_by_default() {
    let mut t = T::new();
    t.kana();
    let out = t.tap(Key::ShiftRight);
    assert_eq!((out.mode, out.preedit.as_str()), (Mode::Kana, ""));
    assert_eq!(out.indicator, None);
}

#[test]
fn escape_without_preedit_passes_through_and_goes_direct() {
    let mut t = T::new();
    t.kana();
    let out = t.key(Key::Esc);
    assert!(!out.consumed);
    assert_eq!(out.mode, Mode::Abc);
}

#[test]
fn cmd_and_option_keys_are_ignored_while_typing() {
    for mods in [
        Modifiers {
            cmd: true,
            ..Default::default()
        },
        Modifiers {
            alt: true,
            ..Default::default()
        },
    ] {
        let mut t = T::new();
        t.kana();
        t.ch(';');
        t.typ("kanji");
        let out = t.press(Key::Char('z'), mods);
        assert!(out.consumed, "{mods:?}");
        assert_eq!(
            (out.commit, out.preedit.as_str()),
            (None, "›かんじ"),
            "{mods:?}"
        );
    }
}

#[test]
fn cmd_keys_pass_on_when_nothing_is_being_typed() {
    let mut t = T::new();
    t.kana();
    let out = t.press(
        Key::Char('z'),
        Modifiers {
            cmd: true,
            ..Default::default()
        },
    );
    assert!(!out.consumed);
}

#[test]
fn jis_eisu_and_kana_keys_act_like_the_shift_taps() {
    let mut t = T::new();
    let out = t.key(Key::Kana);
    assert_eq!((out.mode, out.preedit.as_str()), (Mode::Kana, ""));
    assert_eq!(
        t.key(Key::Kana).preedit,
        "",
        "a second かな key does nothing either"
    );
    t.ch(';');
    t.typ("kanji");
    let out = t.key(Key::Eisu);
    assert_eq!(out.commit.as_deref(), Some("かんじ"));
    assert_eq!(out.mode, Mode::Abc);
}

#[test]
fn focus_in_starts_in_abc_mode_and_drops_the_history() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    let resets = t.converter().resets;
    let out = t.handle(Event::FocusIn { password: false });
    assert_eq!(out.mode, Mode::Abc);
    assert_eq!(out.preedit, "");
    assert_eq!(t.converter().resets, resets + 1);
}

#[test]
fn focus_out_commits_what_is_visible() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    let out = t.handle(Event::FocusOut);
    assert_eq!(out.commit.as_deref(), Some("かんじ"));
}

#[test]
fn a_password_field_never_enters_kana_mode() {
    let mut t = T::new();
    t.handle(Event::FocusIn { password: true });
    assert_eq!(t.tap(Key::ShiftRight).mode, Mode::Abc);
    assert_eq!(t.key(Key::Kana).mode, Mode::Abc);
}

#[test]
fn a_flush_request_commits_and_keeps_the_mode() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    let out = t.handle(Event::Flush);
    assert_eq!(out.commit.as_deref(), Some("かんじ"));
    assert_eq!(out.mode, Mode::Kana);
}

#[test]
fn shift_with_another_key_is_not_a_tap() {
    let mut t = T::new();
    t.press(
        Key::ShiftRight,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    t.ch('A');
    let out = t.release(Key::ShiftRight);
    assert_eq!(out.mode, Mode::Abc);
}

#[test]
fn a_long_shift_press_is_not_a_tap() {
    let mut t = T::new();
    t.press(
        Key::ShiftRight,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    t.now += 400;
    let out = t.release(Key::ShiftRight);
    assert_eq!(out.mode, Mode::Abc);
}

#[test]
fn left_and_right_shift_are_tracked_separately() {
    let mut t = T::new();
    t.press(
        Key::ShiftLeft,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    let out = t.release(Key::ShiftRight);
    assert_eq!(out.mode, Mode::Abc);
}

#[test]
fn the_marks_can_be_changed() {
    let marks = kanaemi_core::Marks {
        reading: "R".to_owned(),
        candidate: "C".to_owned(),
        okurigana: "*".to_owned(),
        registration: "|".to_owned(),
        cursor: "_".to_owned(),
    };
    let mut t = T::with_config(Config { marks, ..config() });
    t.kana();
    assert_eq!(t.typ(";ka;k").1.preedit, "Rか*k");
    assert_eq!(t.typ("u").1.preedit, "C書く");
    for _ in 0..4 {
        t.key(Key::Space);
    }
    assert_eq!(t.key(Key::Space).preedit, "Cか*く|");
}

#[test]
fn the_cursor_is_at_the_end_of_the_preedit() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    let out = t.typ("kanji").1;
    assert_eq!(out.cursor, out.preedit.chars().count());
}

#[test]
fn the_mode_indicator_is_set_only_when_the_mode_changes() {
    let mut t = T::new();
    assert_eq!(t.tap(Key::ShiftRight).indicator, Some(Mode::Kana));
    assert_eq!(t.typ("ka").1.indicator, None);
}

#[test]
fn the_mode_indicator_can_be_turned_off() {
    let mut t = T::with_config(Config {
        mode_indicator: false,
        ..config()
    });
    let out = t.tap(Key::ShiftRight);
    assert_eq!(out.mode, Mode::Kana);
    assert_eq!(out.indicator, None);
}

#[test]
fn random_input_never_panics() {
    let keys = [
        Key::Char('a'),
        Key::Char('k'),
        Key::Char('K'),
        Key::Char(';'),
        Key::Char('n'),
        Key::Char('1'),
        Key::Space,
        Key::Enter,
        Key::Esc,
        Key::Backspace,
        Key::Delete,
        Key::F(6),
        Key::F(7),
        Key::Kana,
        Key::Eisu,
        Key::ShiftLeft,
        Key::ShiftRight,
    ];
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut t = T::new();
    for _ in 0..20_000 {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let pick = (seed >> 33) as usize;
        let event = match pick % 23 {
            16 => Event::FocusIn {
                password: pick.is_multiple_of(2),
            },
            17 => Event::FocusOut,
            18 => Event::Flush,
            19 => Event::Select(pick % 5),
            _ => {
                let key = keys[pick % keys.len()];
                let kind = if pick.is_multiple_of(3) {
                    KeyKind::Release
                } else {
                    KeyKind::Press
                };
                let mods = Modifiers {
                    shift: pick.is_multiple_of(4),
                    ..Default::default()
                };
                t.now += (pick % 400) as u64;
                Event::Key(KeyEvent {
                    key,
                    mods,
                    kind,
                    time_ms: t.now,
                })
            }
        };
        t.handle(event);
    }
}

#[test]
fn another_key_while_shift_is_down_is_not_a_shift_tap() {
    let mut t = T::new();
    t.press(
        Key::ShiftRight,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    t.shifted(Key::Other);
    let out = t.release(Key::ShiftRight);
    assert_eq!((out.mode, out.preedit.as_str()), (Mode::Abc, ""));
}

#[test]
fn every_text_committed_to_the_field_is_told_to_the_converter() {
    let mut t = T::new();
    t.kana();
    t.typ("a");
    t.typ(";kanji");
    t.key(Key::Space);
    t.key(Key::Enter);
    assert_eq!(t.converter().texts, ["あ", "漢字"]);
}

#[test]
fn text_typed_into_a_registration_is_not_committed_to_the_field() {
    let mut t = T::new();
    t.kana();
    t.typ(";zzz");
    t.go_past_the_candidates();
    t.typ("a");
    assert_eq!(t.converter().texts, Vec::<String>::new());
}

#[test]
fn a_shift_tapped_while_the_other_shift_is_held_is_not_a_tap() {
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let mut t = T::new();
    t.press(Key::ShiftLeft, shift);
    t.press(Key::ShiftRight, shift);
    assert_eq!(t.release(Key::ShiftRight).mode, Mode::Abc);

    let mut t = T::new();
    t.kana();
    t.press(Key::ShiftLeft, shift);
    t.press(Key::ShiftRight, shift);
    t.release(Key::ShiftRight);
    assert_eq!(t.release(Key::ShiftLeft).mode, Mode::Kana);
}

#[test]
fn a_shift_tapped_while_the_other_shift_is_held_for_a_letter_is_not_a_tap() {
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let mut t = T::new();
    t.press(Key::ShiftLeft, shift);
    t.ch('A');
    t.press(Key::ShiftRight, shift);
    assert_eq!(t.release(Key::ShiftRight).mode, Mode::Abc);
}

#[test]
fn a_modifier_key_held_across_a_focus_change_does_not_block_taps() {
    let mut t = T::new();
    t.press(
        Key::ShiftLeft,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    t.handle(Event::FocusOut);
    t.handle(Event::FocusIn { password: false });
    assert_eq!(t.tap(Key::ShiftRight).mode, Mode::Kana);
}
