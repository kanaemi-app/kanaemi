//! Reads the settings file into the core's configuration.
//!
//! Everything not written, or written in a way that cannot be read, keeps its
//! default; each item that could not be read is reported so the settings app
//! can show it and the IME can log it.

use std::path::Path;

use kanaemi_core::{Binding, Config, Modifiers, Remap, RomajiTable};
use toml::{Table, Value};

use crate::folder::stays_inside;
use crate::keys::{Scene, is_modifier, sendable};
use crate::{
    APPLICATION_TABLE, DICTIONARY_DIR, DictionarySource, FILE_NAME, UNBOUND, default_romaji_table,
    parse_action, parse_binding_key, parse_chord, read_romaji_table,
};

/// The name the dictionary list gives the user custom dictionary.
pub const USER_CUSTOM: &str = "custom";

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub config: Config,
    /// The dictionaries as listed; `None` when no list is written.
    pub dictionaries: Option<Vec<DictionarySource>>,
    /// The romaji tables as written, bundled names and file names; `None`
    /// when the default stack is used.
    pub romaji_tables: Option<Vec<String>>,
    /// The port to take requests from other programs on; `None` takes none.
    pub control_port: Option<u16>,
}

/// An item that could not be read, named by its path in the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub item: String,
    pub kind: ProblemKind,
}

/// Why an item could not be read. A kind rather than a sentence, so that
/// the settings app can say it in the user's words while logs stay English.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    /// The file is not TOML; the parser's message.
    Syntax(String),
    UnknownItem,
    NotATable,
    NotStrings,
    NotAString,
    NotABool,
    /// A mark must be a non-empty string on one line.
    BadMark,
    UnknownKey(String),
    UnknownModifier(String),
    RomajiUnreadable {
        table: String,
        error: String,
    },
    /// Line numbers, from 1, of a romaji table that could not be read. The
    /// table's other lines are used.
    RomajiInvalidLines {
        table: String,
        lines: Vec<usize>,
    },
    /// A modifier key is passed on to the application as it is, so it
    /// cannot be sent as another key.
    ModifierKey,
    /// A number of milliseconds is a whole number above 0.
    NotAPositiveInteger,
    /// A port is a whole number from 1 to 65535.
    NotAPort,
    /// Keys used while typing are bound to an action written `@name`.
    UnknownAction(String),
    /// An action that means nothing where it is bound.
    ActionNotHere(String),
    /// Only named keys can be sent to the application.
    NotSendable,
    /// A file named by a path that leaves its folder: absolute, or going up
    /// with `..`.
    OutsideFolder(String),
}

impl std::fmt::Display for ProblemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax(message) => write!(f, "not TOML: {message}"),
            Self::UnknownItem => write!(f, "unknown item"),
            Self::NotATable => write!(f, "not a table"),
            Self::NotStrings => write!(f, "not a list of strings"),
            Self::NotAString => write!(f, "not a string"),
            Self::NotABool => write!(f, "not true or false"),
            Self::BadMark => write!(f, "not a non-empty string on one line"),
            Self::UnknownKey(key) => write!(f, "unknown key {key}"),
            Self::UnknownModifier(name) => write!(f, "unknown modifier {name}"),
            Self::RomajiUnreadable { table, error } => write!(f, "{table}: {error}"),
            Self::RomajiInvalidLines { table, lines } => {
                write!(f, "{table}: invalid lines {lines:?}")
            }
            Self::ModifierKey => write!(f, "a modifier key cannot be sent as another key"),
            Self::NotAPositiveInteger => write!(f, "not a whole number above 0"),
            Self::NotAPort => write!(f, "not a whole number from 1 to 65535"),
            Self::UnknownAction(name) => write!(f, "unknown action {name}"),
            Self::ActionNotHere(name) => write!(f, "{name} does nothing here"),
            Self::NotSendable => write!(f, "only named keys can be sent to the application"),
            Self::OutsideFolder(name) => write!(f, "{name} is outside its folder"),
        }
    }
}

impl Settings {
    /// `dir` is the settings folder. Dictionary and romaji table files are
    /// named by their place in its dictionaries and romaji folders.
    pub fn load(text: impl AsRef<str>, dir: impl AsRef<Path>) -> (Self, Vec<Problem>) {
        let mut reader = Reader {
            settings: Settings {
                config: Config {
                    romaji: default_romaji_table(),
                    ..Config::default()
                },
                dictionaries: None,
                romaji_tables: None,
                control_port: None,
            },
            problems: Vec::new(),
            dir: dir.as_ref(),
        };
        match text.as_ref().parse::<Table>() {
            Ok(table) => reader.read(table),
            Err(error) => {
                reader.problem(FILE_NAME, ProblemKind::Syntax(error.message().to_owned()))
            }
        }
        (reader.settings, reader.problems)
    }
}

struct Reader<'a> {
    settings: Settings,
    problems: Vec<Problem>,
    dir: &'a Path,
}

impl Reader<'_> {
    fn problem(&mut self, item: impl Into<String>, kind: ProblemKind) {
        self.problems.push(Problem {
            item: item.into(),
            kind,
        });
    }

    fn read(&mut self, table: Table) {
        for (name, value) in table {
            match name.as_str() {
                "dictionaries" => self.dictionaries(value),
                "mode_indicator" => match value.as_bool() {
                    Some(show) => self.settings.config.mode_indicator = show,
                    None => self.problem(name, ProblemKind::NotABool),
                },
                "marks" => self.section(&name, value, Self::mark),
                "romaji" => self.section(&name, value, Self::romaji),
                "keys" => self.section(&name, value, Self::keys),
                "control" => self.section(&name, value, Self::control),
                _ => self.problem(name, ProblemKind::UnknownItem),
            }
        }
    }

    fn section(&mut self, name: &str, value: Value, item: fn(&mut Self, &str, &str, Value)) {
        match value {
            Value::Table(table) => {
                for (key, value) in table {
                    item(self, &format!("{name}.{key}"), &key, value);
                }
            }
            _ => self.problem(name, ProblemKind::NotATable),
        }
    }

    fn strings(&mut self, item: &str, value: Value) -> Option<Vec<String>> {
        let strings = match value {
            Value::Array(values) => values
                .into_iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>(),
            _ => None,
        };
        if strings.is_none() {
            self.problem(item, ProblemKind::NotStrings);
        }
        strings
    }

    fn dictionaries(&mut self, value: Value) {
        let Some(names) = self.strings("dictionaries", value) else {
            return;
        };
        if let Some(outside) = names.iter().find(|n| !stays_inside(n)) {
            return self.problem("dictionaries", ProblemKind::OutsideFolder(outside.clone()));
        }
        let mut sources: Vec<DictionarySource> = names
            .iter()
            .map(|name| match name.as_str() {
                USER_CUSTOM => DictionarySource::UserCustom,
                file => DictionarySource::File(self.dir.join(DICTIONARY_DIR).join(file)),
            })
            .collect();
        if !sources.contains(&DictionarySource::UserCustom) {
            sources.insert(0, DictionarySource::UserCustom);
        }
        self.settings.dictionaries = Some(sources);
    }

    fn control(&mut self, item: &str, key: &str, value: Value) {
        if key != "port" {
            return self.problem(item, ProblemKind::UnknownItem);
        }
        match value.as_integer().and_then(|port| u16::try_from(port).ok()) {
            Some(port) if port > 0 => self.settings.control_port = Some(port),
            _ => self.problem(item, ProblemKind::NotAPort),
        }
    }

    fn mark(&mut self, item: &str, key: &str, value: Value) {
        let mark = match value.as_str() {
            Some(s) if !s.is_empty() && !s.contains(['\t', '\n', '\r']) => s.to_owned(),
            _ => return self.problem(item, ProblemKind::BadMark),
        };
        let marks = &mut self.settings.config.marks;
        let field = match key {
            "reading" => &mut marks.reading,
            "candidate" => &mut marks.candidate,
            "okurigana" => &mut marks.okurigana,
            "registration" => &mut marks.registration,
            "cursor" => &mut marks.cursor,
            _ => return self.problem(item, ProblemKind::UnknownItem),
        };
        *field = mark;
    }

    fn romaji(&mut self, item: &str, key: &str, value: Value) {
        if key != "tables" {
            return self.problem(item, ProblemKind::UnknownItem);
        }
        let Some(names) = self.strings(item, value) else {
            return;
        };
        if let Some(outside) = names.iter().find(|n| !stays_inside(n)) {
            return self.problem(item, ProblemKind::OutsideFolder(outside.clone()));
        }
        let mut table = RomajiTable::empty();
        self.settings.romaji_tables = Some(names.clone());
        for name in names {
            let text = match read_romaji_table(self.dir, &name) {
                Ok(text) => text,
                Err(error) => {
                    self.problem(
                        item,
                        ProblemKind::RomajiUnreadable {
                            table: name,
                            error: error.to_string(),
                        },
                    );
                    continue;
                }
            };
            let invalid = table.apply(&text);
            if !invalid.is_empty() {
                self.problem(
                    item,
                    ProblemKind::RomajiInvalidLines {
                        table: name,
                        lines: invalid,
                    },
                );
            }
        }
        self.settings.config.romaji = table;
    }

    fn keys(&mut self, item: &str, key: &str, value: Value) {
        if key == "pass_while_composing" {
            let Some(names) = self.strings(item, value) else {
                return;
            };
            let mut pass = Modifiers::default();
            for name in names {
                match name.as_str() {
                    "cmd" => pass.cmd = true,
                    "ctrl" => pass.ctrl = true,
                    "alt" => pass.alt = true,
                    _ => return self.problem(item, ProblemKind::UnknownModifier(name)),
                }
            }
            self.settings.config.pass_while_composing = pass;
            return;
        }
        if key == "tap_timeout_ms" {
            match value.as_integer().and_then(|ms| u64::try_from(ms).ok()) {
                Some(ms) if ms > 0 => self.settings.config.tap_timeout_ms = ms,
                _ => self.problem(item, ProblemKind::NotAPositiveInteger),
            }
            return;
        }
        let Value::Table(table) = value else {
            return self.problem(item, ProblemKind::NotATable);
        };
        let scene = Scene::named(key);
        if scene.is_none() && key != APPLICATION_TABLE {
            return self.problem(item, ProblemKind::UnknownItem);
        }
        for (written, to) in table {
            let at = format!("{item}.{written}");
            let Some(to) = to.as_str() else {
                self.problem(at, ProblemKind::NotAString);
                continue;
            };
            let Some(scene) = scene else {
                let Some(from) = parse_chord(&written) else {
                    self.problem(at, ProblemKind::UnknownKey(written));
                    continue;
                };
                if is_modifier(from.key) {
                    self.problem(at, ProblemKind::ModifierKey);
                    continue;
                }
                let to = match to {
                    UNBOUND => None,
                    to => match parse_chord(to) {
                        Some(to) if sendable(to.key) => Some(to),
                        Some(_) => {
                            self.problem(at, ProblemKind::NotSendable);
                            continue;
                        }
                        None => {
                            self.problem(at, ProblemKind::UnknownKey(to.to_owned()));
                            continue;
                        }
                    },
                };
                let remaps = &mut self.settings.config.bindings.application;
                remaps.retain(|b| b.from != from);
                if let Some(to) = to {
                    remaps.push(Remap { from, to });
                }
                continue;
            };
            let Some((from, gesture)) = parse_binding_key(&written) else {
                self.problem(at, ProblemKind::UnknownKey(written));
                continue;
            };
            let to = match to {
                UNBOUND => None,
                to => match parse_action(to) {
                    Some(action) if scene.allows(action) => Some(action),
                    Some(_) => {
                        self.problem(at, ProblemKind::ActionNotHere(to.to_owned()));
                        continue;
                    }
                    None => {
                        self.problem(at, ProblemKind::UnknownAction(to.to_owned()));
                        continue;
                    }
                },
            };
            let bindings = scene.bindings_mut(&mut self.settings.config.bindings);
            bindings.retain(|b| (b.from, b.gesture) != (from, gesture));
            if let Some(to) = to {
                bindings.push(Binding { from, gesture, to });
            }
        }
    }
}
