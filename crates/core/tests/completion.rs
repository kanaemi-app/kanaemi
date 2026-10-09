mod common;

use common::*;
use kanaemi_core::{Key, Modifiers};

/// In kana mode, with `input` typed after a `;` that begins the reading.
fn reading(input: &str) -> T {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    if !input.is_empty() {
        t.typ(input);
    }
    t
}

fn back_tab(t: &mut T) -> kanaemi_core::Output {
    t.press(
        Key::Tab,
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    )
}

#[test]
fn tab_completes_the_reading_and_goes_round_back_to_what_was_typed() {
    let mut t = reading("ka");
    let shown: Vec<String> = (0..5).map(|_| t.key(Key::Tab).preedit).collect();
    assert_eq!(shown, ["›かく", "›かな", "›かんじ", "›か", "›かく"]);
}

#[test]
fn shift_tab_goes_round_the_other_way() {
    let mut t = reading("ka");
    assert_eq!(back_tab(&mut t).preedit, "›かんじ", "the last first");
    assert_eq!(back_tab(&mut t).preedit, "›かな");
    t.key(Key::Tab);
    assert_eq!(t.key(Key::Tab).preedit, "›か", "past the last, as typed");
    assert_eq!(back_tab(&mut t).preedit, "›かんじ");
}

#[test]
fn completing_uses_the_key_and_shows_no_list() {
    let mut t = reading("ka");
    let out = t.key(Key::Tab);
    assert!(out.consumed);
    assert_eq!(out.candidates, None);
    assert_eq!(out.commit, None);
}

#[test]
fn with_nothing_to_complete_with_the_key_does_nothing() {
    let mut t = reading("zo");
    let out = t.key(Key::Tab);
    assert!(
        out.consumed,
        "the reading stays, so the key is not passed on"
    );
    assert_eq!(out.preedit, "›ぞ");
}

#[test]
fn an_empty_reading_is_not_completed() {
    let mut t = reading("");
    let out = t.key(Key::Tab);
    assert!(out.consumed);
    assert_eq!(out.preedit, "›");
}

#[test]
fn a_reading_with_its_okurigana_marked_is_not_completed() {
    let mut t = reading("ka");
    t.ch(';');
    assert_eq!(t.key(Key::Tab).preedit, "›か*");
}

#[test]
fn a_completed_reading_converts_as_if_typed() {
    let mut t = reading("ki");
    assert_eq!(t.key(Key::Tab).preedit, "›きしゃ");
    let out = t.key(Key::Space);
    assert_eq!(out.preedit, "»記者");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("記者"));
    assert_eq!(
        t.converter().commits,
        [("きしゃ".to_owned(), "記者".to_owned())]
    );
}

#[test]
fn a_completed_reading_commits_as_kana() {
    let mut t = reading("ki");
    t.key(Key::Tab);
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("きしゃ"));
}

#[test]
fn the_letters_of_a_completed_reading_spell_what_was_added() {
    let mut t = reading("ki");
    t.key(Key::Tab);
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("kisha"));
}

#[test]
fn any_other_key_ends_the_completion() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    t.key(Key::Tab);
    assert_eq!(t.key(Key::Backspace).preedit, "›か");
    assert_eq!(
        t.key(Key::Tab).preedit,
        "›かく",
        "completes afresh from the reading as it is now"
    );
}

#[test]
fn typing_after_a_completion_goes_on_from_the_completed_reading() {
    let mut t = reading("ki");
    t.key(Key::Tab);
    let (_, out) = t.typ("ga");
    assert_eq!(out.preedit, "›きしゃが");
    assert_eq!(
        t.key(Key::Tab).preedit,
        "›きしゃが",
        "nothing goes on from it"
    );
}

#[test]
fn the_okurigana_can_be_marked_after_a_completion() {
    let mut t = reading("mo");
    assert_eq!(t.key(Key::Tab).preedit, "›もっ");
    t.ch(';');
    let (_, out) = t.typ("ta");
    assert_eq!(out.preedit, "»持った");
}

#[test]
fn the_reading_is_completed_whole_wherever_the_cursor_is() {
    let mut t = reading("ka");
    assert_eq!(t.key(Key::Left).preedit, "›|か");
    assert_eq!(t.key(Key::Tab).preedit, "›かく", "the cursor at the end");
    t.key(Key::Tab);
    t.key(Key::Tab);
    assert_eq!(
        t.key(Key::Tab).preedit,
        "›|か",
        "the cursor back where it was"
    );
}

#[test]
fn romaji_left_unfinished_is_made_kana_before_completing() {
    let mut t = reading("kix");
    assert_eq!(t.key(Key::Tab).preedit, "›きしゃ");
}

#[test]
fn tab_while_choosing_goes_on_to_the_next_reading_of_the_completion() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    assert_eq!(t.key(Key::Space).preedit, "»角");
    t.key(Key::Space);
    assert_eq!(
        t.key(Key::Tab).preedit,
        "›かな",
        "whichever candidate is chosen"
    );
    assert_eq!(t.key(Key::Tab).preedit, "›かんじ");
}

#[test]
fn shift_tab_while_choosing_goes_back_a_reading_of_the_completion() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    t.key(Key::Tab);
    t.key(Key::Space);
    assert_eq!(back_tab(&mut t).preedit, "›かく");
}

#[test]
fn tab_while_choosing_past_the_last_reading_goes_back_to_what_was_typed() {
    let mut t = reading("ka");
    (0..3).for_each(|_| {
        t.key(Key::Tab);
    });
    t.key(Key::Space);
    assert_eq!(t.key(Key::Tab).preedit, "›か");
}

#[test]
fn tab_while_choosing_a_reading_as_typed_completes_it_from_the_first() {
    let mut t = reading("ka");
    t.key(Key::Space);
    assert_eq!(t.key(Key::Tab).preedit, "›かく");
    assert_eq!(t.key(Key::Tab).preedit, "›かな");
}

#[test]
fn tab_while_choosing_with_nothing_to_complete_with_keeps_the_candidates() {
    let mut t = reading("kanji");
    t.key(Key::Space);
    let out = t.key(Key::Tab);
    assert!(out.consumed);
    assert_eq!(out.preedit, "»漢字");
    assert_eq!(t.key(Key::Space).preedit, "»感じ", "still choosing");
}

#[test]
fn tab_while_choosing_a_word_with_okurigana_keeps_the_candidates() {
    let mut t = reading("ka*ku");
    let chosen = t.key(Key::Space).preedit;
    assert_eq!(t.key(Key::Tab).preedit, chosen);
}

#[test]
fn tab_passes_on_with_nothing_typed() {
    let mut t = reading("kanji");
    t.key(Key::Space);
    t.key(Key::Enter);
    let out = t.key(Key::Tab);
    assert!(!out.consumed);
    assert_eq!(out.preedit, "");
}

#[test]
fn a_registration_can_complete_the_reading_it_types() {
    let mut t = reading("zo");
    t.go_past_the_candidates();
    t.ch(';');
    t.typ("ka");
    let out = t.key(Key::Tab);
    assert_eq!(out.preedit, "»ぞ « ›かく");
}
