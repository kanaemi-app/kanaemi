mod common;

use common::*;
use kanaemi_core::{Event, Key, Mode, Modifiers, Output};

fn undo(t: &mut T) -> Output {
    t.press(
        Key::Backspace,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    )
}

/// 記者 committed for `;kisha`, its first candidate.
fn committed() -> T {
    let mut t = T::new();
    t.kana();
    t.typ(";kisha");
    t.key(Key::Space);
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("記者"));
    t
}

#[test]
fn undoing_asks_the_host_to_erase_the_commit_before_choosing_again() {
    let mut t = committed();
    let out = undo(&mut t);
    assert!(out.consumed);
    assert_eq!(out.erase.as_deref(), Some("記者"));
    assert_eq!((out.preedit.as_str(), out.candidates), ("", None));

    let out = t.handle(Event::Erased(true));
    assert_eq!(out.preedit, "»記者");
    assert_eq!(surfaces(&out)[..2], ["記者", "汽車"]);
    assert_eq!(t.key(Key::Space).preedit, "»汽車");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("汽車"));
}

#[test]
fn caps_lock_bound_nowhere_keeps_the_commit_undoable() {
    let mut t = committed();
    t.key(Key::CapsLock);
    assert_eq!(undo(&mut t).erase.as_deref(), Some("記者"));
}

#[test]
fn the_candidate_chosen_is_chosen_again() {
    let mut t = T::new();
    t.kana();
    t.typ(";kisha");
    t.key(Key::Space);
    t.key(Key::Space);
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("汽車"));
    undo(&mut t);
    let out = t.handle(Event::Erased(true));
    assert_eq!(out.preedit, "»汽車");
    assert_eq!(out.candidates.map(|c| c.selected), Some(1));
}

#[test]
fn text_committed_after_the_candidate_goes_back_after_it() {
    let mut t = committed();
    assert_eq!(t.typ("suru").0, "する");
    assert_eq!(undo(&mut t).erase.as_deref(), Some("記者する"));
    assert_eq!(t.handle(Event::Erased(true)).preedit, "»記者する");
    assert_eq!(t.key(Key::Space).preedit, "»汽車する");
    let out = t.key(Key::Enter);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("汽車する"), "")
    );
}

#[test]
fn a_reading_committed_after_the_candidate_is_text_after_it() {
    let mut t = committed();
    t.typ(";kana");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("かな"));
    assert_eq!(undo(&mut t).erase.as_deref(), Some("記者かな"));
}

#[test]
fn only_the_last_candidate_committed_can_be_undone() {
    let mut t = committed();
    t.typ(";kanji");
    t.key(Key::Space);
    t.key(Key::Enter);
    assert_eq!(undo(&mut t).erase.as_deref(), Some("漢字"));
}

#[test]
fn cancel_commits_what_was_there_again() {
    let mut t = committed();
    t.typ("suru");
    undo(&mut t);
    t.handle(Event::Erased(true));
    t.key(Key::Space);
    let out = t.key(Key::Esc);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("記者する"), "")
    );
    assert_eq!(
        t.converter().commits.last(),
        Some(&("きしゃ".to_owned(), "記者".to_owned()))
    );
}

#[test]
fn cancel_in_the_reading_commits_what_was_there_again() {
    let mut t = committed();
    undo(&mut t);
    t.handle(Event::Erased(true));
    assert_eq!(
        t.key(Key::Backspace).preedit,
        "›きしゃ",
        "back to the reading, unlike cancel"
    );
    let out = t.key(Key::Esc);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("記者"), "")
    );
}

#[test]
fn cancel_steps_back_from_a_completion_listed_before_committing_what_was_there() {
    let mut t = committed();
    undo(&mut t);
    t.handle(Event::Erased(true));
    // Back to the reading, then off to き.
    (0..3).for_each(|_| {
        t.key(Key::Backspace);
    });
    assert_eq!(t.key(Key::Tab).preedit, "›きしゃ");
    assert_eq!(t.key(Key::Esc).preedit, "›き");
    let out = t.key(Key::Esc);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("記者"), "")
    );
}

#[test]
fn the_commit_undone_is_withdrawn_once_erased() {
    let mut t = committed();
    t.typ("suru");
    undo(&mut t);
    assert!(t.converter().withdrawn.is_empty());
    t.handle(Event::Erased(true));
    let learned = t.converter();
    assert_eq!(
        learned.withdrawn,
        [("きしゃ".to_owned(), "記者".to_owned())]
    );
    assert_eq!(learned.erased, ["記者する"]);
}

#[test]
fn nothing_changes_when_the_host_could_not_erase() {
    let mut t = committed();
    undo(&mut t);
    let out = t.handle(Event::Erased(false));
    assert_eq!((out.preedit.as_str(), out.commit), ("", None));
    assert!(t.converter().withdrawn.is_empty());
    let out = undo(&mut t);
    assert_eq!((out.consumed, out.erase), (false, None));
}

#[test]
fn a_commit_undone_as_the_mode_leaves_kana_goes_back_as_it_was() {
    let mut t = committed();
    t.typ("suru");
    undo(&mut t);
    t.handle(Event::SetMode(Mode::Abc));
    let out = t.handle(Event::Erased(true));
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str(), out.mode),
        (Some("記者する"), "", Mode::Abc)
    );
    assert_eq!(out.candidates, None);
    let learned = t.converter();
    assert!(learned.withdrawn.is_empty());
    assert_eq!(learned.commits.len(), 1, "committed once");
    assert_eq!(learned.erased, ["記者する"]);
}

#[test]
fn with_nothing_to_undo_the_key_passes_on() {
    let mut t = T::new();
    t.kana();
    let out = undo(&mut t);
    assert_eq!((out.consumed, out.erase), (false, None));
}

#[test]
fn a_reading_committed_as_kana_cannot_be_undone() {
    let mut t = T::new();
    t.kana();
    t.typ(";kana");
    t.key(Key::Enter);
    assert_eq!(undo(&mut t).erase, None);
}

#[test]
fn a_key_passed_to_the_application_ends_what_can_be_undone() {
    let mut t = committed();
    assert!(!t.key(Key::Other).consumed);
    assert_eq!(undo(&mut t).erase, None);
}

#[test]
fn a_modifier_alone_leaves_what_can_be_undone() {
    let mut t = committed();
    t.down(Key::CtrlLeft);
    t.release(Key::CtrlLeft);
    assert_eq!(undo(&mut t).erase.as_deref(), Some("記者"));
}

#[test]
fn moving_the_focus_or_clicking_ends_what_can_be_undone() {
    for event in [Event::FocusOut, Event::Flush] {
        let mut t = committed();
        t.handle(event);
        assert_eq!(undo(&mut t).erase, None, "{event:?}");
    }
}

#[test]
fn keys_typed_before_the_host_is_done_wait_for_it() {
    let mut t = committed();
    undo(&mut t);
    let out = t.ch('a');
    assert!(out.consumed);
    assert_eq!((out.commit, out.preedit.as_str()), (None, ""));
    let out = t.handle(Event::Erased(true));
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("記者あ"), "")
    );
}

#[test]
fn keys_after_an_undo_among_those_waiting_wait_for_its_erasing() {
    let mut t = committed();
    undo(&mut t);
    t.key(Key::Esc);
    undo(&mut t);
    t.ch('a');
    let out = t.handle(Event::Erased(true));
    assert_eq!(
        (out.erase.as_deref(), out.commit.as_deref()),
        (Some("記者"), Some("記者"))
    );
    let out = t.handle(Event::Erased(true));
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("記者あ"), "")
    );
}

#[test]
fn keys_typed_before_the_host_could_not_erase_go_after_the_commit() {
    let mut t = committed();
    undo(&mut t);
    t.ch('a');
    let out = t.handle(Event::Erased(false));
    assert_eq!(out.commit.as_deref(), Some("あ"));
}

#[test]
fn keys_waiting_for_the_host_are_not_sent_as_other_keys() {
    let mut t = committed();
    undo(&mut t);
    t.ctrl('n');
    let out = t.handle(Event::Erased(false));
    assert_eq!(out.send, None);
}

#[test]
fn keys_waiting_for_the_host_leave_what_they_commit_undoable() {
    let mut t = committed();
    undo(&mut t);
    t.key(Key::Enter);
    t.ctrl('n');
    let out = t.handle(Event::Erased(true));
    assert_eq!(out.commit.as_deref(), Some("記者"));
    assert_eq!(undo(&mut t).erase.as_deref(), Some("記者"));
}

#[test]
fn the_text_after_what_is_chosen_again_does_not_follow_the_focus() {
    let mut t = committed();
    t.typ("suru");
    undo(&mut t);
    t.handle(Event::Erased(true));
    let out = t.handle(Event::FocusIn { password: false });
    assert_eq!((out.commit, out.preedit.as_str()), (None, ""));
}

#[test]
fn a_caret_that_may_have_moved_ends_what_can_be_undone() {
    let mut t = committed();
    let out = t.handle(Event::CaretMoved);
    assert_eq!((out.commit, out.preedit.as_str()), (None, ""));
    assert_eq!(undo(&mut t).erase, None);
}

#[test]
fn a_caret_that_may_have_moved_leaves_what_is_typed() {
    let mut t = committed();
    t.typ(";ka");
    let out = t.handle(Event::CaretMoved);
    assert_eq!((out.commit, out.preedit.as_str()), (None, "›か"));
}

#[test]
fn what_is_chosen_again_can_be_undone_again() {
    let mut t = committed();
    t.typ("suru");
    undo(&mut t);
    t.handle(Event::Erased(true));
    t.key(Key::Space);
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("汽車する"));
    assert_eq!(undo(&mut t).erase.as_deref(), Some("汽車する"));
}

#[test]
fn erasing_the_whole_reading_leaves_the_text_after_it() {
    let mut t = committed();
    t.typ("suru");
    undo(&mut t);
    t.handle(Event::Erased(true));
    // Back to the reading, then each of きしゃ.
    (0..3).for_each(|_| {
        t.key(Key::Backspace);
    });
    assert_eq!(t.key(Key::Backspace).preedit, "›する");
    let out = t.key(Key::Backspace);
    assert_eq!(
        (out.commit.as_deref(), out.preedit.as_str()),
        (Some("する"), "")
    );
}

#[test]
fn a_character_typed_while_choosing_again_goes_after_the_text_after_it() {
    let mut t = committed();
    t.typ("suru");
    undo(&mut t);
    t.handle(Event::Erased(true));
    assert_eq!(t.ch('a').commit.as_deref(), Some("記者するあ"));
}

#[test]
fn a_word_registered_and_committed_cannot_be_undone() {
    let mut t = T::new();
    t.kana();
    t.typ(";nunu");
    t.go_past_the_candidates();
    t.typ("nu");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("ぬ"));
    let out = undo(&mut t);
    assert_eq!((out.consumed, out.erase), (false, None));
}
