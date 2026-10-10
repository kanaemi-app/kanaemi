mod common;

use common::*;
use kanaemi_core::{Core, Event, Key, Modifiers};

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
fn completing_lists_the_readings_with_the_one_shown_selected() {
    let mut t = reading("k");
    assert_eq!(t.ch('a').candidates, None, "not before it is asked");
    let out = t.key(Key::Tab);
    assert!(out.consumed);
    assert_eq!(out.commit, None);
    assert_eq!(surfaces(&out), ["かく", "かな", "かんじ"]);
    assert_eq!(out.candidates.unwrap().selected, 0);
    assert_eq!(t.key(Key::Tab).candidates.unwrap().selected, 1);
}

#[test]
fn a_reading_listed_to_complete_with_shows_no_dictionary() {
    let mut t = reading("ka");
    let view = t.key(Key::Tab).candidates.unwrap();
    assert!(view.items.iter().all(|c| c.source.is_none()));
}

#[test]
fn a_reading_listed_to_complete_with_shows_the_candidate_it_converts_to_first() {
    let mut t = reading("ka");
    let view = t.key(Key::Tab).candidates.unwrap();
    let previews: Vec<(&str, Option<&str>)> = view
        .items
        .iter()
        .map(|c| (c.surface.as_str(), c.preview.as_deref()))
        .collect();
    assert_eq!(
        previews,
        [
            ("かく", Some("角")),
            ("かな", Some("カナ")),
            ("かんじ", Some("漢字")),
        ]
    );
}

#[test]
fn a_reading_listed_to_complete_with_that_converts_to_nothing_shows_none() {
    let mut fake = Fake::default();
    fake.table.insert("かお", vec![]);
    fake.table.insert("かく", vec!["角"]);
    let learned = fake.learned.clone();
    let mut t = T {
        core: Core::new(fake, config()),
        learned,
        now: 1_000,
    };
    t.handle(Event::FocusIn { password: false });
    t.kana();
    t.ch(';');
    t.typ("ka");
    let view = t.key(Key::Tab).candidates.unwrap();
    let previews: Vec<(&str, Option<&str>)> = view
        .items
        .iter()
        .map(|c| (c.surface.as_str(), c.preview.as_deref()))
        .collect();
    assert_eq!(previews, [("かお", None), ("かく", Some("角"))]);
    assert_eq!(view.more, Vec::<String>::new());
}

#[test]
fn the_highlighted_reading_to_complete_with_shows_its_other_candidates_in_order() {
    let mut t = reading("ka");
    let view = t.key(Key::Tab).candidates.unwrap();
    assert_eq!(view.more, ["書く", "核"], "かく after 角");
    let view = t.key(Key::Tab).candidates.unwrap();
    assert_eq!(view.more, ["仮名"], "かな after カナ");
}

#[test]
fn the_other_candidates_of_a_reading_are_cut_short_past_a_page() {
    let mut t = reading("ko");
    let view = t.key(Key::Tab).candidates.unwrap();
    assert_eq!(
        view.more,
        ["校", "行", "考", "効", "項", "構", "講", "公"],
        "こう after 高"
    );
}

#[test]
fn a_candidate_forgotten_while_converting_a_completed_reading_is_gone_from_the_list() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    t.key(Key::Space);
    t.forget();
    let view = t.key(Key::Esc).candidates.unwrap();
    assert_eq!(
        view.items[0].preview.as_deref(),
        Some("書く"),
        "角 forgotten"
    );
    assert_eq!(view.more, ["核"]);
}

#[test]
fn candidates_being_chosen_show_no_others() {
    let mut t = T::new();
    t.kana();
    t.typ(";kanji");
    let view = t.key(Key::Space).candidates.unwrap();
    assert_eq!(view.more, Vec::<String>::new());
}

#[test]
fn down_goes_to_the_next_reading_while_the_list_is_shown() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    assert_eq!(t.key(Key::Down).preedit, "›かな");
}

#[test]
fn up_goes_to_the_previous_reading_while_the_list_is_shown() {
    let mut t = reading("ka");
    back_tab(&mut t);
    assert_eq!(t.key(Key::Up).preedit, "›かな");
}

/// In kana mode with `あ` typed after `;`, which completes to twelve
/// readings, `あか` to `あち` in that order: a page and three more.
fn reading_of_two_pages() -> T {
    let mut fake = Fake::default();
    for kana in "かきくけこさしすせそたち".chars() {
        let reading: &'static str = Box::leak(format!("あ{kana}").into_boxed_str());
        fake.table.insert(reading, vec![]);
    }
    let learned = fake.learned.clone();
    let mut t = T {
        core: Core::new(fake, config()),
        learned,
        now: 1_000,
    };
    t.handle(Event::FocusIn { password: false });
    t.kana();
    t.ch(';');
    t.ch('a');
    t
}

#[test]
fn the_list_tells_which_page_of_readings_it_shows_of_how_many() {
    let mut t = reading_of_two_pages();
    let view = t.key(Key::Tab).candidates.unwrap();
    assert_eq!((view.page, view.pages), (0, 2));
    let view = t.ctrl('n').candidates.unwrap();
    assert_eq!((view.page, view.pages), (1, 2));
}

#[test]
fn ctrl_n_goes_to_the_first_reading_of_the_next_page() {
    let mut t = reading_of_two_pages();
    t.key(Key::Tab);
    t.key(Key::Tab);
    let out = t.ctrl('n');
    assert_eq!(out.preedit, "›あそ");
    assert_eq!(out.candidates.unwrap().selected, 0);
}

#[test]
fn ctrl_p_goes_to_the_first_reading_of_the_previous_page() {
    let mut t = reading_of_two_pages();
    back_tab(&mut t);
    assert_eq!(t.ctrl('p').preedit, "›あか");
}

#[test]
fn past_either_end_page_by_page_is_the_reading_as_typed_as_complete_goes() {
    let mut t = reading_of_two_pages();
    t.key(Key::Tab);
    t.ctrl('n');
    let out = t.ctrl('n');
    assert_eq!(out.preedit, "›あ", "past the last page");
    assert_eq!(out.candidates, None);

    let mut t = reading_of_two_pages();
    t.key(Key::Tab);
    let out = t.ctrl('p');
    assert_eq!(out.preedit, "›あ", "before the first page");
    assert_eq!(out.candidates, None);
}

#[test]
fn turning_a_page_while_choosing_turns_the_candidates_page() {
    let mut t = reading("ko");
    t.key(Key::Tab);
    t.key(Key::Space);
    assert_eq!(t.ctrl('n').preedit, "»工");
}

#[test]
fn commit_while_the_list_is_shown_keeps_the_reading_shown_without_the_list() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    let out = t.key(Key::Enter);
    assert!(out.consumed);
    assert_eq!(out.commit, None);
    assert_eq!(out.preedit, "›かく");
    assert_eq!(out.candidates, None);
}

#[test]
fn commit_with_no_list_shown_commits_the_reading_as_kana() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    t.key(Key::Enter);
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("かく"));
}

#[test]
fn escape_goes_back_a_step_at_a_time() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    t.key(Key::Tab);
    assert_eq!(t.key(Key::Space).preedit, "»カナ");
    let out = t.key(Key::Esc);
    assert_eq!(
        out.preedit, "›かな",
        "back to the list, at the reading converted"
    );
    assert_eq!(out.candidates.unwrap().selected, 1);
    let out = t.key(Key::Esc);
    assert_eq!(out.preedit, "›か", "back to the reading as typed");
    assert_eq!(out.candidates, None);
    let out = t.key(Key::Esc);
    assert_eq!(
        (out.preedit.as_str(), out.commit),
        ("", None),
        "nothing typed"
    );
}

#[test]
fn backspace_while_choosing_goes_back_to_the_list_as_escape_does() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    t.key(Key::Space);
    let out = t.key(Key::Backspace);
    assert_eq!(out.preedit, "›かく");
    assert_eq!(out.candidates.unwrap().selected, 0);
}

#[test]
fn escape_while_choosing_a_reading_as_typed_goes_back_to_it_with_no_list() {
    let mut t = reading("kanji");
    t.key(Key::Space);
    let out = t.key(Key::Esc);
    assert_eq!(out.preedit, "›かんじ");
    assert_eq!(out.candidates, None);
}

#[test]
fn space_converts_the_reading_the_list_shows() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    assert_eq!(t.key(Key::Space).preedit, "»角");
}

#[test]
fn ctrl_n_turns_no_page_when_no_list_is_shown() {
    let mut t = reading("kanji");
    let out = t.ctrl('n');
    assert_eq!(out.preedit, "›かんじ");
    assert_eq!(out.candidates, None);
}

#[test]
fn back_at_the_reading_as_typed_no_list_is_shown() {
    let mut t = reading("ka");
    (0..3).for_each(|_| {
        t.key(Key::Tab);
    });
    let out = t.key(Key::Tab);
    assert_eq!(out.preedit, "›か");
    assert_eq!(out.candidates, None);
}

#[test]
fn a_reading_of_the_list_is_picked_by_the_keys_that_pick_a_candidate() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    let out = t.ch('3');
    assert_eq!(out.preedit, "›かんじ");
    assert_eq!(out.candidates.unwrap().selected, 2, "still completing");
    assert_eq!(t.key(Key::Tab).preedit, "›か", "going on from there");
}

#[test]
fn a_reading_of_the_list_is_picked_by_selecting_it() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    let out = t.handle(Event::Select(1));
    assert_eq!(out.preedit, "›かな");
    assert_eq!(out.candidates.unwrap().selected, 1);
}

#[test]
fn a_number_the_list_has_no_reading_for_does_nothing() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    let out = t.ch('9');
    assert!(out.consumed);
    assert_eq!(out.preedit, "›かく");
    assert_eq!(out.candidates.unwrap().selected, 0, "still listed");
    assert_eq!(t.key(Key::Tab).preedit, "›かな", "still completing");
}

#[test]
fn a_click_that_may_move_the_caret_leaves_the_list_to_pick_from() {
    let mut t = reading("ka");
    t.key(Key::Tab);
    t.handle(Event::CaretMoved);
    assert_eq!(t.handle(Event::Select(2)).preedit, "›かんじ");
}

#[test]
fn a_key_that_picks_types_as_it_does_with_no_list_shown() {
    let mut t = reading("ka");
    assert_eq!(t.ch('3').preedit, "›か３");
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
