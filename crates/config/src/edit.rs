//! Changes one setting at a time in the settings file, leaving the rest of
//! what the user wrote — comments, order, layout — as it was.
//!
//! The file usually starts as the commented template, so a setting is set by
//! uncommenting its line in the template and reset by putting that line back:
//! setting and then resetting gives the template again.

use toml_edit::{DocumentMut, Item, Key, TableLike, Value};

use crate::TEMPLATE;

/// The text of a settings file being edited. Every edit keeps it valid TOML.
#[derive(Clone, Debug)]
pub struct Editor {
    text: String,
}

/// How one line of the file reads.
#[derive(Debug, PartialEq)]
enum Line {
    Header { table: Vec<String>, commented: bool },
    Item { key: String, commented: bool },
    Other,
}

impl Editor {
    /// Fails, with the parser's message, on text that is not TOML: it cannot
    /// be edited without guessing what was meant.
    pub fn new(text: impl Into<String>) -> Result<Self, String> {
        let text = text.into();
        parse(&text)?;
        Ok(Self { text })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The keys written in `table`, as written.
    pub fn keys(&self, table: &[&str]) -> Vec<String> {
        let Ok(mut doc) = parse(&self.text) else {
            return Vec::new();
        };
        table_mut(&mut doc, table)
            .map(|t| t.iter().map(|(k, _)| k.to_owned()).collect())
            .unwrap_or_default()
    }

    /// Writes `value` at `path`: the tables, then the item's key.
    pub fn set(&mut self, path: &[&str], value: impl Into<Value>) {
        let (key, table) = path.split_last().expect("a setting has a key");
        let value = value.into();
        let mut doc = parse(&self.text).expect("kept valid");
        if let Some(Item::Value(old)) = item_mut(&mut doc, table, key) {
            // Keep what surrounds the value, such as a comment after it.
            let decor = old.decor().clone();
            *old = value;
            *old.decor_mut() = decor;
            self.text = doc.to_string();
            return;
        }
        let defined = is_defined(&doc, table);
        let mut lines = lines_of(&self.text);
        let line = format!("{} = {}", Key::new(*key).display_repr(), value);
        if let Some((header, at)) = find(&lines, table, key, true, defined) {
            lines[at] = line;
            if let Some(header) = header
                && let Some(uncommented) = lines[header].strip_prefix('#')
            {
                lines[header] = uncommented.to_owned();
            }
        } else if let Some(header) = (!table.is_empty() && !defined)
            .then(|| find_header(&lines, table, true))
            .flatten()
        {
            lines[header] = lines[header][1..].to_owned();
            lines.insert(header + 1, line);
        } else if table.is_empty() {
            // At the end of the top-level items, before the first section.
            let mut at = lines
                .iter()
                .position(|l| matches!(classify(l), Line::Header { .. }))
                .unwrap_or(lines.len());
            while at > 0 && lines[at - 1].trim().is_empty() {
                at -= 1;
            }
            lines.insert(at, line);
        } else if let Some(header) = defined.then(|| find_header(&lines, table, false)).flatten() {
            lines.insert(header + 1, line);
        } else if !defined {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!(
                "[{}]",
                table
                    .iter()
                    .map(|name| Key::new(*name).display_repr().into_owned())
                    .collect::<Vec<_>>()
                    .join(".")
            ));
            lines.push(line);
        } else {
            // Written without a header line of its own: inline or dotted.
            self.text = insert(doc, table, key, value);
            return;
        }
        let text = join(&lines);
        // A layout the lines above do not foresee — a table written with
        // dotted keys, a line inside a multi-line string — takes the value
        // through the document instead.
        let expected = changed(&self.text, table, key, Some(&value));
        self.text = if expected.is_some() && meaning(&text) == expected {
            text
        } else {
            insert(doc, table, key, value)
        };
    }

    /// Removes what is written at `path`, so the setting takes its default.
    pub fn reset(&mut self, path: &[&str]) {
        let (key, table) = path.split_last().expect("a setting has a key");
        let mut lines = lines_of(&self.text);
        let Some((header, at)) = find(&lines, table, key, false, true) else {
            let mut doc = parse(&self.text).expect("kept valid");
            if let Some(parent) = table_mut(&mut doc, table) {
                parent.remove(key);
                self.text = doc.to_string();
            }
            return;
        };
        let template = lines_of(TEMPLATE);
        match find(&template, table, key, true, false) {
            Some((_, original)) => lines[at] = template[original].clone(),
            None => {
                lines.remove(at);
            }
        }
        // A section left with nothing in it goes back to the template's
        // commented header.
        let emptied = parse(&join(&lines))
            .ok()
            .is_some_and(|mut doc| table_mut(&mut doc, table).is_some_and(|t| t.is_empty()));
        if let Some(header) = header
            && emptied
            && find_header(&template, table, true).is_some()
        {
            lines[header] = format!("#{}", lines[header]);
        }
        let text = join(&lines);
        let expected = changed(&self.text, table, key, None);
        if expected.is_some() && meaning(&text) == expected {
            self.text = text;
        } else {
            let mut doc = parse(&self.text).expect("kept valid");
            if let Some(parent) = table_mut(&mut doc, table) {
                parent.remove(key);
            }
            self.text = doc.to_string();
        }
    }
}

/// What a settings text says, apart from how it is laid out. An empty
/// table says nothing, as a commented header does not.
fn meaning(text: &str) -> Option<toml::Table> {
    text.parse().ok().map(pruned)
}

fn pruned(table: toml::Table) -> toml::Table {
    table
        .into_iter()
        .filter_map(|(key, value)| match value {
            toml::Value::Table(inner) => {
                let inner = pruned(inner);
                (!inner.is_empty()).then(|| (key, toml::Value::Table(inner)))
            }
            value => Some((key, value)),
        })
        .collect()
}

/// What `text` says once `key` in `table` is `value`, or is gone with `None`.
fn changed(text: &str, table: &[&str], key: &str, value: Option<&Value>) -> Option<toml::Table> {
    let mut meant = meaning(text)?;
    let mut current = &mut meant;
    for name in table {
        current = current
            .entry(name.to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()?;
    }
    match value {
        Some(value) => {
            let parsed: toml::Table = format!("v = {value}").parse().ok()?;
            current.insert(key.to_owned(), parsed.get("v")?.clone());
        }
        None => {
            current.remove(key);
        }
    }
    Some(pruned(meant))
}

/// `doc` with `value` at `key` in `table`, making the tables it lacks.
fn insert(mut doc: DocumentMut, table: &[&str], key: &str, value: Value) -> String {
    let mut current: &mut dyn TableLike = doc.as_table_mut();
    for name in table {
        if current.get(name).and_then(Item::as_table_like).is_none() {
            current.insert(name, Item::Table(toml_edit::Table::new()));
        }
        current = current
            .get_mut(name)
            .and_then(Item::as_table_like_mut)
            .expect("just made");
    }
    current.insert(key, Item::Value(value));
    doc.to_string()
}

fn parse(text: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>()
        .map_err(|error| error.message().to_owned())
}

/// A table with a header or written inline (`marks = { … }`).
fn table_mut<'a>(doc: &'a mut DocumentMut, table: &[&str]) -> Option<&'a mut dyn TableLike> {
    let mut current: &mut dyn TableLike = doc.as_table_mut();
    for name in table {
        current = current.get_mut(name)?.as_table_like_mut()?;
    }
    Some(current)
}

fn item_mut<'a>(doc: &'a mut DocumentMut, table: &[&str], key: &str) -> Option<&'a mut Item> {
    table_mut(doc, table)?.get_mut(key)
}

/// Whether the table is written, with a header or inline, so a second header
/// would be an error.
fn is_defined(doc: &DocumentMut, table: &[&str]) -> bool {
    let Some((last, parents)) = table.split_last() else {
        return true;
    };
    let mut current: &dyn TableLike = doc.as_table();
    for name in parents {
        match current.get(name).and_then(Item::as_table_like) {
            Some(next) => current = next,
            None => return false,
        }
    }
    match current.get(last) {
        Some(Item::Table(t)) => !t.is_implicit(),
        Some(Item::Value(Value::InlineTable(_))) => true,
        _ => false,
    }
}

fn lines_of(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
}

fn join(lines: &[String]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn classify(line: &str) -> Line {
    let (commented, body) = match line.strip_prefix('#') {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let trimmed = body.trim();
    if trimmed.starts_with('[') && !trimmed.starts_with("[[") {
        let inner = trimmed[1..].split(']').next().unwrap_or_default();
        return match Key::parse(inner) {
            Ok(keys) => Line::Header {
                table: keys.iter().map(|k| k.get().to_owned()).collect(),
                commented,
            },
            Err(_) => Line::Other,
        };
    }
    let Ok(doc) = body.parse::<DocumentMut>() else {
        return Line::Other;
    };
    let mut items = doc.iter();
    match (items.next(), items.next()) {
        (Some((key, Item::Value(_))), None) => Line::Item {
            key: key.to_owned(),
            commented,
        },
        _ => Line::Other,
    }
}

fn find_header(lines: &[String], table: &[&str], commented: bool) -> Option<usize> {
    lines.iter().position(|line| {
        classify(line)
            == Line::Header {
                table: table.iter().map(|s| (*s).to_owned()).collect(),
                commented,
            }
    })
}

/// The line of `key` in `table`, with the line of the table's header if it
/// has one. A commented line is looked for under the table's header, or
/// under its commented header while the table is not `defined` elsewhere.
fn find(
    lines: &[String],
    table: &[&str],
    key: &str,
    commented: bool,
    defined: bool,
) -> Option<(Option<usize>, usize)> {
    let mut header: Option<(usize, Vec<String>, bool)> = None;
    for (at, line) in lines.iter().enumerate() {
        match classify(line) {
            Line::Header {
                table,
                commented: header_commented,
            } => header = Some((at, table, header_commented)),
            Line::Item {
                key: found,
                commented: line_commented,
            } if found == key && line_commented == commented => {
                let matches = match &header {
                    None => table.is_empty(),
                    Some((_, names, header_commented)) => {
                        names == table && (!header_commented || (commented && !defined))
                    }
                };
                if matches {
                    return Some((header.map(|(h, _, _)| h), at));
                }
            }
            _ => {}
        }
    }
    None
}
