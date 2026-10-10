use std::path::Path;

use kanaemi_config::{Editor, Settings, TEMPLATE, Value};

fn edited(text: &str, edit: impl FnOnce(&mut Editor)) -> String {
    let mut editor = Editor::new(text).unwrap();
    edit(&mut editor);
    editor.text().to_owned()
}

fn settings(text: &str) -> Settings {
    let (settings, problems) = Settings::load(text, Path::new("/nonexistent"));
    assert!(problems.is_empty(), "{problems:?}\n{text}");
    settings
}

#[test]
fn setting_a_templated_item_uncomments_its_line_and_section() {
    let text = edited(TEMPLATE, |e| e.set(&["marks", "reading"], "▽"));
    assert_eq!(
        text,
        TEMPLATE
            .replace("#[marks]\n", "[marks]\n")
            .replace("#reading = \"›\"\n", "reading = \"▽\"\n")
    );
    assert_eq!(settings(&text).config.marks.reading, "▽");
}

#[test]
fn a_top_level_item_is_uncommented_in_place() {
    let text = edited(TEMPLATE, |e| e.set(&["mode_indicator"], false));
    assert_eq!(
        text,
        TEMPLATE.replace("#mode_indicator = true\n", "mode_indicator = false\n")
    );
    assert!(!settings(&text).config.mode_indicator);
}

#[test]
fn a_quoted_key_is_found_in_the_template() {
    let text = edited(TEMPLATE, |e| e.set(&["keys", "reading", "ctrl+h"], "@none"));
    assert_eq!(
        text,
        TEMPLATE
            .replace("#[keys.reading]\n", "[keys.reading]\n")
            .replacen(
                "#\"ctrl+h\" = \"@backspace\"\n",
                "\"ctrl+h\" = \"@none\"\n",
                1
            )
    );
}

#[test]
fn an_item_missing_from_the_template_goes_under_its_section() {
    let text = edited(TEMPLATE, |e| {
        e.set(&["keys", "reading", "ctrl+x"], "@cancel")
    });
    assert_eq!(
        text,
        TEMPLATE.replace(
            "#[keys.reading]\n",
            "[keys.reading]\n\"ctrl+x\" = \"@cancel\"\n"
        )
    );
}

#[test]
fn a_list_is_written_on_one_line() {
    let text = edited(TEMPLATE, |e| {
        e.set(
            &["romaji", "tables"],
            Value::from_iter(["hepburn", "kunrei"]),
        )
    });
    assert!(
        text.contains("\ntables = [\"hepburn\", \"kunrei\"]\n"),
        "{text}"
    );
}

#[test]
fn resetting_puts_the_template_back() {
    let paths: [&[&str]; 4] = [
        &["mode_indicator"],
        &["marks", "reading"],
        &["keys", "reading", "ctrl+h"],
        &["keys", "reading", "ctrl+x"],
    ];
    for path in paths {
        let text = edited(TEMPLATE, |e| {
            e.set(path, "x");
            e.reset(path);
        });
        assert_eq!(text, TEMPLATE, "{path:?}");
    }
}

#[test]
fn a_key_of_an_os_table_goes_under_that_table_and_back() {
    for os in kanaemi_core::Os::ALL.map(kanaemi_config::os_table) {
        let path = ["keys", "application", os, "ctrl+w"];

        let text = edited(TEMPLATE, |e| e.set(&path, "alt+backspace"));

        assert!(
            text.contains(&format!(
                "\n[keys.application.{os}]\n\"ctrl+w\" = \"alt+backspace\"\n"
            )),
            "{text}"
        );
        assert!(text.contains("#[keys.application]\n"), "{text}");
        settings(&text);
        let reset = edited(&text, |e| e.reset(&path));
        assert_eq!(reset, TEMPLATE, "{os}");
    }
}

#[test]
fn resetting_one_item_keeps_the_others_and_their_section() {
    let text = edited(TEMPLATE, |e| {
        e.set(&["marks", "reading"], "▽");
        e.set(&["marks", "cursor"], "_");
        e.reset(&["marks", "reading"]);
    });
    assert_eq!(
        text,
        TEMPLATE
            .replace("#[marks]\n", "[marks]\n")
            .replace("#cursor = \"|\"\n", "cursor = \"_\"\n")
    );
}

#[test]
fn a_written_value_is_replaced_keeping_its_comment() {
    let text = edited("[marks]\nreading = \"a\" # mine\n", |e| {
        e.set(&["marks", "reading"], "b")
    });
    assert_eq!(text, "[marks]\nreading = \"b\" # mine\n");
}

#[test]
fn a_hand_written_file_without_the_template_still_takes_settings() {
    let text = edited("# my settings\n", |e| {
        e.set(&["marks", "reading"], "▽");
        e.set(&["mode_indicator"], false);
    });
    assert!(text.starts_with("# my settings\n"), "{text}");
    let read = settings(&text);
    assert_eq!(read.config.marks.reading, "▽");
    assert!(!read.config.mode_indicator);
    let text = edited(&text, |e| {
        e.reset(&["marks", "reading"]);
        e.reset(&["mode_indicator"]);
    });
    assert_eq!(settings(&text), settings(""));
}

#[test]
fn resetting_what_is_not_written_changes_nothing() {
    assert_eq!(
        edited(TEMPLATE, |e| e.reset(&["marks", "reading"])),
        TEMPLATE
    );
}

#[test]
fn a_file_that_is_not_toml_cannot_be_edited() {
    assert!(Editor::new("[marks\n").is_err());
}

#[test]
fn an_item_goes_under_a_hand_written_section() {
    let text = edited("[marks]\ncursor = \"_\"\n", |e| {
        e.set(&["marks", "reading"], "▽")
    });
    assert_eq!(text, "[marks]\nreading = \"▽\"\ncursor = \"_\"\n");
}

#[test]
fn a_parent_section_can_follow_its_child() {
    let text = edited("[keys.reading]\n\"ctrl+h\" = \"@cancel\"\n", |e| {
        e.set(&["keys", "pass_while_composing"], Value::from_iter(["cmd"]))
    });
    let settings = settings(&text);
    assert!(settings.config.pass_while_composing.cmd);
}

#[test]
fn a_table_written_inline_is_edited_where_it_is() {
    let text = edited("marks = { reading = \"▽\" }\n", |e| {
        e.set(&["marks", "reading"], "◇");
        e.set(&["marks", "cursor"], "_");
    });
    let read = settings(&text);
    assert_eq!(read.config.marks.reading, "◇");
    assert_eq!(read.config.marks.cursor, "_");
    assert!(!text.contains("[marks]"), "{text}");
}

#[test]
fn a_section_keeps_its_header_while_a_value_on_many_lines_is_left() {
    let text = edited(
        "[keys]\npass_while_composing = [\n  \"cmd\",\n]\nreading = { \"ctrl+x\" = \"@cancel\" }\n",
        |e| e.reset(&["keys", "reading"]),
    );
    assert!(text.starts_with("[keys]\n"), "{text}");
    assert!(settings(&text).config.pass_while_composing.cmd, "{text}");
}

#[test]
fn a_table_written_with_dotted_keys_is_edited_without_a_second_header() {
    let text = edited("marks.reading = \"▽\"\n", |e| {
        e.set(&["marks", "cursor"], "_")
    });
    let read = settings(&text);
    assert_eq!(read.config.marks.reading, "▽");
    assert_eq!(read.config.marks.cursor, "_");
}

#[test]
fn a_line_inside_a_multi_line_string_is_not_a_setting() {
    let text = "dictionaries = ['''\n[marks]\nreading = \"fake\"\n''']\n[marks]\nreading = \"▽\"\n";
    let reset = edited(text, |e| e.reset(&["marks", "reading"]));
    assert!(reset.contains("reading = \"fake\""), "{reset}");
    assert!(!reset.contains("reading = \"▽\""), "{reset}");
    let set = edited(text, |e| e.set(&["marks", "reading"], "◇"));
    assert!(set.contains("reading = \"fake\""), "{set}");
    assert!(set.contains("reading = \"◇\""), "{set}");
}
