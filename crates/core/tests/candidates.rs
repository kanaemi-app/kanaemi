mod common;

use common::*;
use kanaemi_core::{Action, Binding, Event, Form, Gesture, Key, Modifiers};

#[test]
fn no_matching_candidate_shows_katakana_then_registration() {
    let mut t = T::new();
    t.kana();
    let out = t.typ(";ka;ka").1;
    assert_eq!(out.preedit, "»カカ");
    assert_eq!(t.go_past_the_candidates().preedit, "»か*か « ");
}

#[test]
fn backspace_while_choosing_cancels_as_escape_does() {
    // Converted by Space, by the okurigana, and with keys typed while choosing.
    let choosing = |keys: &str| {
        let mut t = T::new();
        t.kana();
        let out = t.typ(keys).1;
        if !out.preedit.starts_with('»') {
            t.key(Key::Space);
        }
        t
    };
    for keys in [";kanji", ";mo;tta", ";i;tts"] {
        let escaped = choosing(keys).key(Key::Esc);
        let erased = choosing(keys).key(Key::Backspace);
        assert!(
            escaped.preedit.starts_with('›'),
            "{keys}: {}",
            escaped.preedit
        );
        assert_eq!(
            (erased.preedit, erased.candidates, erased.commit),
            (escaped.preedit, escaped.candidates, escaped.commit),
            "{keys}"
        );
    }
}

#[test]
fn space_cycles_forward_and_shift_space_backward() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    t.key(Key::Space);
    assert_eq!(t.key(Key::Space).preedit, "»感じ");
    assert_eq!(t.shifted(Key::Space).preedit, "»漢字");
    assert_eq!(
        t.shifted(Key::Space).preedit,
        "»kanji",
        "before the first is the last"
    );
}

#[test]
fn space_past_the_last_candidate_starts_registration() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kisha");
    for _ in 0..6 {
        t.key(Key::Space);
    }
    assert_eq!(t.key(Key::Space).preedit, "»きしゃ « ");
}

#[test]
fn a_digit_commits_that_candidate() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    t.key(Key::Space);
    assert_eq!(t.ch('2').commit.as_deref(), Some("感じ"));
}

#[test]
fn enter_commits_the_selected_candidate_and_records_it() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    t.key(Key::Space);
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("漢字"));
    assert_eq!(
        t.converter().commits,
        [("かんじ".to_owned(), "漢字".to_owned())]
    );
}

#[test]
fn typing_while_candidate_mode_commits_implicitly() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    t.key(Key::Space);
    assert_eq!(t.typ("wo").0, "漢字を");
}

#[test]
fn escape_and_backspace_return_to_the_reading() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    t.key(Key::Space);
    assert_eq!(t.key(Key::Esc).preedit, "›かんじ");
    t.key(Key::Space);
    assert_eq!(t.key(Key::Backspace).preedit, "›かんじ");
}

#[test]
fn selecting_with_the_mouse_commits_that_candidate() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    t.key(Key::Space);
    assert_eq!(t.handle(Event::Select(2)).commit.as_deref(), Some("幹事"));
}

#[test]
fn katakana_already_among_the_candidates_is_not_added_again() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kana");
    let out = t.key(Key::Space);
    assert_eq!(surfaces(&out), ["カナ", "仮名", "ｶﾅ", "ｋａｎａ", "kana"]);
}

#[test]
fn a_reading_without_candidates_offers_its_own_forms_then_registration() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("nunu");
    let out = t.key(Key::Space);
    assert_eq!(out.preedit, "»ヌヌ");
    assert_eq!(surfaces(&out), ["ヌヌ", "ﾇﾇ", "ｎｕｎｕ", "nunu"]);
    assert_eq!(t.go_past_the_candidates().preedit, "»ぬぬ « ");
}

#[test]
fn shift_delete_hides_the_selected_candidate() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    t.key(Key::Space);
    t.key(Key::Space);
    let out = t.forget();
    assert_eq!(out.preedit, "»幹事");
    assert_eq!(
        t.converter().deleted,
        [("かんじ".to_owned(), "感じ".to_owned())]
    );
}

#[test]
fn the_first_shift_delete_only_asks() {
    let mut t = T::new();
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    let out = t.shifted(Key::Delete);
    assert_eq!(
        out.preedit,
        "»漢字（候補から除外するには、もう一度同じキーを押してください）"
    );
    assert!(out.candidates.is_some());
    assert!(t.converter().deleted.is_empty());
}

#[test]
fn cancel_while_asking_keeps_the_candidate() {
    let mut t = T::new();
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    t.shifted(Key::Delete);
    assert_eq!(t.key(Key::Esc).preedit, "»漢字");
    assert_eq!(
        t.shifted(Key::Delete).preedit,
        "»漢字（候補から除外するには、もう一度同じキーを押してください）",
        "asks again"
    );
    assert!(t.converter().deleted.is_empty());
}

#[test]
fn another_key_while_asking_does_what_it_does_and_forgets_nothing() {
    let mut t = T::new();
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    t.shifted(Key::Delete);
    assert_eq!(t.key(Key::Space).preedit, "»感じ");
    assert_eq!(
        t.shifted(Key::Delete).preedit,
        "»感じ（候補から除外するには、もう一度同じキーを押してください）",
        "asks again"
    );
    assert!(t.converter().deleted.is_empty());
}

#[test]
fn letting_go_of_shift_between_the_presses_still_forgets() {
    let mut t = T::new();
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    t.shifted(Key::Delete);
    t.release(Key::Delete);
    t.shifted(Key::ShiftLeft);
    t.shifted(Key::Delete);
    assert_eq!(
        t.converter().deleted,
        [("かんじ".to_owned(), "漢字".to_owned())]
    );
}

#[test]
fn holding_the_key_down_does_not_answer_the_question() {
    let mut t = T::new();
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    t.shifted(Key::Delete);
    for _ in 0..5 {
        t.repeat(Key::Delete, shift);
    }
    assert!(t.converter().deleted.is_empty(), "repeats are one press");
    t.shifted(Key::Delete);
    assert_eq!(
        t.converter().deleted,
        [("かんじ".to_owned(), "漢字".to_owned())]
    );
}

#[test]
fn an_ignored_shortcut_while_asking_withdraws_the_question() {
    let mut t = T::new();
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    t.shifted(Key::Delete);
    let ctrl_z = t.press(
        Key::Char('z'),
        Modifiers {
            ctrl: true,
            ..Default::default()
        },
    );
    assert_eq!(ctrl_z.preedit, "»漢字");
    t.shifted(Key::Delete);
    assert!(t.converter().deleted.is_empty());
}

#[test]
fn caps_lock_or_fn_while_asking_keeps_the_question() {
    let mut t = T::new();
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    t.shifted(Key::Delete);
    t.key(Key::Modifier);
    t.shifted(Key::Delete);
    assert_eq!(
        t.converter().deleted,
        [("かんじ".to_owned(), "漢字".to_owned())]
    );
}

#[test]
fn a_candidate_that_cannot_be_forgotten_is_not_asked_about() {
    let mut t = T::new();
    t.kana();
    t.typ(";kisha");
    t.key(Key::Space);
    t.key(Key::Space);
    t.key(Key::Space);
    assert_eq!(t.shifted(Key::Delete).preedit, "»キシャ");
}

#[test]
fn deleting_a_candidate_selects_the_one_after_it() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kana");
    t.key(Key::Space);
    t.key(Key::Space);
    let out = t.forget();
    assert_eq!(out.preedit, "»ｶﾅ");
}

#[test]
fn the_katakana_candidate_cannot_be_deleted() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kisha");
    t.key(Key::Space);
    t.key(Key::Space);
    t.key(Key::Space);
    let out = t.forget();
    assert_eq!(out.preedit, "»キシャ");
    assert!(t.converter().deleted.is_empty());
}

#[test]
fn deleting_every_dictionary_candidate_leaves_katakana() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kisha");
    t.key(Key::Space);
    t.forget();
    let out = t.forget();
    assert_eq!(out.preedit, "»キシャ");
    assert_eq!(surfaces(&out), ["キシャ", "ｷｼｬ", "ｋｉｓｈａ", "kisha"]);
}

#[test]
fn ctrl_z_after_deleting_is_not_an_undo() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    t.key(Key::Space);
    t.forget();
    let out = t.press(
        Key::Char('z'),
        Modifiers {
            ctrl: true,
            ..Default::default()
        },
    );
    assert!(
        out.consumed,
        "a Ctrl key without a meaning is ignored in candidate mode"
    );
    assert_eq!(out.commit, None);
    assert_eq!(out.preedit, "»感じ");
}

#[test]
fn f6_to_f10_commit_a_reading_in_each_form() {
    for (key, form) in [
        (Key::F(6), "かんじ"),
        (Key::F(7), "カンジ"),
        (Key::F(8), "ｶﾝｼﾞ"),
        (Key::F(9), "ｋａｎｊｉ"),
        (Key::F(10), "kanji"),
    ] {
        let mut t = reading_kanji();
        let out = t.key(key);
        assert_eq!(out.commit.as_deref(), Some(form), "{key:?}");
        assert_eq!(out.preedit, "", "{key:?}");
        assert_eq!(
            t.converter().commits.last().map(|c| c.1.as_str()),
            Some(form)
        );
    }
}

#[test]
fn f6_to_f10_commit_while_choosing_too() {
    let mut t = reading_kanji();
    t.key(Key::Space);
    assert_eq!(t.key(Key::F(7)).commit.as_deref(), Some("カンジ"));
}

#[test]
fn the_selecting_forms_turn_a_reading_into_each_form_without_committing() {
    for (key, form) in [
        (Key::F(6), "かんじ"),
        (Key::F(7), "カンジ"),
        (Key::F(8), "ｶﾝｼﾞ"),
        (Key::F(9), "ｋａｎｊｉ"),
        (Key::F(10), "kanji"),
    ] {
        let mut t = selecting_forms();
        t.kana();
        t.typ(";kanji");
        let out = t.key(key);
        assert_eq!(out.commit, None, "{key:?}");
        assert_eq!(out.preedit, format!("»{form}"), "{key:?}");
        assert_eq!(t.key(Key::Enter).commit.as_deref(), Some(form), "{key:?}");
    }
}

#[test]
fn the_selecting_forms_work_while_choosing_too() {
    let mut t = selecting_forms();
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    assert_eq!(t.key(Key::F(10)).preedit, "»kanji");
    assert_eq!(t.key(Key::F(7)).preedit, "»カンジ");
}

#[test]
fn a_committing_form_of_nothing_typed_commits_nothing() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    let out = t.key(Key::F(7));
    assert_eq!(out.commit, None);
}

#[test]
fn the_typed_letters_follow_katakana_among_the_candidates() {
    let mut t = reading_kanji();
    let out = t.key(Key::Space);
    let surfaces: Vec<_> = out
        .candidates
        .unwrap()
        .items
        .into_iter()
        .map(|c| c.surface)
        .collect();
    assert_eq!(
        surfaces,
        [
            "漢字",
            "感じ",
            "幹事",
            "カンジ",
            "ｶﾝｼﾞ",
            "ｋａｎｊｉ",
            "kanji"
        ]
    );
}

#[test]
fn a_reading_shortened_keeps_the_letters_of_what_is_left() {
    let mut t = reading_kanji();
    t.key(Key::Backspace);
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("kan"));
}

#[test]
fn a_reading_edited_in_the_middle_still_has_its_letters() {
    let mut t = reading_kanji();
    t.key(Key::Left);
    t.key(Key::Left);
    t.typ("na");
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("kananji"));
}

#[test]
fn kana_whose_keys_are_broken_are_spelled_from_the_table() {
    let mut t = T::new();
    t.kana();
    t.typ(";kya");
    t.key(Key::Backspace);
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("ki"));
}

#[test]
fn half_width_katakana_splits_voiced_marks() {
    let mut t = T::new();
    t.kana();
    t.typ(";gapporo-");
    assert_eq!(t.key(Key::F(8)).commit.as_deref(), Some("ｶﾞｯﾎﾟﾛｰ"));
}

#[test]
fn unfinished_romaji_stays_in_the_letters() {
    let mut t = T::new();
    t.kana();
    t.typ(";kank");
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("kank"));
}

#[test]
fn unfinished_romaji_stays_in_the_letters_while_choosing() {
    let mut t = selecting_forms();
    t.kana();
    t.typ(";kank");
    t.key(Key::F(10));
    t.key(Key::F(7));
    assert_eq!(t.key(Key::F(10)).preedit, "»kank");
}

#[test]
fn a_key_the_romaji_swallows_stays_in_the_letters() {
    let mut t = T::new();
    t.kana();
    t.typ(";test");
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("test"));
}

#[test]
fn a_key_bound_to_move_commits_unfinished_romaji_like_an_arrow() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ("kon").0, "こ");
    let out = t.ctrl('b');
    assert!(!out.consumed);
    assert_eq!(out.commit.as_deref(), Some("ん"));
    assert_eq!(out.preedit, "");
}

#[test]
fn letters_that_make_no_kana_still_convert_to_letters() {
    let mut t = T::new();
    t.kana();
    t.typ(";pdf");
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("pdf"));
}

#[test]
fn unfinished_romaji_keeps_its_place_in_the_letters() {
    let mut t = reading_kanji();
    t.key(Key::Home);
    t.ch('k');
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("kkanji"));
}

#[test]
fn moving_the_cursor_alone_keeps_the_letters() {
    let mut t = T::new();
    t.kana();
    t.typ(";kya");
    t.key(Key::Left);
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("kya"));
}

#[test]
fn moving_the_cursor_keeps_keys_that_made_no_kana() {
    let mut t = T::new();
    t.kana();
    t.typ(";test");
    t.key(Key::End);
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("test"));
}

#[test]
fn candidates_are_shown_nine_to_a_page() {
    let mut t = T::new();
    t.kana();
    t.typ(";kou");
    let out = t.key(Key::Space);
    let view = out.candidates.unwrap();
    assert_eq!(view.items.len(), 9);
    assert_eq!((view.items[0].surface.as_str(), view.selected), ("高", 0));
    for _ in 0..8 {
        t.key(Key::Space);
    }
    let view = t.key(Key::Space).candidates.unwrap();
    assert_eq!((view.items[0].surface.as_str(), view.selected), ("工", 0));
    let view = t.key(Key::Space).candidates.unwrap();
    assert_eq!((view.items[0].surface.as_str(), view.selected), ("工", 1));
    let view = t.shifted(Key::Space).candidates.unwrap();
    let view_back = t.shifted(Key::Space).candidates.unwrap();
    assert_eq!(view.selected, 0);
    assert_eq!(
        (view_back.items[0].surface.as_str(), view_back.selected),
        ("高", 8)
    );
}

#[test]
fn the_list_tells_which_page_it_shows_of_how_many() {
    let mut t = T::new();
    t.kana();
    t.typ(";kou");
    let view = t.key(Key::Space).candidates.unwrap();
    assert_eq!((view.page, view.pages), (0, 2));
    let view = t.ctrl('n').candidates.unwrap();
    assert_eq!((view.page, view.pages), (1, 2));
}

#[test]
fn ctrl_n_goes_to_the_first_candidate_of_the_next_page() {
    let mut t = T::new();
    t.kana();
    t.typ(";kou");
    t.key(Key::Space);
    t.key(Key::Space);
    let out = t.ctrl('n');
    assert_eq!(out.preedit, "»工");
    let view = out.candidates.unwrap();
    assert_eq!((view.items[0].surface.as_str(), view.selected), ("工", 0));
}

#[test]
fn ctrl_p_goes_to_the_first_candidate_of_the_previous_page() {
    let mut t = T::new();
    t.kana();
    t.typ(";kou");
    for _ in 0..11 {
        t.key(Key::Space);
    }
    let out = t.ctrl('p');
    assert_eq!(out.preedit, "»高");
    assert_eq!(out.candidates.unwrap().page, 0);
}

#[test]
fn the_next_page_after_the_last_is_the_first_and_the_previous_before_the_first_is_the_last() {
    let mut t = T::new();
    t.kana();
    t.typ(";kou");
    t.key(Key::Space);
    t.ctrl('n');
    assert_eq!(t.ctrl('n').preedit, "»高", "past the last page");
    assert_eq!(t.ctrl('p').preedit, "»工", "before the first page");
}

#[test]
fn the_next_page_of_a_single_page_never_starts_registering() {
    let mut t = T::new();
    t.kana();
    t.typ(";kanji");
    t.key(Key::Space);
    t.key(Key::Space);
    let out = t.ctrl('n');
    assert_eq!(out.preedit, "»漢字");
    assert_eq!(out.candidates.unwrap().pages, 1);
}

#[test]
fn page_down_and_page_up_turn_pages_once_bound() {
    let mut config = config();
    config.bindings.candidates.extend([
        Binding {
            from: plain(Key::PageDown),
            gesture: Gesture::Press,
            to: Action::NextPage,
        },
        Binding {
            from: plain(Key::PageUp),
            gesture: Gesture::Press,
            to: Action::PreviousPage,
        },
    ]);
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";kou");
    t.key(Key::Space);
    assert_eq!(t.key(Key::PageDown).preedit, "»工");
    assert_eq!(t.key(Key::PageUp).preedit, "»高");
}

#[test]
fn a_number_picks_from_the_page_it_shows() {
    let mut t = T::new();
    t.kana();
    t.typ(";kou");
    t.key(Key::Space);
    assert_eq!(t.ch('2').commit.as_deref(), Some("校"));
    t.typ(";kou");
    for _ in 0..10 {
        t.key(Key::Space);
    }
    assert_eq!(t.ch('2').commit.as_deref(), Some("功"));
}

#[test]
fn a_number_past_the_last_page_does_nothing() {
    let mut t = T::new();
    t.kana();
    t.typ(";kou");
    let mut out = t.key(Key::Space);
    for _ in 0..9 {
        out = t.key(Key::Space);
    }
    let on_page = out.candidates.unwrap().items.len();
    assert!(on_page < 8);
    let out = t.ch('8');
    assert_eq!(out.commit, None);
    assert!(out.candidates.is_some());
}

#[test]
fn the_mouse_picks_from_the_page_it_shows() {
    let mut t = T::new();
    t.kana();
    t.typ(";kou");
    for _ in 0..10 {
        t.key(Key::Space);
    }
    assert_eq!(t.handle(Event::Select(2)).commit.as_deref(), Some("孝"));
}

#[test]
fn each_candidate_shows_the_dictionary_it_came_from_and_a_form_of_the_reading_none() {
    let mut t = T::new();
    t.kana();
    t.typ(";kanji");
    let view = t.key(Key::Space).candidates.unwrap();
    let sources: Vec<(&str, Option<&str>)> = view
        .items
        .iter()
        .map(|c| (c.surface.as_str(), c.source.as_deref()))
        .collect();
    assert_eq!(
        sources[..4],
        [
            ("漢字", Some(SOURCE)),
            ("感じ", Some(SOURCE)),
            ("幹事", Some(SOURCE)),
            ("カンジ", None),
        ]
    );
}

#[test]
fn a_committing_form_that_has_no_such_form_commits_nothing() {
    let mut config = config();
    config.bindings.reading.retain(|b| b.from.key != Key::F(10));
    config.bindings.reading.push(Binding {
        from: plain(Key::F(10)),
        gesture: Gesture::Press,
        to: Action::Form(Form::Alphanumeric),
    });
    let mut t = T::with_config(config);
    t.kana();
    t.typ(";pdf");
    assert_eq!(t.key(Key::F(10)).preedit, "»pdf");
    let out = t.key(Key::F(6));
    assert_eq!(out.commit, None);
    assert_eq!(out.preedit, "»pdf");
}

#[test]
fn the_mouse_picks_nothing_past_the_page_it_shows() {
    let mut t = T::new();
    t.kana();
    t.typ(";kou");
    t.key(Key::Space);
    for index in [9, usize::MAX] {
        let out = t.handle(Event::Select(index));
        assert_eq!(out.commit, None, "{index}");
        assert!(out.candidates.is_some(), "{index}");
    }
}

#[test]
fn a_dictionary_candidate_can_be_forgotten_though_it_is_a_form_of_the_reading() {
    let mut t = T::new();
    t.kana();
    t.typ(";kana");
    t.key(Key::Space);
    let out = t.forget();
    assert_eq!(
        t.converter().deleted,
        [("かな".to_owned(), "カナ".to_owned())]
    );
    assert_eq!(out.preedit, "»仮名");
    assert_eq!(surfaces(&out), ["仮名", "ｶﾅ", "ｋａｎａ", "kana", "カナ"]);
}

#[test]
fn the_form_offered_again_after_forgetting_cannot_be_forgotten() {
    let mut t = T::new();
    t.kana();
    t.typ(";kana");
    t.key(Key::Space);
    t.forget();
    t.shifted(Key::Space);
    let out = t.forget();
    assert_eq!(out.preedit, "»カナ");
    assert_eq!(t.converter().deleted.len(), 1);
}

#[test]
fn moving_to_the_start_keeps_keys_that_made_no_kana() {
    let mut t = T::new();
    t.kana();
    t.typ(";pdf");
    t.key(Key::Home);
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("pdf"));
}

#[test]
fn keys_that_made_no_kana_at_the_start_keep_their_place() {
    let mut t = T::new();
    t.kana();
    t.typ(";ka");
    t.key(Key::Home);
    t.ch('q');
    t.key(Key::End);
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("qka"));
}

#[test]
fn moving_the_cursor_keeps_keys_typed_after_a_shortened_kana() {
    for moved in [false, true] {
        let mut t = T::new();
        t.kana();
        t.typ(";kya");
        t.key(Key::Backspace);
        t.ch('k');
        if moved {
            t.key(Key::Home);
        }
        assert_eq!(
            t.key(Key::F(10)).commit.as_deref(),
            Some("kik"),
            "moved: {moved}"
        );
    }
}
