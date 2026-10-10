mod common;

use common::*;
use kanaemi_core::{Event, Key, Mode, Modifiers, Output};

fn reread(t: &mut T) -> Output {
    t.ctrl(';')
}

fn undo(t: &mut T) -> Output {
    t.press(
        Key::Backspace,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    )
}

/// `input` typed in kana mode with no reading begun.
fn typed(input: &str) -> T {
    let mut t = T::new();
    t.kana();
    t.typ(input);
    t
}

/// `input` typed, then read again: the host erased the kana.
fn read_again(input: &str) -> (T, Output) {
    let mut t = typed(input);
    assert!(reread(&mut t).erase.is_some());
    let out = t.handle(Event::Erased(true));
    (t, out)
}

#[test]
fn kana_typed_without_a_reading_are_erased_and_read_again() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ("kanji").0, "かんじ");
    let out = reread(&mut t);
    assert!(out.consumed);
    assert_eq!(out.erase.as_deref(), Some("かんじ"));
    assert_eq!((out.preedit.as_str(), out.commit), ("", None));

    let out = t.handle(Event::Erased(true));
    assert_eq!(out.preedit, "›かんじ");
    assert_eq!(t.key(Key::Space).preedit, "»漢字");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("漢字"));
    let learned = t.converter();
    assert_eq!(learned.erased, ["かんじ"]);
    assert_eq!(learned.commits, [("かんじ".to_owned(), "漢字".to_owned())]);
}

#[test]
fn text_that_is_not_kana_ends_the_run() {
    let mut t = typed("a,kanji");
    assert_eq!(reread(&mut t).erase.as_deref(), Some("かんじ"));
    let mut t = typed("ka1nji");
    assert_eq!(reread(&mut t).erase.as_deref(), Some("んじ"));
    let mut t = typed("ka-");
    assert_eq!(reread(&mut t).erase.as_deref(), Some("かー"), "ー is kana");
}

#[test]
fn a_candidate_committed_ends_the_run() {
    let mut t = T::new();
    t.kana();
    t.typ(";kisha");
    t.key(Key::Space);
    t.key(Key::Enter);
    assert_eq!(t.typ("hakanji").0, "はかんじ");
    assert_eq!(reread(&mut t).erase.as_deref(), Some("はかんじ"));
}

#[test]
fn a_reading_committed_as_kana_cannot_be_read_again() {
    let mut t = T::new();
    t.kana();
    t.typ(";kana");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("かな"));
    let out = reread(&mut t);
    assert_eq!((out.consumed, out.erase), (false, None));
}

#[test]
fn a_converted_commit_is_not_read_again() {
    let mut t = T::new();
    t.kana();
    t.typ(";kisha");
    t.key(Key::Space);
    t.key(Key::Enter);
    let out = reread(&mut t);
    assert_eq!((out.consumed, out.erase), (false, None));
}

#[test]
fn with_nothing_to_read_again_the_key_passes_on() {
    let mut t = T::new();
    t.kana();
    let out = reread(&mut t);
    assert_eq!((out.consumed, out.erase), (false, None));
}

#[test]
fn a_reading_dropped_leaves_the_run() {
    let mut t = typed("ka;");
    t.key(Key::Esc);
    t.typ("nji");
    assert_eq!(reread(&mut t).erase.as_deref(), Some("かんじ"));
}

#[test]
fn unfinished_romaji_goes_on_after_the_reading() {
    let mut t = typed("nihon");
    let out = reread(&mut t);
    assert_eq!(out.erase.as_deref(), Some("にほ"));
    assert_eq!(out.preedit, "", "the romaji waits with the kana");
    assert_eq!(t.handle(Event::Erased(true)).preedit, "›にほn");
    assert_eq!(t.typ("go").1.preedit, "›にほんご");
}

#[test]
fn the_reading_read_again_keeps_the_keys_typed() {
    let (mut t, _) = read_again("kanji");
    assert_eq!(t.key(Key::F(10)).commit.as_deref(), Some("kanji"));
}

#[test]
fn reading_again_commits_the_first_kana_of_a_reading_as_it_was_taken() {
    let (mut t, out) = read_again("hakanji");
    assert_eq!(out.preedit, "›はかんじ");
    let out = reread(&mut t);
    assert!(out.consumed);
    assert_eq!(
        (out.commit.as_deref(), out.erase, out.preedit.as_str()),
        (Some("は"), None, "›かんじ")
    );
    assert_eq!(t.key(Key::Space).preedit, "»漢字");
}

#[test]
fn kana_typed_together_are_committed_together() {
    let (mut t, _) = read_again("kyaku");
    let out = reread(&mut t);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("きゃ"), "›く")
    );
}

#[test]
fn the_last_kana_stays_in_the_reading() {
    let (mut t, _) = read_again("kya");
    let out = reread(&mut t);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›きゃ"));
}

#[test]
fn a_reading_changed_since_it_was_taken_commits_nothing() {
    let (mut t, _) = read_again("hakanji");
    t.ch('a');
    let out = reread(&mut t);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›はかんじあ"));
    t.key(Key::Backspace);
    t.key(Key::Left);
    let out = reread(&mut t);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›はかん|じ"));
}

#[test]
fn a_reading_not_taken_back_commits_nothing() {
    let mut t = T::new();
    t.kana();
    t.typ(";hakanji");
    let out = reread(&mut t);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›はかんじ"));
}

#[test]
fn cancel_types_the_kana_again() {
    let (mut t, _) = read_again("hakanji");
    let out = t.key(Key::Esc);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("はかんじ"), "")
    );
    assert_eq!(reread(&mut t).erase.as_deref(), Some("はかんじ"));
}

#[test]
fn cancel_types_the_kana_left_and_the_romaji_again() {
    let (mut t, _) = read_again("nihon");
    let out = t.key(Key::Esc);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("にほ"), "n")
    );
    let (mut t, _) = read_again("hakanji");
    reread(&mut t);
    let out = t.key(Key::Esc);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("かんじ"), "")
    );
}

#[test]
fn cancel_among_the_candidates_goes_back_to_the_reading() {
    let (mut t, _) = read_again("kanji");
    t.key(Key::Space);
    let out = t.key(Key::Esc);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›かんじ"));
    let out = t.key(Key::Esc);
    assert_eq!(out.commit.as_deref(), Some("かんじ"));
}

#[test]
fn erasing_the_whole_reading_leaves_nothing() {
    let (mut t, _) = read_again("ka");
    t.key(Key::Backspace);
    let out = t.key(Key::Backspace);
    assert_eq!((out.commit, out.preedit.as_str()), (None, ""));
    assert_eq!(reread(&mut t).erase, None);
}

#[test]
fn a_commit_before_the_kana_can_still_be_undone_without_them() {
    let mut t = T::new();
    t.kana();
    t.typ(";kisha");
    t.key(Key::Space);
    t.key(Key::Enter);
    t.typ("ka");
    reread(&mut t);
    t.handle(Event::Erased(true));
    t.key(Key::Backspace);
    t.key(Key::Backspace);
    assert_eq!(undo(&mut t).erase.as_deref(), Some("記者"));
}

#[test]
fn what_is_converted_from_kana_read_again_follows_the_commit_before_them() {
    let mut t = T::new();
    t.kana();
    t.typ(";kisha");
    t.key(Key::Space);
    t.key(Key::Enter);
    t.typ("kanji");
    reread(&mut t);
    t.handle(Event::Erased(true));
    let out = t.key(Key::Enter);
    assert_eq!(out.commit.as_deref(), Some("かんじ"));
    assert_eq!(undo(&mut t).erase.as_deref(), Some("記者かんじ"));
}

#[test]
fn keys_typed_before_the_host_is_done_wait_for_it() {
    let mut t = typed("kanji");
    reread(&mut t);
    let out = t.ch('a');
    assert!(out.consumed);
    assert_eq!((out.commit, out.preedit.as_str()), (None, ""));
    assert_eq!(t.handle(Event::Erased(true)).preedit, "›かんじあ");
}

#[test]
fn nothing_changes_when_the_host_could_not_erase() {
    let mut t = typed("nihon");
    reread(&mut t);
    let out = t.handle(Event::Erased(false));
    assert_eq!((out.commit, out.preedit.as_str()), (None, "n"));
    assert!(t.converter().erased.is_empty());
    assert_eq!(reread(&mut t).erase, None);
}

#[test]
fn kana_read_again_as_the_mode_leaves_kana_go_back_as_they_were() {
    let mut t = typed("kanji");
    reread(&mut t);
    t.handle(Event::SetMode(Mode::Abc));
    let out = t.handle(Event::Erased(true));
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str(), out.mode),
        (Some("かんじ"), "", Mode::Abc)
    );
}

#[test]
fn a_key_passed_to_the_application_ends_the_run() {
    let mut t = typed("kanji");
    assert!(!t.key(Key::Other).consumed);
    assert_eq!(reread(&mut t).erase, None);
}

#[test]
fn moving_the_focus_or_the_caret_ends_the_run() {
    for event in [Event::FocusOut, Event::Flush, Event::CaretMoved] {
        let mut t = typed("kanji");
        t.handle(event);
        assert_eq!(reread(&mut t).erase, None, "{event:?}");
    }
}

#[test]
fn kana_typed_after_the_caret_moved_start_a_run() {
    let mut t = typed("ky");
    t.handle(Event::CaretMoved);
    t.typ("aku");
    assert_eq!(reread(&mut t).erase.as_deref(), Some("きゃく"));
}

#[test]
fn undoing_a_commit_ends_the_run_after_it() {
    let mut t = T::new();
    t.kana();
    t.typ(";kisha");
    t.key(Key::Space);
    t.key(Key::Enter);
    t.typ("ka");
    undo(&mut t);
    t.handle(Event::Erased(true));
    t.key(Key::Enter);
    assert_eq!(reread(&mut t).erase, None);
}

#[test]
fn leaving_while_the_host_erases_commits_only_what_is_visible() {
    for event in [Event::FocusOut, Event::Flush] {
        let mut t = typed("ahn");
        let out = reread(&mut t);
        assert_eq!(out.erase.as_deref(), Some("あ"));
        assert_eq!(out.preedit, "");
        assert_eq!(t.handle(event).commit, None, "{event:?}");
    }
}
