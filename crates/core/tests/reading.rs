mod common;

use common::*;
use kanaemi_core::{Config, Event, Key, Mode};

#[test]
fn right_shift_then_kanji_shows_the_reading() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    assert_eq!(t.typ("kanji").1.preedit, "›かんじ");
}

#[test]
fn space_converts_into_candidate_selection() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    let out = t.key(Key::Space);
    assert_eq!(out.preedit, "»漢字");
    assert_eq!(
        surfaces(&out),
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
fn space_on_an_empty_reading_does_nothing() {
    let mut t = T::new();
    t.kana();
    t.typ(";k");
    t.key(Key::Backspace);
    let out = t.key(Key::Space);
    assert_eq!(out.preedit, "›");
    assert!(out.candidates.is_none());
}

#[test]
fn enter_commits_the_kana() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("かんじ"));
}

#[test]
fn backspace_deletes_one_reading_char() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    assert_eq!(t.key(Key::Backspace).preedit, "›かん");
}

#[test]
fn escape_cancels_the_reading_and_stays_japanese() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    let out = t.key(Key::Esc);
    assert_eq!(out.preedit, "");
    assert!(out.consumed);
    assert_eq!(out.mode, Mode::Kana);
}

#[test]
fn left_shift_commits_the_reading_as_kana() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanji");
    let out = t.tap(Key::ShiftLeft);
    assert_eq!(out.commit.as_deref(), Some("かんじ"));
    assert_eq!(out.mode, Mode::Abc);
}

#[test]
fn an_uppercase_letter_inside_a_reading_is_typed_into_it() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ(";kaKu").1.preedit, "›かKう");
}

#[test]
fn the_first_okurigana_kana_converts() {
    let mut t = T::new();
    t.kana();
    let out = t.typ(";ka;ku").1;
    assert_eq!(out.preedit, "»書く");
    assert_eq!(
        t.converter()
            .okurigana_seen
            .last()
            .cloned()
            .flatten()
            .as_deref(),
        Some("く")
    );
}

#[test]
fn kana_made_only_from_romaji_typed_before_the_okurigana_stays_in_the_reading() {
    let mut t = T::new();
    t.kana();
    let out = t.typ(";kan;ji").1;
    assert_eq!(out.preedit, "»感じ");
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
fn a_small_tsu_typed_after_the_mark_goes_into_the_okurigana() {
    let mut t = T::new();
    t.kana();
    let out = t.typ(";ka;tt").1;
    assert_eq!(out.preedit, "»カッt");
    assert_eq!(
        t.converter()
            .okurigana_seen
            .last()
            .cloned()
            .flatten()
            .as_deref(),
        Some("っ")
    );
}

#[test]
fn the_rest_of_the_okurigana_is_typed_after_the_conversion() {
    let mut t = T::new();
    t.kana();
    let out = t.typ(";ta;be").1;
    assert_eq!(out.preedit, "»食べ");
    assert_eq!(t.typ("ru").0, "食べる");
}

#[test]
fn without_a_marker_the_okurigana_is_left_to_the_dictionary() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kaku");
    let out = t.key(Key::Space);
    assert_eq!(out.preedit, "»角");
    assert_eq!(t.converter().okurigana_seen.last().cloned().flatten(), None);
}

#[test]
fn backspace_removes_pending_romaji_then_the_marker() {
    let mut t = T::new();
    t.kana();
    t.typ(";ka;k");
    assert_eq!(t.key(Key::Backspace).preedit, "›か*");
    assert_eq!(t.key(Key::Backspace).preedit, "›か");
}

#[test]
fn backspace_and_escape_from_an_okurigana_conversion() {
    let mut t = T::new();
    t.kana();
    t.typ(";ka;ku");
    assert_eq!(t.key(Key::Backspace).preedit, "›か*");

    let mut t = T::new();
    t.kana();
    t.typ(";ka;ku");
    assert_eq!(t.key(Key::Esc).preedit, "›か*く");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("かく"));
}

#[test]
fn the_cursor_moves_inside_a_reading_and_typing_inserts_there() {
    let mut t = reading_kanji();
    let out = t.key(Key::Left);
    assert!(out.consumed);
    assert_eq!(out.preedit, "›かん|じ");
    assert_eq!(out.cursor, 5, "the application's cursor stays at the end");
    t.key(Key::Left);
    assert_eq!(t.typ("ka").1.preedit, "›かか|んじ");
    assert_eq!(
        t.ch('k').preedit,
        "›かかk|んじ",
        "romaji waits at the cursor"
    );
}

#[test]
fn home_end_and_right_move_the_cursor() {
    let mut t = reading_kanji();
    assert_eq!(t.key(Key::Home).preedit, "›|かんじ");
    assert_eq!(t.key(Key::Right).preedit, "›か|んじ");
    assert_eq!(t.key(Key::End).preedit, "›かんじ");
    assert_eq!(t.key(Key::Right).preedit, "›かんじ", "the end is the end");
}

#[test]
fn backspace_and_delete_work_around_the_cursor() {
    let mut t = reading_kanji();
    t.key(Key::Left);
    assert_eq!(t.key(Key::Backspace).preedit, "›か|じ");
    assert_eq!(t.key(Key::Delete).preedit, "›か");
}

#[test]
fn converting_uses_the_whole_reading_wherever_the_cursor_is() {
    let mut t = reading_kanji();
    t.key(Key::Home);
    assert_eq!(t.key(Key::Space).preedit, "»漢字");
}

#[test]
fn okurigana_is_marked_only_at_the_end() {
    let mut t = reading_kanji();
    t.key(Key::Left);
    assert_eq!(t.ch(';').preedit, "›かん|じ");
}

#[test]
fn the_rest_of_the_okurigana_carries_on_however_the_candidate_is_committed() {
    for commit in [Key::Enter, Key::Char('1')] {
        let mut t = T::new();
        t.kana();
        let out = t.typ(";mo;tt").1;
        assert!(out.preedit.ends_with('t'), "{}", out.preedit);
        let out = t.key(commit);
        assert_eq!(out.preedit, "t", "{commit:?}");
        assert_eq!(t.ch('a').commit.as_deref(), Some("た"), "{commit:?}");
    }
}

#[test]
fn finishing_the_rest_of_the_okurigana_keeps_the_candidates() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tt");
    let out = t.ch('a');
    assert_eq!((out.commit, out.preedit.as_str()), (None, "»持った"));
    assert!(out.candidates.is_some());
    assert_eq!(t.key(Key::Space).preedit, "»モッた");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("モッた"));
}

#[test]
fn a_character_after_the_rest_of_the_okurigana_is_finished_commits() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tta");
    let out = t.ch('k');
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("持った"), "k")
    );
}

#[test]
fn a_character_that_does_not_go_on_with_the_rest_of_the_okurigana_commits() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tt");
    let out = t.ch('k');
    assert_eq!(out.commit.as_deref(), Some("持っ"));
    assert!(out.candidates.is_none());
}

#[test]
fn a_character_that_drops_the_rest_of_the_okurigana_for_kana_of_its_own_commits() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tt");
    let out = t.ch('.');
    assert_eq!(out.commit.as_deref(), Some("持っ。"));
    assert!(out.candidates.is_none());
}

#[test]
fn backspace_erases_what_was_typed_after_the_okurigana_first() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tta");
    assert_eq!(t.key(Key::Backspace).preedit, "»持っt");
    assert_eq!(t.key(Key::Backspace).preedit, "›も*っ");
}

#[test]
fn going_back_to_the_reading_drops_what_was_typed_after_the_okurigana() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tta");
    assert_eq!(t.key(Key::Esc).preedit, "›も*っt");
}

#[test]
fn begin_after_the_rest_of_the_okurigana_is_finished_starts_an_empty_reading() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tta");
    let out = t.ch(';');
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("持った"), "›")
    );
}

#[test]
fn the_rest_of_the_okurigana_carries_on_after_a_mouse_pick() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tt");
    let out = t.handle(Event::Select(0));
    assert_eq!(out.preedit, "t");
}

#[test]
fn committing_what_is_visible_leaves_no_rest_behind() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tt");
    t.handle(Event::FocusOut);
    assert_eq!(t.ch('a').commit.as_deref(), Some("あ"));
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tt");
    let out = t.tap(Key::ShiftLeft);
    assert_eq!((out.mode, out.preedit.as_str()), (Mode::Abc, ""));
}

#[test]
fn romaji_typed_after_an_okurigana_survives_going_back_to_the_reading() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ(";mo;tt").1.preedit, "»持っt");
    assert_eq!(t.key(Key::Esc).preedit, "›も*っt");
    assert_eq!(t.key(Key::Backspace).preedit, "›も*っ");

    let mut t = T::new();
    t.kana();
    t.typ(";mo;tt");
    assert_eq!(
        t.key(Key::Backspace).preedit,
        "›も*っ",
        "the romaji goes first"
    );
}

#[test]
fn romaji_typed_after_an_okurigana_survives_a_registration() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tt");
    t.key(Key::Char('0'));
    let out = t.key(Key::Esc);
    assert_eq!(out.preedit, "›も*っt");
}

#[test]
fn only_the_first_kana_of_a_keystroke_goes_into_the_okurigana() {
    let mut t = T::new();
    t.kana();
    t.typ(";ka;n.");
    assert_eq!(
        t.converter()
            .okurigana_seen
            .last()
            .cloned()
            .flatten()
            .as_deref(),
        Some("ん")
    );
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("カン。"));
    assert_eq!(
        t.converter().commits.last(),
        Some(&("かん".to_owned(), "カン".to_owned()))
    );
}

#[test]
fn a_character_outside_the_table_after_the_okurigana_is_typed_after_the_word() {
    let mut t = T::new();
    t.kana();
    t.typ(";ka;n");
    t.ch('é');
    assert_eq!(
        t.converter()
            .okurigana_seen
            .last()
            .cloned()
            .flatten()
            .as_deref(),
        Some("ん")
    );
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("カンé"));
}

fn with_a_waiting_rule() -> T {
    let mut t = T::with_config(Config {
        romaji: table(&["ka\tか\na\tあ\nb\tい\nabc\tX"]),
        ..config()
    });
    t.kana();
    t
}

#[test]
fn romaji_waiting_for_a_longer_rule_at_the_mark_becomes_kana_of_the_reading() {
    let mut t = with_a_waiting_rule();
    t.typ(";kaa;b");
    assert_eq!(
        t.converter()
            .okurigana_seen
            .last()
            .cloned()
            .flatten()
            .as_deref(),
        Some("い")
    );
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("カアイ"));
}

#[test]
fn only_the_first_kana_resolved_at_a_conversion_goes_into_the_okurigana() {
    let mut t = with_a_waiting_rule();
    t.typ(";ka;ab");
    t.key(Key::Space);
    assert_eq!(
        t.converter()
            .okurigana_seen
            .last()
            .cloned()
            .flatten()
            .as_deref(),
        Some("あ")
    );
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("カアい"));
}

#[test]
fn romaji_after_an_okurigana_is_typed_as_it_is_when_registered_in_abc_mode() {
    let mut t = T::new();
    t.kana();
    t.typ(";mo;tt");
    t.key(Key::Char('0'));
    t.tap(Key::ShiftLeft);
    t.ch('x');
    let out = t.key(Key::Enter);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("xっt"), "")
    );
}

#[test]
fn moving_and_deleting_do_nothing_once_the_okurigana_is_marked() {
    for key in [Key::Left, Key::Right, Key::Home, Key::End, Key::Delete] {
        let mut t = T::new();
        t.kana();
        t.typ(";ka;n");
        assert_eq!(t.key(key).preedit, "›か*n", "{key:?}");
        t.ch('a');
        assert_eq!(
            t.converter()
                .okurigana_seen
                .last()
                .cloned()
                .flatten()
                .as_deref(),
            Some("な"),
            "{key:?}"
        );
    }
}
