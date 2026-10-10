//! The keys sent to the application as other keys, on every OS at once:
//! what each OS sends for a pressed key, from the table for every OS, the
//! table of that OS and Kanaemi's own remaps for it, and the fewest lines of
//! those tables that make each OS send what is wanted.

use kanaemi_config::{APPLICATION_TABLE, ApplicationTables, UNBOUND, format_chord, os_table};
use kanaemi_core::{Chord, Os, default_remaps};

/// A line of the settings file to write: its path, and the value written
/// there, or `None` to remove the line.
pub type Edit = (Vec<String>, Option<String>);

/// One pressed key, and what each OS sends for it: another key, or `None`
/// when the key goes on as itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub key: Chord,
    pub sent: Vec<(Os, Option<Chord>)>,
}

impl Row {
    /// Whether every OS sends what Kanaemi makes it send.
    pub fn is_default(&self) -> bool {
        self.sent
            .iter()
            .all(|(os, to)| *to == default_sent(*os, self.key))
    }
}

/// What `os` sends for `key`, `None` when it goes on as itself.
pub fn sent(tables: &ApplicationTables, os: Os, key: Chord) -> Option<Chord> {
    let os_line = tables.os.get(&os).and_then(|lines| line(lines, key));
    match os_line.or_else(|| line(&tables.shared, key)) {
        Some(to) => to,
        None => default_sent(os, key),
    }
}

/// What `os` sends for `key` with nothing written.
pub fn default_sent(os: Os, key: Chord) -> Option<Chord> {
    default_remaps(Some(os))
        .into_iter()
        .find(|r| r.from == key)
        .map(|r| r.to)
}

/// What a table writes for `key`: `None` with no line, `Some(None)` for a
/// line taking the remap off.
fn line(lines: &[(Chord, Option<Chord>)], key: Chord) -> Option<Option<Chord>> {
    lines
        .iter()
        .find(|(from, _)| *from == key)
        .map(|(_, to)| *to)
}

/// Every key Kanaemi remaps on some OS or a table writes: Kanaemi's own
/// first, in their order, then the written ones.
pub fn rows(tables: &ApplicationTables) -> Vec<Row> {
    let defaults = Os::ALL
        .into_iter()
        .flat_map(|os| default_remaps(Some(os)))
        .map(|r| r.from);
    let written = Os::ALL.iter().filter_map(|os| tables.os.get(os)).flatten();
    let written = tables.shared.iter().chain(written).map(|(from, _)| *from);
    let mut keys: Vec<Chord> = Vec::new();
    for key in defaults.chain(written) {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys.into_iter()
        .map(|key| Row {
            key,
            sent: Os::ALL.map(|os| (os, sent(tables, os, key))).to_vec(),
        })
        .collect()
}

/// What makes each OS send `wanted(os)` for `key`: the shared line that
/// leaves the fewest OS lines to write, none when that is a tie, and those
/// OS lines. Lines already written so stay as they are.
pub fn writing(
    tables: &ApplicationTables,
    key: Chord,
    wanted: impl Fn(Os) -> Option<Chord>,
) -> Vec<Edit> {
    let Some(name) = format_chord(key) else {
        return Vec::new();
    };
    let wanted = Os::ALL.map(|os| (os, wanted(os)));
    // What each OS sends with the shared line `shared` and no line of its
    // own.
    let beneath = |shared: Option<Option<Chord>>, os: Os| match shared {
        Some(to) => to,
        None => default_sent(os, key),
    };
    let os_lines = |shared: Option<Option<Chord>>| {
        wanted.map(|(os, to)| (os, (beneath(shared, os) != to).then_some(to)))
    };
    let count = |shared| os_lines(shared).iter().filter(|(_, l)| l.is_some()).count();
    let mut shared = None;
    for (_, to) in wanted {
        if count(Some(to)) < count(shared) {
            shared = Some(to);
        }
    }
    let path = |os: Option<Os>| {
        ["keys", APPLICATION_TABLE]
            .into_iter()
            .chain(os.map(os_table))
            .chain([name.as_str()])
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let mut edits = Vec::new();
    let mut edit = |at: Option<Os>, now: Option<Option<Chord>>, then: Option<Option<Chord>>| {
        if now != then {
            edits.push((path(at), then.map(written)));
        }
    };
    edit(None, line(&tables.shared, key), shared);
    for (os, then) in os_lines(shared) {
        let now = tables.os.get(&os).and_then(|lines| line(lines, key));
        edit(Some(os), now, then);
    }
    edits
}

/// A key sent as the settings file writes it, or the remap taken off.
fn written(to: Option<Chord>) -> String {
    match to {
        // A key the file has no name for is never read from it, nor offered.
        Some(to) => format_chord(to).unwrap_or_default(),
        None => UNBOUND.to_owned(),
    }
}

/// What makes every key go on as itself on every OS.
pub fn all_off(tables: &ApplicationTables) -> Vec<Edit> {
    rows(tables)
        .into_iter()
        .flat_map(|row| writing(tables, row.key, |_| None))
        .collect()
}

/// What puts every key back to Kanaemi's own on every OS.
pub fn all_default(tables: &ApplicationTables) -> Vec<Edit> {
    rows(tables)
        .into_iter()
        .flat_map(|row| writing(tables, row.key, |os| default_sent(os, row.key)))
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use kanaemi_config::{Editor, Settings, Value, parse_chord};

    use super::*;

    fn chord(text: &str) -> Chord {
        parse_chord(text).unwrap()
    }

    /// The lines of a table, each key with what it is sent as.
    type Lines<'a> = &'a [(&'a str, Option<&'a str>)];

    fn tables(shared: Lines, os: &[(Os, Lines)]) -> ApplicationTables {
        let entries = |list: Lines| {
            list.iter()
                .map(|(from, to)| (chord(from), to.map(chord)))
                .collect::<Vec<_>>()
        };
        ApplicationTables {
            shared: entries(shared),
            os: os
                .iter()
                .map(|(os, list)| (*os, entries(list)))
                .collect::<HashMap<_, _>>(),
        }
    }

    fn shared_line(key: &str, value: Option<&str>) -> Edit {
        (
            ["keys", APPLICATION_TABLE, key].map(str::to_owned).to_vec(),
            value.map(str::to_owned),
        )
    }

    fn os_line(os: Os, key: &str, value: Option<&str>) -> Edit {
        (
            ["keys", APPLICATION_TABLE, os_table(os), key]
                .map(str::to_owned)
                .to_vec(),
            value.map(str::to_owned),
        )
    }

    /// `text` with `edits` written over it as the app writes them.
    pub(crate) fn edited(text: &str, edits: &[Edit]) -> String {
        let mut editor = Editor::new(text).unwrap();
        for (path, value) in edits {
            let path: Vec<&str> = path.iter().map(String::as_str).collect();
            crate::store::edit(&mut editor, &path, value.as_deref().map(Value::from));
        }
        editor.text().to_owned()
    }

    /// The tables a file holds once `edits` are written to one holding
    /// `text`.
    fn after(text: &str, edits: &[Edit]) -> ApplicationTables {
        let text = edited(text, edits);
        let (settings, problems) = Settings::load(&text, Path::new("/nonexistent"));
        assert!(problems.is_empty(), "{problems:?}\n{text}");
        settings.application_tables
    }

    #[test]
    fn with_nothing_written_each_os_sends_kanaemis_own() {
        let none = ApplicationTables::default();

        assert_eq!(
            sent(&none, Os::Windows, chord("ctrl+a")),
            Some(chord("home"))
        );
        assert_eq!(
            sent(&none, Os::MacOs, chord("ctrl+backspace")),
            Some(chord("alt+backspace"))
        );
        assert_eq!(sent(&none, Os::Linux, chord("ctrl+backspace")), None);
        assert_eq!(sent(&none, Os::Linux, chord("ctrl+k")), None);
        assert_eq!(
            sent(&none, Os::MacOs, chord("ctrl+a")),
            None,
            "macOS text fields go to the line start themselves"
        );
    }

    #[test]
    fn the_shared_table_is_written_over_kanaemis_own_and_an_os_table_over_both() {
        let written = tables(
            &[("ctrl+a", Some("end")), ("ctrl+backspace", None)],
            &[(
                Os::Linux,
                &[("ctrl+a", None), ("ctrl+backspace", Some("ctrl+backspace"))],
            )],
        );

        assert_eq!(
            sent(&written, Os::MacOs, chord("ctrl+a")),
            Some(chord("end"))
        );
        assert_eq!(sent(&written, Os::Linux, chord("ctrl+a")), None);
        assert_eq!(sent(&written, Os::MacOs, chord("ctrl+backspace")), None);
        assert_eq!(
            sent(&written, Os::Linux, chord("ctrl+backspace")),
            Some(chord("ctrl+backspace"))
        );
    }

    #[test]
    fn every_key_remapped_on_some_os_or_written_is_a_row() {
        let written = tables(
            &[("ctrl+k", Some("end"))],
            &[(Os::Windows, &[("ctrl+u", None), ("ctrl+a", Some("end"))])],
        );

        let rows = rows(&written);

        let keys: Vec<Chord> = rows.iter().map(|r| r.key).collect();
        let mut expected: Vec<Chord> = Vec::new();
        for remap in Os::ALL.into_iter().flat_map(|os| default_remaps(Some(os))) {
            if !expected.contains(&remap.from) {
                expected.push(remap.from);
            }
        }
        expected.extend([chord("ctrl+k"), chord("ctrl+u")]);
        assert_eq!(keys, expected);
        let ctrl_a = rows.iter().find(|r| r.key == chord("ctrl+a")).unwrap();
        assert_eq!(
            ctrl_a.sent,
            [
                (Os::MacOs, None),
                (Os::Windows, Some(chord("end"))),
                (Os::Linux, Some(chord("home"))),
            ]
        );
    }

    #[test]
    fn a_row_is_kanaemis_own_while_every_os_sends_its_default() {
        let written = tables(
            &[],
            &[(Os::Linux, &[("ctrl+backspace", Some("ctrl+backspace"))])],
        );

        let rows = rows(&written);

        let row = |key| rows.iter().find(|r| r.key == chord(key)).unwrap();
        assert!(row("ctrl+a").is_default());
        assert!(!row("ctrl+backspace").is_default());
    }

    #[test]
    fn what_every_os_sends_by_default_writes_nothing() {
        let key = chord("ctrl+backspace");

        let edits = writing(&ApplicationTables::default(), key, |os| {
            default_sent(os, key)
        });

        assert_eq!(edits, []);
    }

    #[test]
    fn going_back_to_kanaemis_own_everywhere_removes_every_line_of_the_key() {
        let written = tables(
            &[("ctrl+a", Some("end"))],
            &[
                (Os::MacOs, &[("ctrl+a", Some("home"))]),
                (Os::Linux, &[("ctrl+a", None)]),
            ],
        );

        let key = chord("ctrl+a");

        let edits = writing(&written, key, |os| default_sent(os, key));

        assert_eq!(
            edits,
            [
                shared_line("ctrl+a", None),
                os_line(Os::MacOs, "ctrl+a", None),
                os_line(Os::Linux, "ctrl+a", None),
            ]
        );
    }

    #[test]
    fn one_os_apart_from_the_default_is_written_in_its_table_only() {
        let key = chord("ctrl+a");

        let edits = writing(&ApplicationTables::default(), key, |os| {
            Some(chord(if os == Os::MacOs { "end" } else { "home" }))
        });

        assert_eq!(edits, [os_line(Os::MacOs, "ctrl+a", Some("end"))]);
    }

    #[test]
    fn a_value_most_oses_want_goes_in_the_shared_table() {
        let key = chord("ctrl+k");

        let edits = writing(&ApplicationTables::default(), key, |os| {
            Some(chord(if os == Os::Linux { "home" } else { "end" }))
        });

        assert_eq!(
            edits,
            [
                shared_line("ctrl+k", Some("end")),
                os_line(Os::Linux, "ctrl+k", Some("home")),
            ]
        );
    }

    #[test]
    fn taking_a_key_off_on_every_os_writes_none_once_in_the_shared_table() {
        let edits = writing(&ApplicationTables::default(), chord("ctrl+h"), |_| None);

        assert_eq!(edits, [shared_line("ctrl+h", Some(UNBOUND))]);
    }

    #[test]
    fn taking_a_key_off_where_it_is_kanaemis_own_writes_none_in_that_table() {
        let key = chord("ctrl+h");

        let edits = writing(&ApplicationTables::default(), key, |os| {
            (os != Os::Windows).then(|| default_sent(os, key)).flatten()
        });

        assert_eq!(edits, [os_line(Os::Windows, "ctrl+h", Some(UNBOUND))]);
    }

    #[test]
    fn an_equal_count_leaves_the_shared_table_unwritten() {
        // Unwritten in the shared table, macOS and Windows need a line each;
        // written there as any of the wanted keys, two others do.
        let key = chord("ctrl+k");

        let edits = writing(&ApplicationTables::default(), key, |os| match os {
            Os::MacOs => Some(chord("end")),
            Os::Windows => Some(chord("home")),
            Os::Linux => None,
        });

        assert_eq!(
            edits,
            [
                os_line(Os::MacOs, "ctrl+k", Some("end")),
                os_line(Os::Windows, "ctrl+k", Some("home")),
            ]
        );
    }

    #[test]
    fn a_line_already_written_as_wanted_is_left_alone() {
        let written = tables(
            &[("ctrl+k", Some("end"))],
            &[(Os::Linux, &[("ctrl+k", Some("end"))])],
        );

        let edits = writing(&written, chord("ctrl+k"), |os| {
            Some(chord(if os == Os::Windows { "home" } else { "end" }))
        });

        assert_eq!(
            edits,
            [
                os_line(Os::Windows, "ctrl+k", Some("home")),
                os_line(Os::Linux, "ctrl+k", None),
            ]
        );
    }

    #[test]
    fn a_key_is_written_the_one_way_the_file_names_it() {
        let edits = writing(&ApplicationTables::default(), chord("shift+ctrl+k"), |_| {
            Some(chord("shift+end"))
        });

        assert_eq!(edits, [shared_line("ctrl+shift+k", Some("shift+end"))]);
    }

    #[test]
    fn what_is_written_makes_each_os_send_what_was_wanted() {
        let text = "[keys.application]\n\"ctrl+a\" = \"end\"\n\"shift+ctrl+k\" = \"home\"\n\
                    [keys.application.windows]\n\"ctrl+a\" = \"@none\"\n\"ctrl+shift+k\" = \"end\"\n";
        let (settings, _) = Settings::load(text, Path::new("/nonexistent"));
        let wants = [
            (Os::MacOs, Some("up")),
            (Os::Windows, None),
            (Os::Linux, Some("home")),
        ];
        for key in ["ctrl+a", "ctrl+shift+k", "ctrl+backspace", "ctrl+u"].map(chord) {
            let wanted = |os| {
                let (_, to) = wants.iter().find(|(o, _)| *o == os).unwrap();
                to.map(chord)
            };

            let edits = writing(&settings.application_tables, key, wanted);

            let written = after(text, &edits);
            for os in Os::ALL {
                assert_eq!(sent(&written, os, key), wanted(os), "{key:?} on {os:?}");
            }
        }
    }

    #[test]
    fn all_off_sends_every_key_as_itself_on_every_os() {
        let text = "[keys.application]\n\"ctrl+k\" = \"end\"\n\
                    [keys.application.linux]\n\"ctrl+u\" = \"home\"\n\"ctrl+h\" = \"left\"\n";
        let (settings, _) = Settings::load(text, Path::new("/nonexistent"));

        let written = after(text, &all_off(&settings.application_tables));

        for row in rows(&written) {
            assert!(row.sent.iter().all(|(_, to)| to.is_none()), "{row:?}");
        }
        assert_eq!(all_off(&written), []);
    }

    #[test]
    fn all_default_writes_nothing_at_all() {
        let text = "[keys.application]\n\"ctrl+k\" = \"end\"\n\"ctrl+h\" = \"@none\"\n\
                    [keys.application.macos]\n\"ctrl+backspace\" = \"@none\"\n";
        let (settings, _) = Settings::load(text, Path::new("/nonexistent"));

        let written = after(text, &all_default(&settings.application_tables));

        assert_eq!(written, ApplicationTables::default());
        assert_eq!(all_default(&written), []);
    }
}
