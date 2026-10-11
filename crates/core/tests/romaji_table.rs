mod common;

use common::*;
use kanaemi_core::{Config, Event, Key, RomajiTable};
use proptest::collection::vec;
use proptest::prelude::*;

#[test]
fn romaji_is_typed_by_the_longest_rule_and_the_ways_around_the_rules() {
    let cases = [
        ("ka", "か"),
        ("kya", "きゃ"),
        ("kka", "っか"),
        ("nka", "んか"),
        ("n'a", "んあ"),
        ("qa", "あ"),
        ("-", "ー"),
    ];
    for (input, want) in cases {
        let mut t = T::new();
        t.kana();
        assert_eq!(t.typ(input).0, want, "romaji {input}");
    }
}

#[test]
fn symbols_are_typed_through_the_table_like_letters() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ("1@").0, "１＠");
}

#[test]
fn symbols_are_typed_through_the_table_in_a_reading_too() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("ka1");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("か１"));
}

#[test]
fn a_shifted_symbol_goes_through_the_table_like_any_other() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.shifted(Key::Char('!')).commit.as_deref(), Some("！"));
}

#[test]
fn a_rule_can_take_uppercase_input() {
    assert_eq!(typed_with(table(&["Ka\tカ"]), "Ka"), "カ");
}

#[test]
fn an_uppercase_letter_no_rule_takes_is_typed_as_it_is() {
    assert_eq!(typed_with(romaji(), "Aka"), "Aか");
}

#[test]
fn a_romaji_rule_can_be_changed() {
    let mut config = config();
    config.romaji.apply("1\t1");
    assert_eq!(typed_with(config.romaji, "12"), "1２");
}

#[test]
fn a_romaji_rule_can_be_removed() {
    let mut config = config();
    config.romaji.apply("!1");
    assert_eq!(typed_with(config.romaji, "12"), "1２");
}

#[test]
fn later_rules_override_earlier_ones() {
    assert_eq!(typed_with(table(&["kz\tかん\nkz\tけん"]), "kz"), "けん");
    assert_eq!(typed_with(table(&["kz\tかん", "kz\tけん"]), "kz"), "けん");
}

#[test]
fn a_rule_removed_by_a_later_line_comes_back_when_added_again() {
    assert_eq!(typed_with(table(&["a\tあ", "!a", "a\tア"]), "a"), "ア");
}

#[test]
fn romaji_table_files_skip_blank_and_comment_lines() {
    assert_eq!(typed_with(table(&["# vowels\n\na\tあ\r\n"]), "a"), "あ");
}

#[test]
fn comment_lines_after_the_description_are_skipped_too() {
    assert_eq!(
        typed_with(
            table(&["# vowels\n# from somewhere\na\tあ\n# more\ni\tい"]),
            "ai"
        ),
        "あい"
    );
}

#[test]
fn romaji_table_escapes() {
    let t = table(&["\\#\t＃\n\\!\t！\n\\\\\t＼"]);
    assert_eq!(typed_with(t, "#!\\"), "＃！＼");
}

#[test]
fn invalid_romaji_table_lines_are_skipped_and_reported() {
    let mut t = RomajiTable::empty();
    let text = "a\tあ\nb\nc\t\n\td\ne\tえ\tx\n\\q\tq\nfあ\tふ\n!\ni\tい";
    assert_eq!(t.apply(text), vec![2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(typed_with(t, "ai"), "あい");
}

fn with_rules(rules: &str) -> T {
    let mut romaji = romaji();
    assert_eq!(romaji.apply(rules), Vec::<usize>::new());
    let mut t = T::with_config(Config { romaji, ..config() });
    t.kana();
    t
}

#[test]
fn the_space_key_types_what_a_rule_taking_a_space_makes() {
    let mut t = with_rules(" \t　");
    let out = t.key(Key::Space);
    assert!(out.consumed);
    assert_eq!(out.commit.as_deref(), Some("　"));
}

#[test]
fn a_key_going_on_with_unfinished_romaji_into_a_rule_is_typed_before_its_binding() {
    let mut t = with_rules("z \t・");
    assert_eq!(t.ch('z').commit, None);
    assert_eq!(t.key(Key::Space).commit.as_deref(), Some("・"));

    let mut t = with_rules("z \t・");
    t.ch(';');
    t.typ("kaz");
    assert_eq!(t.key(Key::Space).preedit, "›か・", "in a reading too");
}

#[test]
fn a_key_going_on_with_unfinished_romaji_into_no_rule_does_what_it_is_bound_to() {
    let mut t = with_rules("z \t・");
    t.ch(';');
    t.typ("kanjik");
    assert_eq!(t.key(Key::Space).preedit, "»漢字");
}

#[test]
fn without_a_rule_taking_a_space_the_space_key_passes_on() {
    let mut t = T::new();
    t.kana();
    let out = t.key(Key::Space);
    assert!(!out.consumed);
    assert_eq!(out.commit, None);

    let mut t = with_rules(" \t　\n! ");
    assert!(!t.key(Key::Space).consumed, "the rule was removed");
}

#[test]
fn a_space_bound_to_a_function_acts_before_a_rule_taking_a_space() {
    let mut t = with_rules(" \t　");
    t.ch(';');
    t.typ("kanji");
    assert_eq!(t.key(Key::Space).preedit, "»漢字");
}

#[test]
fn a_table_stacked_later_wins() {
    let t = table(&["1\t１\n,\t，", ",\t、"]);
    assert_eq!(typed_with(t, ",1"), "、１");
}

#[test]
fn unfinished_romaji_is_dropped_on_commit_except_a_trailing_n() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kan");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("かん"));
}

#[test]
fn a_rule_that_another_rule_extends_is_used_when_the_longer_one_does_not_come() {
    let mut romaji = romaji();
    romaji.apply("ab\tX");
    assert_eq!(typed_with(romaji.clone(), "aka"), "あか");
    assert_eq!(typed_with(romaji.clone(), "ab"), "X");

    let mut t = T::with_config(Config { romaji, ..config() });
    t.kana();
    t.ch(';');
    t.typ("ka");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("か"));
}

#[test]
fn unfinished_romaji_at_a_commit_uses_the_longest_rule_and_drops_letters_forming_none() {
    let mut t = T::with_config(Config {
        romaji: table(&["a\tあ\nabc\tX"]),
        keep_unfinished_romaji: false,
        ..config()
    });
    t.kana();
    t.typ("ab");
    assert_eq!(t.handle(Event::Flush).commit.as_deref(), Some("あ"));
}

#[test]
fn unfinished_romaji_at_a_commit_goes_on_after_the_longest_rule() {
    let mut t = T::with_config(Config {
        romaji: table(&["a\tあ\nb\tい\nabc\tX"]),
        ..config()
    });
    t.kana();
    t.typ("ab");
    assert_eq!(t.handle(Event::Flush).commit.as_deref(), Some("あい"));
}

#[test]
fn an_n_followed_by_nothing_forming_a_rule_at_a_commit_is_n() {
    let mut t = T::with_config(Config {
        romaji: table(&["nbc\tX"]),
        keep_unfinished_romaji: false,
        ..config()
    });
    t.kana();
    t.typ("nb");
    assert_eq!(t.handle(Event::Flush).commit.as_deref(), Some("ん"));
}

#[test]
fn unfinished_romaji_in_a_reading_keeps_what_it_forms_at_a_commit() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kony");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("こん"));
}

#[test]
fn the_ways_around_the_rules_work_whichever_rules_the_table_has() {
    let only_ka = || table(&["ka\tか"]);
    assert_eq!(typed_with(only_ka(), "nka"), "んか");
    assert_eq!(typed_with(only_ka(), "kka"), "っか");
    assert_eq!(typed_with(only_ka(), "tta"), "っ");
    assert_eq!(typed_with(romaji(), "qqa"), "っあ");
}

#[test]
fn a_doubled_vowel_or_n_is_not_a_small_tsu() {
    let only_ka = || table(&["ka\tか"]);
    assert_eq!(typed_with(only_ka(), "aaka"), "か");
    assert_eq!(typed_with(only_ka(), "nnka"), "んんか");
}

#[test]
fn an_n_left_at_a_commit_is_n_whichever_rules_the_table_has() {
    let mut t = T::with_config(Config {
        romaji: table(&["ka\tか"]),
        ..config()
    });
    t.kana();
    t.typ("n");
    assert_eq!(t.handle(Event::Flush).commit.as_deref(), Some("ん"));
}

#[test]
fn a_symbol_forming_no_rule_is_typed_as_it_is() {
    assert_eq!(typed_with(table(&["z-\t〜"]), "-"), "-");
    assert_eq!(typed_with(table(&["z-\t〜"]), "z-"), "〜");
}

#[test]
fn a_doubled_symbol_is_not_a_small_tsu() {
    assert_eq!(typed_with(table(&[".a\tX"]), "..a"), ".X");
}

#[test]
fn a_symbol_left_at_a_commit_is_typed_as_it_is() {
    let mut t = T::with_config(Config {
        romaji: table(&["-a\tX"]),
        ..config()
    });
    t.kana();
    t.typ("-");
    assert_eq!(t.handle(Event::Flush).commit.as_deref(), Some("-"));
}

#[test]
fn unfinished_romaji_outside_a_reading_is_committed_as_typed() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ("arew").0, "あれ");
    let out = t.key(Key::Enter);
    assert_eq!(out.commit.as_deref(), Some("w"));
    assert!(!out.consumed, "enter still reaches the application");
}

#[test]
fn unfinished_romaji_outside_a_reading_keeps_what_rules_form_and_the_rest_as_typed() {
    let mut t = T::with_config(Config {
        romaji: table(&["a\tあ\nabc\tX"]),
        ..config()
    });
    t.kana();
    t.typ("ab");
    assert_eq!(t.handle(Event::Flush).commit.as_deref(), Some("あb"));

    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ("tt").0, "っ");
    assert_eq!(t.handle(Event::Flush).commit.as_deref(), Some("t"));
}

#[test]
fn an_n_left_outside_a_reading_is_still_n() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ("kan").0, "か");
    assert_eq!(t.handle(Event::Flush).commit.as_deref(), Some("ん"));
}

#[test]
fn unfinished_romaji_is_kept_before_a_character_off_the_table_and_a_new_reading() {
    let mut t = T::new();
    t.kana();
    assert_eq!(t.typ("arewA").0, "あれwA");

    let mut t = T::new();
    t.kana();
    t.typ("arew");
    assert_eq!(t.ch(';').commit.as_deref(), Some("w"));
}

#[test]
fn unfinished_romaji_in_a_reading_is_still_dropped() {
    let mut t = T::new();
    t.kana();
    t.ch(';');
    t.typ("kanj");
    assert_eq!(t.key(Key::Enter).commit.as_deref(), Some("かん"));
}

#[test]
fn unfinished_romaji_can_be_dropped_outside_a_reading_too() {
    let mut t = T::with_config(Config {
        keep_unfinished_romaji: false,
        ..config()
    });
    t.kana();
    t.typ("arew");
    assert_eq!(t.key(Key::Enter).commit, None);
}

// Robustness: whatever a table file holds, applying it never panics, and
// typing with what it made never does either.

/// Pieces of a table line that the format gives a meaning to.
#[rustfmt::skip]
const PIECES: &[&str] = &[
    // Input and output.
    "k", "a", "n", "ka", "nn", "A", "1", ";", "~", "か", "ん", "っ", "ー", " ",
    // Separators, marks and escapes.
    "\t", "\r", "!", "#", "\\", "\\\\", "\\#", "\\!", "\\t", "\u{FEFF}", "\u{7F}",
];

fn table_text() -> impl Strategy<Value = String> {
    let line = prop_oneof![
        9 => vec(proptest::sample::select(PIECES), 0..6).prop_map(|pieces| pieces.concat()),
        1 => any::<String>(),
    ];
    vec(line, 0..12).prop_map(|lines| lines.join("\n"))
}

proptest! {
    #[test]
    fn any_table_file_applies_and_types_without_panicking(
        text in table_text(),
        input in "[a-zA-Z0-9;:'~!@ -]{1,12}",
    ) {
        let mut table = romaji();
        let invalid = table.apply(&text);
        let lines = text.split('\n').count();
        prop_assert!(invalid.windows(2).all(|pair| pair[0] < pair[1]));
        prop_assert!(invalid.iter().all(|line| (1..=lines).contains(line)));

        let mut t = T::with_config(Config { romaji: table, ..config() });
        t.kana();
        t.typ(&input);
        t.key(Key::Enter);
    }
}
