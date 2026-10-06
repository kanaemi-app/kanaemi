mod common;

use common::*;
use kanaemi_core::{Action, Binding, Bindings, Chord, Config, Gesture, Key, Mode, Modifiers};

#[test]
fn a_key_the_core_does_not_handle_passes_on_and_keeps_the_preedit() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kan");
    let out = t.key(Key::Other);
    assert!(!out.consumed);
    assert_eq!(out.commit, None);
    assert_eq!(out.preedit, "›かn");
}

#[test]
fn a_key_the_core_does_not_handle_with_cmd_is_ignored_while_typing() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    let out = t.press(
        Key::Other,
        Modifiers {
            cmd: true,
            ..Default::default()
        },
    );
    assert!(out.consumed);
    assert_eq!(out.commit, None);
}

#[test]
fn keys_without_a_meaning_in_a_reading_pass_on() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kan");
    let out = t.key(Key::F(1));
    assert!(!out.consumed);
    assert_eq!(out.preedit, "›かn");
}

#[test]
fn keys_without_a_meaning_while_candidate_mode_pass_on() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    t.key(Key::Space);
    let out = t.key(Key::F(1));
    assert!(!out.consumed);
    assert_eq!(out.preedit, "»漢字");
    assert!(
        t.key(Key::F(6)).consumed,
        "F6 and F7 do nothing in candidate mode"
    );
}

#[test]
fn keys_without_a_meaning_commit_unfinished_romaji_and_pass_on() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ("kon").0, "こ");
    let out = t.key(Key::F(1));
    assert!(!out.consumed);
    assert_eq!(out.commit.as_deref(), Some("ん"));
    assert_eq!(out.preedit, "");
}

#[test]
fn moving_and_deleting_keys_commit_unfinished_romaji_and_pass_on() {
    for key in [
        Key::Left,
        Key::Right,
        Key::Up,
        Key::Down,
        Key::Home,
        Key::End,
        Key::Delete,
        Key::F(2),
        Key::F(6),
    ] {
        let mut t = T::new();
        t.kana();
        assert_eq!(t.typ("kon").0, "こ");
        let out = t.key(key);
        assert!(!out.consumed, "{key:?}");
        assert_eq!(out.commit.as_deref(), Some("ん"), "{key:?}");
        assert_eq!(out.preedit, "", "{key:?}");
    }
}

#[test]
fn letters_forming_no_kana_are_committed_as_typed_when_a_key_passes_on() {
    let mut t = T::new();
    t.kana();
    t.ch('k');
    let out = t.key(Key::Delete);
    assert!(!out.consumed);
    assert_eq!(out.commit.as_deref(), Some("k"));
    assert_eq!(out.preedit, "");
}

#[test]
fn letters_forming_no_kana_can_be_dropped_when_a_key_passes_on() {
    let mut t = T::with_config(Config {
        keep_unfinished_romaji: false,
        ..config()
    });
    t.kana();
    t.ch('k');
    let out = t.key(Key::Delete);
    assert!(!out.consumed);
    assert_eq!(out.commit, None);
    assert_eq!(out.preedit, "");
}

#[test]
fn keys_without_a_meaning_pass_on_during_a_registration_typed_in_abc_mode() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    t.go_past_the_candidates();
    t.tap(Key::ShiftLeft);
    t.typ("vim");
    let out = t.key(Key::F(1));
    assert!(!out.consumed);
    assert!(out.preedit.ends_with("vim"));
}

#[test]
fn ctrl_h_is_backspace_in_a_reading() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    let out = t.press(
        Key::Char('h'),
        Modifiers {
            ctrl: true,
            ..Default::default()
        },
    );
    assert!(out.consumed);
    assert_eq!(out.commit, None);
    assert_eq!(out.preedit, "›かん");
}

#[test]
fn ctrl_g_cancels_like_esc() {
    let mut t = reading_kanji();
    let out = t.ctrl('g');
    assert!(out.consumed);
    assert_eq!((out.preedit.as_str(), out.mode), ("", Mode::Kana));
}

#[test]
fn ctrl_j_and_ctrl_m_commit_like_enter() {
    for c in ['j', 'm'] {
        let mut t = reading_kanji();
        assert_eq!(t.ctrl(c).commit.as_deref(), Some("かんじ"), "Ctrl+{c}");
    }
}

#[test]
fn ctrl_n_and_ctrl_p_move_through_candidates() {
    let mut t = reading_kanji();
    t.key(Key::Space);
    assert_eq!(t.ctrl('n').preedit, "»感じ");
    assert_eq!(t.ctrl('p').preedit, "»漢字");
}

#[test]
fn other_ctrl_keys_are_ignored_while_typing() {
    let mut t = reading_kanji();
    let out = t.ctrl('z');
    assert!(out.consumed);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›かんじ"));
}

#[test]
fn ctrl_keys_pass_on_when_nothing_is_being_typed() {
    let mut t = T::new();
    t.kana();
    let out = t.ctrl('x');
    assert!(!out.consumed);
    assert_eq!(out.commit, None);
}

#[test]
fn the_ctrl_key_bindings_can_be_changed() {
    let ctrl = |c| Chord {
        key: Key::Char(c),
        mods: Modifiers {
            ctrl: true,
            ..Default::default()
        },
    };
    let mut config = config();
    config.bindings.reading.retain(|b| b.from != ctrl('g'));
    config.bindings.reading.push(Binding {
        from: ctrl('k'),
        gesture: Gesture::Press,
        to: Action::Cancel,
    });
    let mut t = T::with_config(config);
    t.kana();
    t.ch(';');
    t.typ("kanji");
    assert_eq!(
        t.ctrl('g').preedit,
        "›かんじ",
        "an unbound Ctrl key is ignored"
    );
    assert_eq!(t.ctrl('k').preedit, "", "Ctrl+K now cancels");
}

#[test]
fn the_cursor_keys_have_ctrl_bindings() {
    let mut t = reading_kanji();
    assert_eq!(t.ctrl('a').preedit, "›|かんじ");
    assert_eq!(t.ctrl('f').preedit, "›か|んじ");
    assert_eq!(t.ctrl('e').preedit, "›かんじ");
    assert_eq!(t.ctrl('b').preedit, "›かん|じ");
    assert_eq!(t.ctrl('d').preedit, "›かん");
}

#[test]
fn up_and_down_move_through_candidates() {
    let mut t = reading_kanji();
    t.key(Key::Space);
    assert_eq!(t.key(Key::Down).preedit, "»感じ");
    assert_eq!(t.key(Key::Up).preedit, "»漢字");
}

#[test]
fn cursor_keys_pass_on_when_nothing_is_being_typed() {
    let mut t = T::new();
    t.kana();
    for key in [
        Key::Left,
        Key::Right,
        Key::Up,
        Key::Down,
        Key::Home,
        Key::End,
    ] {
        assert!(!t.key(key).consumed, "{key:?}");
    }
}

#[test]
fn modifier_keys_can_be_set_to_commit_and_pass_while_typing() {
    let config = Config {
        pass_while_composing: Modifiers {
            cmd: true,
            ..Default::default()
        },
        ..config()
    };
    let with = |mods: Modifiers| {
        let mut t = T::with_config(config.clone());
        t.kana();
        t.ch(';');
        t.typ("kan");
        t.press(Key::Char('s'), mods)
    };
    let out = with(Modifiers {
        cmd: true,
        ..Default::default()
    });
    assert_eq!((out.consumed, out.commit.as_deref()), (false, Some("かん")));
    let out = with(Modifiers {
        ctrl: true,
        ..Default::default()
    });
    assert_eq!((out.consumed, out.preedit.as_str()), (true, "›かn"));
}

#[test]
fn a_bound_key_with_nothing_being_typed_is_sent_to_the_application_as_another_key() {
    for kana in [false, true] {
        let mut t = T::new();
        if kana {
            t.kana();
        }
        let out = t.ctrl('h');
        assert!(out.consumed, "kana: {kana}");
        assert_eq!(
            out.send,
            Some(Chord {
                key: Key::Backspace,
                mods: Modifiers::default()
            }),
            "kana: {kana}"
        );
    }
}

#[test]
fn while_typing_the_key_works_on_the_preedit_instead() {
    let mut t = reading_kanji();
    let out = t.ctrl('h');
    assert_eq!((out.send, out.preedit.as_str()), (None, "›かん"));
}

#[test]
fn an_unbound_key_with_nothing_being_typed_passes_on_as_it_is() {
    let mut t = T::new();
    let out = t.ctrl('x');
    assert!(!out.consumed);
    assert_eq!(out.send, None);
}

#[test]
fn the_keys_sent_to_the_application_can_be_changed() {
    let mut t = T::with_config(Config {
        bindings: Bindings {
            application: Vec::new(),
            ..Bindings::default()
        },
        ..config()
    });
    let out = t.ctrl('h');
    assert!(!out.consumed);
    assert_eq!(out.send, None);
}

#[test]
fn ctrl_n_converts_a_reading_but_types_no_space_in_the_text_to_register() {
    let mut t = reading_kanji();
    assert!(
        t.key(Key::Up).consumed,
        "an arrow does not reach the application"
    );
    assert_eq!(t.ctrl('n').preedit, "»漢字");

    let mut t = T::new();
    t.kana();
    t.typ(";nunu");
    t.go_past_the_candidates();
    t.typ("kaji");
    let out = t.ctrl('n');
    assert!(out.consumed);
    assert_eq!(out.preedit, "»ぬぬ « かじ");
    assert!(t.key(Key::Down).consumed);
}

#[test]
fn previous_in_a_reading_converts_to_the_last_candidate() {
    let mut t = reading_kanji();
    assert_eq!(t.ctrl('p').preedit, "»kanji");
    let mut t = reading_kanji();
    assert_eq!(t.shifted(Key::Space).preedit, "»kanji");
}

#[test]
fn space_is_a_key_like_any_other_and_can_do_something_else() {
    let mut config = config();
    config
        .bindings
        .candidates
        .retain(|b| b.from != plain(Key::Space));
    config.bindings.candidates.push(Binding {
        from: plain(Key::Space),
        gesture: Gesture::Press,
        to: Action::Commit,
    });
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";kanji");
    assert_eq!(t.ctrl('n').preedit, "»漢字", "Ctrl+N still converts");
    assert_eq!(t.ctrl('n').preedit, "»感じ");
    assert_eq!(t.key(Key::Space).commit.as_deref(), Some("感じ"));
}

#[test]
fn a_key_taken_out_does_nothing_while_typing() {
    let mut config = config();
    config
        .bindings
        .reading
        .retain(|b| b.from != plain(Key::Space));
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";kanji");
    let out = t.key(Key::Space);
    assert!(out.consumed);
    assert_eq!(out.preedit, "›かんじ");
}

#[test]
fn a_letter_can_go_back_a_candidate_as_in_skk() {
    let mut config = config();
    config.bindings.candidates.push(Binding {
        from: plain(Key::Char('x')),
        gesture: Gesture::Press,
        to: Action::Previous,
    });
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    t.key(Key::Space);
    assert_eq!(t.ch('x').preedit, "»漢字");
}

#[test]
fn a_key_can_leave_kana_mode_while_nothing_is_typed() {
    let mut config = config();
    config.bindings.kana.push(Binding {
        from: plain(Key::Char('l')),
        gesture: Gesture::Press,
        to: Action::Abc,
    });
    let mut t = T::with_config(config);
    t.kana();
    let out = t.ch('l');
    assert!(out.consumed);
    assert_eq!(out.mode, Mode::Abc);
    assert_eq!(out.indicator, Some(Mode::Abc));
    assert!(
        !t.ch('l').consumed,
        "in ABC mode l is typed by the application"
    );
}

#[test]
fn a_key_can_enter_kana_mode_from_abc_mode() {
    let mut config = config();
    config.bindings.abc.push(Binding {
        from: Chord {
            key: Key::Char('j'),
            mods: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        },
        gesture: Gesture::Press,
        to: Action::Kana,
    });
    let mut t = T::with_config(config);
    let out = t.ctrl('j');
    assert!(out.consumed);
    assert_eq!(out.mode, Mode::Kana);
}

#[test]
fn leaving_kana_mode_from_a_reading_commits_it_first() {
    let mut config = config();
    config.bindings.reading.push(Binding {
        from: plain(Key::Char('l')),
        gesture: Gesture::Press,
        to: Action::Abc,
    });
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";kanji");
    let out = t.ch('l');
    assert_eq!(out.commit.as_deref(), Some("かんじ"));
    assert_eq!(out.mode, Mode::Abc);
}

#[test]
fn a_shift_tap_is_a_binding_that_can_be_taken_out() {
    let mut config = config();
    config
        .bindings
        .kana
        .retain(|b| b.from.key != Key::ShiftLeft);
    let mut t = T::with_config(config);
    t.kana();
    assert_eq!(t.tap(Key::ShiftLeft).mode, Mode::Kana);
    assert_eq!(t.key(Key::Eisu).mode, Mode::Abc, "the 英数 key still is");
}

#[test]
fn a_shift_tap_can_do_something_else() {
    let mut config = config();
    config.bindings.reading.push(Binding {
        from: plain(Key::ShiftRight),
        gesture: Gesture::Tap,
        to: Action::Next,
    });
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";kanji");
    assert_eq!(t.tap(Key::ShiftRight).preedit, "»漢字");
}

#[test]
fn the_keys_that_pick_a_candidate_can_be_changed() {
    let mut config = config();
    config
        .bindings
        .candidates
        .retain(|b| !matches!(b.to, Action::Pick(_)));
    for (c, place) in [('a', 0), ('s', 1)] {
        config.bindings.candidates.push(Binding {
            from: plain(Key::Char(c)),
            gesture: Gesture::Press,
            to: Action::Pick(place),
        });
    }
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    assert_eq!(t.ch('s').commit.as_deref(), Some("感じ"));
    t.typ(";kanji");
    t.key(Key::Space);
    assert_eq!(
        t.ch('2').commit.as_deref(),
        Some("漢字２"),
        "2 is typed after the selected candidate"
    );
}

#[test]
fn a_ctrl_cmd_or_option_tap_can_be_bound() {
    for key in [Key::CtrlLeft, Key::CmdRight, Key::AltLeft] {
        let mut config = config();
        config.bindings.kana.push(Binding {
            from: plain(key),
            gesture: Gesture::Tap,
            to: Action::Abc,
        });
        let mut t = T::with_config(config);
        t.kana();
        let out = t.tap(key);
        assert_eq!(out.mode, Mode::Abc, "{key:?}");
        assert!(!out.consumed, "{key:?} reaches the application too");
    }
}

#[test]
fn a_modifier_bound_to_its_press_acts_as_it_goes_down() {
    let mut config = config();
    config.bindings.kana.push(Binding {
        from: plain(Key::ShiftLeft),
        gesture: Gesture::Press,
        to: Action::Abc,
    });
    let mut t = T::with_config(config);
    t.kana();
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let out = t.press(Key::ShiftLeft, shift);
    assert_eq!(out.mode, Mode::Abc);
    assert!(!out.consumed, "the application gets the modifier too");
}

#[test]
fn a_modifier_bound_to_its_press_acts_with_other_modifiers_held_as_written() {
    let ctrl = Modifiers {
        ctrl: true,
        ..Default::default()
    };
    let mut config = config();
    config.bindings.kana.push(Binding {
        from: Chord {
            key: Key::ShiftLeft,
            mods: ctrl,
        },
        gesture: Gesture::Press,
        to: Action::Abc,
    });
    let mut t = T::with_config(config);
    t.kana();
    let out = t.press(
        Key::ShiftLeft,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(out.mode, Mode::Kana, "Ctrl is not held");
    t.release(Key::ShiftLeft);
    let out = t.press(
        Key::ShiftLeft,
        Modifiers {
            shift: true,
            ctrl: true,
            ..Default::default()
        },
    );
    assert_eq!(out.mode, Mode::Abc);
}

#[test]
fn a_modifier_pressed_while_its_other_side_is_held_is_bound_with_that_modifier() {
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let mut config = config();
    config.bindings.kana.extend([
        Binding {
            from: Chord {
                key: Key::ShiftRight,
                mods: shift,
            },
            gesture: Gesture::Press,
            to: Action::Abc,
        },
        Binding {
            from: plain(Key::ShiftRight),
            gesture: Gesture::Press,
            to: Action::Kana,
        },
    ]);
    let mut t = T::with_config(config);
    t.kana();
    t.press(Key::ShiftLeft, shift);
    assert_eq!(t.press(Key::ShiftRight, shift).mode, Mode::Abc);
}

#[test]
fn a_tap_binding_does_not_act_as_the_modifier_goes_down() {
    let mut t = T::new();
    t.kana();
    let out = t.press(
        Key::ShiftLeft,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(out.mode, Mode::Kana);
    assert_eq!(t.release(Key::ShiftLeft).mode, Mode::Abc);
}

#[test]
fn a_press_binding_does_not_act_on_a_tap() {
    let mut config = config();
    config.bindings.reading.push(Binding {
        from: plain(Key::ShiftRight),
        gesture: Gesture::Press,
        to: Action::Next,
    });
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";kanji");
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    assert_eq!(t.press(Key::ShiftRight, shift).preedit, "»漢字");
    assert_eq!(
        t.release(Key::ShiftRight).preedit,
        "»漢字",
        "the release is not one more next"
    );
}

#[test]
fn how_long_a_tap_may_last_is_set() {
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let mut t = T::with_config(Config {
        tap_timeout_ms: 1_000,
        ..config()
    });
    t.press(Key::ShiftRight, shift);
    t.now += 800;
    assert_eq!(t.release(Key::ShiftRight).mode, Mode::Kana);

    let mut t = T::with_config(Config {
        tap_timeout_ms: 100,
        ..config()
    });
    t.press(Key::ShiftRight, shift);
    t.now += 150;
    assert_eq!(t.release(Key::ShiftRight).mode, Mode::Abc);
}

#[test]
fn a_modifier_held_for_a_shortcut_is_not_a_tap() {
    let mut config = config();
    config.bindings.kana.push(Binding {
        from: plain(Key::CmdLeft),
        gesture: Gesture::Tap,
        to: Action::Abc,
    });
    let mut t = T::with_config(config);
    t.kana();
    let cmd = Modifiers {
        cmd: true,
        ..Default::default()
    };
    t.press(Key::CmdLeft, cmd);
    t.press(Key::Char('c'), cmd);
    assert_eq!(t.release(Key::CmdLeft).mode, Mode::Kana);
}

#[test]
fn a_shift_pressed_while_ctrl_is_held_is_not_a_tap() {
    let mut t = T::new();
    t.kana();
    t.press(
        Key::ShiftLeft,
        Modifiers {
            shift: true,
            ctrl: true,
            ..Default::default()
        },
    );
    assert_eq!(t.release(Key::ShiftLeft).mode, Mode::Kana);
}

#[test]
fn a_form_key_commits_a_lone_n_and_passes_on() {
    let mut t = T::new();
    t.kana();
    t.ch('n');
    let out = t.key(Key::F(7));
    assert!(!out.consumed);
    assert_eq!(out.commit.as_deref(), Some("ん"));
    assert_eq!(out.preedit, "");
}

#[test]
fn another_key_commits_unfinished_romaji_and_passes_on() {
    let mut t = T::new();
    t.kana();
    t.ch('n');
    let out = t.key(Key::Other);
    assert!(!out.consumed);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("ん"), "")
    );
}

#[test]
fn another_key_in_a_reading_passes_on_and_keeps_it() {
    let mut t = reading_kanji();
    let out = t.key(Key::Other);
    assert!(!out.consumed);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›かんじ"));
}

#[test]
fn henkan_and_muhenkan_switch_modes_and_convert() {
    let mut t = T::new();
    assert_eq!(t.key(Key::Henkan).mode, Mode::Kana);
    t.typ(";kanji");
    assert_eq!(t.key(Key::Henkan).preedit, "»漢字");
    assert_eq!(t.key(Key::Muhenkan).preedit, "»カンジ");
    t.key(Key::Enter);
    assert_eq!(t.key(Key::Muhenkan).mode, Mode::Abc);
}
