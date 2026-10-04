use std::fs;
use std::path::{Path, PathBuf};

use kanaemi_config::{
    APPLICATION_TABLE, DictionarySource, FILE_NAME, ProblemKind, Settings, TEMPLATE, UNBOUND,
    actions, binding_tables, bindings_table, default_romaji_table, format_action, key_names,
    parse_action, parse_binding_key, parse_chord, read_or_create, read_romaji_table, sendable_keys,
};
use kanaemi_core::{Action, Binding, Chord, Config, Gesture, Key, Modifiers, Remap, RomajiTable};

fn load(text: &str) -> (Settings, Vec<String>) {
    load_in(text, Path::new("/nonexistent"))
}

fn load_in(text: &str, dir: &Path) -> (Settings, Vec<String>) {
    let (settings, problems) = Settings::load(text, dir);
    (settings, problems.into_iter().map(|p| p.item).collect())
}

/// The configuration read from a file that writes nothing.
fn defaults() -> Config {
    Config {
        romaji: default_romaji_table(),
        ..Config::default()
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("kanaemi-config-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn ctrl(c: char) -> Chord {
    Chord {
        key: Key::Char(c),
        mods: Modifiers {
            ctrl: true,
            ..Default::default()
        },
    }
}

fn plain(key: Key) -> Chord {
    Chord {
        key,
        mods: Modifiers::default(),
    }
}

fn target(bindings: &[Binding], from: Chord) -> Option<Action> {
    bindings
        .iter()
        .find(|b| b.from == from && b.gesture == Gesture::Press)
        .map(|b| b.to)
}

fn tapped(bindings: &[Binding], key: Key) -> Option<Action> {
    bindings
        .iter()
        .find(|b| b.from == plain(key) && b.gesture == Gesture::Tap)
        .map(|b| b.to)
}

fn sent(remaps: &[Remap], from: Chord) -> Option<Chord> {
    remaps.iter().find(|b| b.from == from).map(|b| b.to)
}

#[test]
fn an_empty_file_gives_the_defaults() {
    let (settings, problems) = load("");
    assert_eq!(problems, Vec::<String>::new());
    assert_eq!(settings.config, defaults());
    assert_eq!(settings.dictionaries, None);
    assert_eq!(settings.romaji_tables, None);
}

#[test]
fn marks_can_be_changed_one_by_one() {
    let (settings, problems) = load("[marks]\nreading = \"▽\"\ncursor = \"_\"");
    assert_eq!(problems, Vec::<String>::new());
    assert_eq!(settings.config.marks.reading, "▽");
    assert_eq!(settings.config.marks.cursor, "_");
    assert_eq!(
        settings.config.marks.candidate,
        Config::default().marks.candidate
    );
}

#[test]
fn a_mark_must_be_a_non_empty_line() {
    let (settings, problems) = load("[marks]\nreading = \"\"\ncandidate = \"a\\tb\"");
    assert_eq!(problems, ["marks.reading", "marks.candidate"]);
    assert_eq!(settings.config.marks, Config::default().marks);
}

#[test]
fn the_mode_indicator_can_be_turned_off() {
    let (settings, problems) = load("mode_indicator = false");
    assert_eq!(problems, Vec::<String>::new());
    assert!(!settings.config.mode_indicator);
    let (settings, problems) = load("mode_indicator = \"no\"");
    assert_eq!(problems, ["mode_indicator"]);
    assert!(settings.config.mode_indicator);
}

#[test]
fn romaji_tables_stack_bundled_tables_and_files() {
    let dir = temp_dir("romaji");
    fs::create_dir_all(dir.join("romaji")).unwrap();
    fs::write(dir.join("romaji").join("mine.tsv"), "qq\tくく\n").unwrap();
    let (settings, problems) = load_in("[romaji]\ntables = [\"hepburn\", \"mine.tsv\"]", &dir);
    assert_eq!(problems, Vec::<String>::new());
    let mut expected = RomajiTable::empty();
    expected.apply(read_romaji_table(&dir, "hepburn").unwrap());
    expected.apply("qq\tくく\n");
    assert_eq!(settings.config.romaji, expected);
    assert_eq!(
        settings.romaji_tables.as_deref(),
        Some(&["hepburn".to_owned(), "mine.tsv".to_owned()][..])
    );
}

#[test]
fn an_unknown_or_unreadable_romaji_table_is_skipped() {
    let (settings, problems) = load("[romaji]\ntables = [\"hepburn\", \"nowhere.tsv\"]");
    assert_eq!(problems, ["romaji.tables"]);
    assert_ne!(settings.config.romaji, RomajiTable::empty());
}

#[test]
fn bindings_are_changed_added_and_removed() {
    let (settings, problems) = load(
        "[keys.reading]\n\"ctrl+r\" = \"@register\"\n\"ctrl+n\" = \"@none\"\n\"ctrl+h\" = \"@delete\"\n\"space\" = \"@commit\"",
    );
    assert_eq!(problems, Vec::<String>::new());
    let reading = &settings.config.bindings.reading;
    assert_eq!(target(reading, ctrl('r')), Some(Action::Register));
    assert_eq!(target(reading, ctrl('n')), None);
    assert_eq!(target(reading, ctrl('h')), Some(Action::Delete));
    assert_eq!(target(reading, plain(Key::Space)), Some(Action::Commit));
    assert_eq!(
        settings.config.bindings.candidates,
        Config::default().bindings.candidates
    );
}

#[test]
fn a_binding_that_cannot_be_read_is_skipped() {
    let (_, problems) = load("[keys.reading]\n\"ctrl+\" = \"@commit\"\n\"ctrl+x\" = \"@jump\"");
    assert_eq!(problems, ["keys.reading.ctrl+", "keys.reading.ctrl+x"]);
}

#[test]
fn modifiers_that_commit_and_pass_while_composing() {
    let (settings, problems) = load("[keys]\npass_while_composing = [\"cmd\", \"alt\"]");
    assert_eq!(problems, Vec::<String>::new());
    let pass = settings.config.pass_while_composing;
    assert_eq!((pass.cmd, pass.ctrl, pass.alt), (true, false, true));
}

#[test]
fn dictionaries_are_listed_in_order() {
    let dir = Path::new("/base");
    let (settings, problems) = load_in("dictionaries = [\"a.tsv\", \"custom\", \"b.tsv\"]", dir);
    assert_eq!(problems, Vec::<String>::new());
    assert_eq!(
        settings.dictionaries,
        Some(vec![
            DictionarySource::File(dir.join("dictionaries").join("a.tsv")),
            DictionarySource::UserCustom,
            DictionarySource::File(dir.join("dictionaries").join("b.tsv")),
        ])
    );
}

#[test]
fn the_user_custom_dictionary_goes_first_when_not_listed() {
    let (settings, _) = load("dictionaries = [\"a.tsv\"]");
    assert_eq!(
        settings.dictionaries.unwrap()[0],
        DictionarySource::UserCustom
    );
}

#[test]
fn a_file_that_is_not_toml_gives_the_defaults() {
    let (settings, problems) = load("[marks\nreading = 1");
    assert_eq!(problems, ["config.toml"]);
    assert_eq!(settings.config, defaults());
}

#[test]
fn unknown_items_are_reported() {
    let (_, problems) = load("colour = \"red\"\n[marks]\nshape = \"x\"");
    assert_eq!(problems, ["colour", "marks.shape"]);
}

#[test]
fn chords() {
    assert_eq!(parse_chord("ctrl+h"), Some(ctrl('h')));
    assert_eq!(
        parse_chord("shift+space"),
        Some(Chord {
            key: Key::Space,
            mods: Modifiers {
                shift: true,
                ..Default::default()
            }
        })
    );
    assert_eq!(parse_chord("f12"), Some(plain(Key::F(12))));
    assert_eq!(parse_chord(";"), Some(plain(Key::Char(';'))));
    assert_eq!(parse_chord("ctrl+"), None);
    assert_eq!(parse_chord("hyper+a"), None);
    assert_eq!(parse_chord("ab"), None);
}

/// The template with every setting line uncommented.
fn uncommented_template() -> String {
    kanaemi_config::TEMPLATE
        .lines()
        .map(|line| match line.strip_prefix('#') {
            Some(rest) if !rest.starts_with([' ', '#']) && !rest.is_empty() => rest,
            _ => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_template_changes_nothing_as_it_is() {
    let (settings, problems) = load(kanaemi_config::TEMPLATE);
    assert_eq!(problems, Vec::<String>::new());
    assert_eq!(settings.config, defaults());
}

#[test]
fn the_template_states_the_defaults() {
    let (settings, problems) = load(&uncommented_template());
    assert_eq!(problems, Vec::<String>::new());
    assert_eq!(settings.config, defaults());
}

#[test]
fn actions_that_are_not_keys_are_written_with_an_at_sign() {
    let (_, problems) = load("[keys.reading]\n\"ctrl+r\" = \"register\"");
    assert_eq!(problems, ["keys.reading.ctrl+r"]);
    assert_eq!(parse_chord("@register"), None, "not a key to press");
    let (_, problems) = load("[keys.reading]\n\"ctrl+n\" = \"none\"");
    assert_eq!(problems, ["keys.reading.ctrl+n"], "none is written @none");
}

#[test]
fn a_key_used_while_typing_does_something_rather_than_stand_for_a_key() {
    let (_, problems) = load("[keys.candidates]\n\"x\" = \"shift+space\"");
    assert_eq!(
        problems,
        ["keys.candidates.x"],
        "previous is written @previous"
    );
}

#[test]
fn an_action_works_only_where_it_means_something() {
    let (settings, problems) = load("[keys.reading]\n\"ctrl+k\" = \"@forget\"");
    assert_eq!(problems, ["keys.reading.ctrl+k"]);
    assert_eq!(target(&settings.config.bindings.reading, ctrl('k')), None);
}

#[test]
fn actions_are_written_with_an_at_sign() {
    for action in binding_tables().into_iter().flat_map(actions) {
        assert_eq!(parse_action(format_action(action)), Some(action));
    }
    assert_eq!(format_action(Action::Next), "@next");
    assert_eq!(parse_action("next"), None);
}

#[test]
fn the_plus_key_can_be_written() {
    assert_eq!(parse_chord("+"), Some(plain(Key::Char('+'))));
    assert_eq!(parse_chord("ctrl++"), Some(ctrl('+')));
}

#[test]
fn keys_sent_to_the_application_must_be_named_keys() {
    let (settings, problems) = load(
        "[keys.application]\n\"ctrl+x\" = \"ctrl+c\"\n\"ctrl+y\" = \"f5\"\n\"ctrl+k\" = \"shift+end\"",
    );
    assert_eq!(
        problems,
        ["keys.application.ctrl+x", "keys.application.ctrl+y"]
    );
    let application = &settings.config.bindings.application;
    assert_eq!(sent(application, ctrl('x')), None);
    assert_eq!(
        sent(application, ctrl('k')),
        Some(Chord {
            key: Key::End,
            mods: Modifiers {
                shift: true,
                ..Default::default()
            }
        })
    );
}

#[test]
fn a_shifted_letter_is_written_lower_case_but_matches_the_capital() {
    assert_eq!(
        parse_chord("shift+a"),
        Some(Chord {
            key: Key::Char('A'),
            mods: Modifiers {
                shift: true,
                ..Default::default()
            }
        })
    );
}

#[test]
fn only_keys_a_host_can_send_go_to_the_application() {
    let (_, problems) = load("[keys.application]\n\"ctrl+h\" = \"left-shift\"");
    assert_eq!(problems, ["keys.application.ctrl+h"]);
}

#[test]
fn a_modifier_key_cannot_be_sent_to_the_application_as_another() {
    for key in ["left-shift", "right-cmd", "left-alt"] {
        let text = format!("[keys.application]\n\"{key}\" = \"enter\"");
        let (_, problems) = Settings::load(&text, Path::new("/nonexistent"));
        let kinds: Vec<_> = problems.into_iter().map(|p| p.kind).collect();
        assert_eq!(kinds, [ProblemKind::ModifierKey], "{key}");
    }
    let (_, problems) = load("[keys.application]\n\"left-shift#tap\" = \"enter\"");
    assert_eq!(problems, ["keys.application.left-shift#tap"]);
}

#[test]
fn a_tap_is_written_after_a_modifier_key_pressed_alone() {
    assert_eq!(
        parse_binding_key("left-shift#tap"),
        Some((plain(Key::ShiftLeft), Gesture::Tap))
    );
    assert_eq!(
        parse_binding_key("left-shift"),
        Some((plain(Key::ShiftLeft), Gesture::Press))
    );
    assert_eq!(
        parse_binding_key("ctrl+h"),
        Some((ctrl('h'), Gesture::Press))
    );
    assert_eq!(
        parse_binding_key("#"),
        Some((plain(Key::Char('#')), Gesture::Press))
    );
    for text in [
        "space#tap",
        "a#tap",
        "ctrl+left-shift#tap",
        "left-shift#hold",
        "left-shift#",
        "#tap",
    ] {
        assert_eq!(parse_binding_key(text), None, "{text}");
    }
}

#[test]
fn a_hold_is_written_after_a_key_that_types_a_character() {
    for (text, key) in [
        ("space#hold", Key::Space),
        (";#hold", Key::Char(';')),
        ("a#hold", Key::Char('a')),
        ("##hold", Key::Char('#')),
    ] {
        assert_eq!(
            parse_binding_key(text),
            Some((plain(key), Gesture::Hold)),
            "{text}"
        );
    }
    assert_eq!(
        parse_binding_key("shift+a#hold").map(|(c, g)| (c.key, g)),
        Some((Key::Char('A'), Gesture::Hold))
    );
    for text in [
        "henkan#hold",
        "enter#hold",
        "ctrl+a#hold",
        "cmd+space#hold",
        "left-shift#hold",
        "#hold",
    ] {
        assert_eq!(parse_binding_key(text), None, "{text}");
    }
}

#[test]
fn a_hold_is_bound_beside_the_key_pressed_alone() {
    let (settings, problems) = load("[keys.reading]\n\"space#hold\" = \"@begin\"");
    assert_eq!(problems, Vec::<String>::new());
    let reading = &settings.config.bindings.reading;
    let held = reading
        .iter()
        .find(|b| b.from == plain(Key::Space) && b.gesture == Gesture::Hold)
        .map(|b| b.to);
    assert_eq!(held, Some(Action::Begin));
    assert_eq!(target(reading, plain(Key::Space)), Some(Action::Next));
    let written = bindings_table(&settings.config.bindings, "reading");
    assert!(written.contains(&("space#hold".to_owned(), "@begin".to_owned())));
}

#[test]
fn a_modifier_key_is_bound_to_its_press_and_to_its_tap_apart() {
    let (settings, problems) =
        load("[keys.kana]\n\"left-shift\" = \"@kana\"\n\"left-shift#tap\" = \"@none\"");
    assert_eq!(problems, Vec::<String>::new());
    let kana = &settings.config.bindings.kana;
    assert_eq!(target(kana, plain(Key::ShiftLeft)), Some(Action::Kana));
    assert_eq!(tapped(kana, Key::ShiftLeft), None);
}

#[test]
fn how_long_a_tap_may_last_is_a_number_of_milliseconds() {
    let (settings, problems) = load("[keys]\ntap_timeout_ms = 500");
    assert_eq!(problems, Vec::<String>::new());
    assert_eq!(settings.config.tap_timeout_ms, 500);
    for value in ["0", "-1", "\"500\"", "1.5"] {
        let (settings, problems) = load(&format!("[keys]\ntap_timeout_ms = {value}"));
        assert_eq!(problems, ["keys.tap_timeout_ms"], "{value}");
        assert_eq!(
            settings.config.tap_timeout_ms,
            defaults().tap_timeout_ms,
            "{value}"
        );
    }
}

#[test]
fn a_missing_file_is_created_from_the_template() {
    let dir = temp_dir("create").join("nested");
    assert_eq!(read_or_create(&dir).unwrap(), TEMPLATE);
    assert_eq!(fs::read_to_string(dir.join(FILE_NAME)).unwrap(), TEMPLATE);
}

#[test]
fn an_existing_file_is_read_and_kept() {
    let dir = temp_dir("existing");
    fs::write(dir.join(FILE_NAME), "mode_indicator = false\n").unwrap();
    assert_eq!(read_or_create(&dir).unwrap(), "mode_indicator = false\n");
}

#[test]
fn chords_are_written_back_the_way_they_are_read() {
    for text in [
        "ctrl+h",
        "shift+space",
        "ctrl+cmd+alt+shift+left",
        "f12",
        ";",
        "+",
        "ctrl++",
        "right-shift",
        "right-shift#tap",
        "ctrl+right-shift",
        "shift+a",
        "#",
    ] {
        let (settings, problems) = load(&format!("[keys.reading]\n{text:?} = \"@commit\""));
        assert_eq!(problems, Vec::<String>::new(), "{text}");

        let written = bindings_table(&settings.config.bindings, "reading");

        assert!(
            written.contains(&(text.to_owned(), "@commit".to_owned())),
            "{text}: {written:?}"
        );
    }
}

#[test]
fn the_bindings_tables_are_written_as_the_file_writes_them() {
    let bindings = Config::default().bindings;

    let reading = bindings_table(&bindings, "reading");
    let application = bindings_table(&bindings, APPLICATION_TABLE);

    assert!(reading.contains(&("ctrl+n".to_owned(), "@next".to_owned())));
    assert!(application.contains(&("ctrl+h".to_owned(), "backspace".to_owned())));
    assert_eq!(bindings_table(&bindings, "nowhere"), []);
}

#[test]
fn each_table_offers_only_the_actions_that_work_there() {
    assert!(actions("candidates").contains(&Action::Forget));
    assert!(!actions("reading").contains(&Action::Forget));
    assert_eq!(actions("kana"), [Action::Abc, Action::Kana, Action::Begin]);
    assert_eq!(actions("abc"), [Action::Abc, Action::Kana]);
    assert_eq!(actions(APPLICATION_TABLE), []);
}

#[test]
fn a_problem_says_what_kind_it_is() {
    let (_, problems) = Settings::load(
        "[keys.reading]\n\"nowhere\" = \"@next\"",
        Path::new("/nonexistent"),
    );
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].item, "keys.reading.nowhere");
    assert_eq!(
        problems[0].kind,
        ProblemKind::UnknownKey("nowhere".to_owned())
    );
}

#[test]
fn keys_can_switch_modes_while_nothing_is_typed() {
    let (settings, problems) = load(
        "[keys.kana]\n\"l\" = \"@abc\"\n[keys.abc]\n\"ctrl+j\" = \"@kana\"\n\"x\" = \"@commit\"",
    );
    assert_eq!(problems, ["keys.abc.x"]);
    let bindings = &settings.config.bindings;
    assert_eq!(
        target(&bindings.kana, plain(Key::Char('l'))),
        Some(Action::Abc)
    );
    assert_eq!(target(&bindings.abc, ctrl('j')), Some(Action::Kana));
}

#[test]
fn shift_taps_and_the_jis_keys_are_bound_like_other_keys() {
    let (settings, problems) =
        load("[keys.reading]\n\"right-shift#tap\" = \"@next\"\n[keys.kana]\n\"eisu\" = \"@none\"");
    assert_eq!(problems, Vec::<String>::new());
    let bindings = &settings.config.bindings;
    assert_eq!(
        tapped(&bindings.reading, Key::ShiftRight),
        Some(Action::Next)
    );
    assert_eq!(target(&bindings.kana, plain(Key::Eisu)), None);
}

#[test]
fn begin_is_bound_to_semicolon_where_a_reading_can_be_started_or_marked() {
    let bindings = defaults().bindings;
    for (table, scene) in [
        ("kana", &bindings.kana),
        ("reading", &bindings.reading),
        ("candidates", &bindings.candidates),
        ("registration", &bindings.registration),
    ] {
        assert_eq!(
            target(scene, plain(Key::Char(';'))),
            Some(Action::Begin),
            "{table}"
        );
    }
    assert_eq!(target(&bindings.abc, plain(Key::Char(';'))), None);
}

#[test]
fn begin_is_written_at_begin_and_cannot_be_bound_in_abc_mode() {
    assert_eq!(format_action(Action::Begin), "@begin");
    let (settings, problems) =
        load("[keys.kana]\n\";\" = \"@none\"\n\"q\" = \"@begin\"\n[keys.abc]\n\"q\" = \"@begin\"");
    assert_eq!(problems, ["keys.abc.q"]);
    let kana = &settings.config.bindings.kana;
    assert_eq!(target(kana, plain(Key::Char('q'))), Some(Action::Begin));
    assert_eq!(target(kana, plain(Key::Char(';'))), None);
}

#[test]
fn picking_a_candidate_by_number_is_an_action() {
    let (settings, problems) = load("[keys.candidates]\n\"a\" = \"@select-1\"\n\"1\" = \"@none\"");
    assert_eq!(problems, Vec::<String>::new());
    let candidates = &settings.config.bindings.candidates;
    assert_eq!(
        target(candidates, plain(Key::Char('a'))),
        Some(Action::Pick(0))
    );
    assert_eq!(target(candidates, plain(Key::Char('1'))), None);
    let (_, problems) = load("[keys.candidates]\n\"a\" = \"@select-10\"");
    assert_eq!(problems, ["keys.candidates.a"]);
}

#[test]
fn every_named_key_can_be_listed_and_read() {
    let names = key_names();
    for name in ["left-shift", "eisu", "henkan", "space", "f1", "f12"] {
        assert!(names.contains(&name.to_owned()), "{name}");
    }
    for name in &names {
        assert!(parse_chord(name).is_some(), "{name}");
    }
}

#[test]
fn every_modifier_tap_has_a_name_on_each_side() {
    let (settings, problems) = load(
        "[keys.kana]\n\"left-ctrl#tap\" = \"@abc\"\n\"right-cmd#tap\" = \"@abc\"\n\"left-alt#tap\" = \"@abc\"\n[keys.application]\n\"right-ctrl\" = \"enter\"",
    );
    assert_eq!(problems, ["keys.application.right-ctrl"]);
    let kana = &settings.config.bindings.kana;
    for key in [Key::CtrlLeft, Key::CmdRight, Key::AltLeft] {
        assert_eq!(tapped(kana, key), Some(Action::Abc), "{key:?}");
    }
}

#[test]
fn an_invalid_romaji_line_is_reported_by_its_number() {
    let dir = temp_dir("romaji-lines");
    fs::create_dir_all(dir.join("romaji")).unwrap();
    fs::write(dir.join("romaji").join("mine.tsv"), "ka\tか\nbad line\n").unwrap();
    let (_, problems) = Settings::load("[romaji]\ntables = [\"mine.tsv\"]", &dir);
    assert_eq!(
        problems[0].kind,
        ProblemKind::RomajiInvalidLines {
            table: "mine.tsv".to_owned(),
            lines: vec![2],
        }
    );
}

#[test]
fn every_sendable_key_is_a_remap_target_and_nothing_else_named_is() {
    for name in key_names() {
        let (_, problems) = load(&format!(
            "[keys.{APPLICATION_TABLE}]\n\"ctrl+h\" = \"{name}\"\n"
        ));
        assert_eq!(
            problems.is_empty(),
            sendable_keys().contains(&name.as_str()),
            "{name}"
        );
    }
}

#[test]
fn every_bindings_table_reads_and_takes_a_binding_out() {
    for table in binding_tables() {
        let (_, problems) = load(&format!("[keys.{table}]\n\"ctrl+h\" = \"{UNBOUND}\"\n"));
        assert!(problems.is_empty(), "{table}: {problems:?}");
    }
    let (_, problems) = load("[keys.nowhere]\n");
    assert!(!problems.is_empty());
}

#[test]
fn a_dictionary_outside_the_dictionaries_folder_cannot_be_listed() {
    for name in ["../custom.tsv", "/etc/passwd", "sub/../../x.tsv"] {
        let text = format!("dictionaries = [\"a.tsv\", {name:?}]");

        let (settings, problems) = Settings::load(&text, Path::new("/base"));

        let kinds: Vec<_> = problems.into_iter().map(|p| (p.item, p.kind)).collect();
        assert_eq!(
            kinds,
            [(
                "dictionaries".to_owned(),
                ProblemKind::OutsideFolder(name.to_owned())
            )],
            "{name}"
        );
        assert_eq!(settings.dictionaries, None, "{name}");
    }
}

#[test]
fn a_romaji_table_outside_the_romaji_folder_cannot_be_listed() {
    let dir = temp_dir("romaji-outside");
    fs::write(dir.join("outside.tsv"), "qq\tくく\n").unwrap();
    let outside = dir.join("outside.tsv").display().to_string();
    for name in ["../outside.tsv", outside.as_str()] {
        let text = format!("[romaji]\ntables = [\"hepburn\", {name:?}]");

        let (settings, problems) = Settings::load(&text, &dir);

        let kinds: Vec<_> = problems.into_iter().map(|p| (p.item, p.kind)).collect();
        assert_eq!(
            kinds,
            [(
                "romaji.tables".to_owned(),
                ProblemKind::OutsideFolder(name.to_owned())
            )],
            "{name}"
        );
        assert_eq!(settings.romaji_tables, None, "{name}");
        assert_eq!(settings.config.romaji, default_romaji_table(), "{name}");
    }
}

#[test]
fn reading_a_romaji_table_by_a_name_outside_the_romaji_folder_fails() {
    let dir = temp_dir("romaji-read-outside");
    fs::write(dir.join("outside.tsv"), "qq\tくく\n").unwrap();

    let read = read_romaji_table(&dir, "../outside.tsv");

    assert_eq!(read.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn the_template_names_the_modifiers_of_both_keyboard_families() {
    assert!(TEMPLATE.contains("Ctrl・Cmd（Win）・Option（Alt）"));
}

#[test]
fn the_windows_jis_keys_are_bound_like_other_keys() {
    let (settings, problems) =
        load("[keys.reading]\n\"henkan\" = \"@commit\"\n[keys.abc]\n\"muhenkan\" = \"@kana\"");
    assert_eq!(problems, Vec::<String>::new());
    let bindings = &settings.config.bindings;
    assert_eq!(
        target(&bindings.reading, plain(Key::Henkan)),
        Some(Action::Commit)
    );
    assert_eq!(
        target(&bindings.abc, plain(Key::Muhenkan)),
        Some(Action::Kana)
    );
}

#[test]
fn the_template_says_that_a_written_dictionary_list_replaces_the_folder_default() {
    let comment: String = TEMPLATE
        .lines()
        .skip_while(|line| !line.starts_with("# 辞書の一覧"))
        .take_while(|line| !line.starts_with("#dictionaries"))
        .collect();
    assert!(comment.contains("代わり"), "{comment}");
    assert!(comment.contains("フォルダの辞書も書く"), "{comment}");
}
