//! The IME's log, as the settings app shows it.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// How much of the log's end is read: the log grows without bound, and the
/// latest lines are the ones that explain a problem.
const TAIL_BYTES: u64 = 256 * 1024;

/// The last whole lines of the log, newest last. A missing log is empty.
pub fn tail(path: &Path) -> io::Result<String> {
    let mut file = match File::open(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
        file => file?,
    };
    let len = file.metadata()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    // Reading from the middle of the file starts in the middle of a line.
    let text = match (start > 0, text.find('\n')) {
        (true, Some(at)) => &text[at + 1..],
        _ => &text,
    };
    Ok(text.to_owned())
}

/// A line of the log cut into the parts the style sheet colours, in order,
/// each with its class or none; together they are the line again.
pub struct Styled<'a> {
    /// The class of a line that records something gone wrong.
    pub alert: Option<&'static str>,
    pub pieces: Vec<(Option<&'static str>, &'a str)>,
}

/// `line` cut as the IME's tracing writes it: time, level, target, message
/// and `key=value` fields. A line in another shape is one plain piece.
pub fn style(line: &str) -> Styled<'_> {
    cut(line).unwrap_or_else(|| Styled {
        alert: None,
        pieces: vec![(None, line)],
    })
}

fn cut(line: &str) -> Option<Styled<'_>> {
    let time_end = line.find(' ')?;
    let time = &line[..time_end];
    if !is_time(time) {
        return None;
    }
    // The level is padded to one width, so a short one leaves more spaces.
    let level_start = time_end + line[time_end..].len() - line[time_end..].trim_start().len();
    let level_end = level_start + line[level_start..].find(' ')?;
    let alert = match &line[level_start..level_end] {
        "TRACE" | "DEBUG" | "INFO" => None,
        "WARN" => Some("log-warn"),
        "ERROR" => Some("log-error"),
        _ => return None,
    };
    let target_start = level_end + line[level_end..].len() - line[level_end..].trim_start().len();
    let target_end = target_start + line[target_start..].find(": ")?;
    if target_start == target_end || line[target_start..target_end].contains(' ') {
        return None;
    }
    let body_start = target_end + 2;
    let mut pieces = vec![
        (Some("log-time"), time),
        (None, &line[time_end..level_start]),
        (Some("log-level"), &line[level_start..level_end]),
        (None, &line[level_end..target_start]),
        (Some("log-target"), &line[target_start..target_end]),
        (None, &line[target_end..body_start]),
    ];
    let body = &line[body_start..];
    let fields = fields(body);
    let message_end = fields.first().map_or(body.len(), |&(start, _)| start);
    push_word(&mut pieces, "log-message", &body[..message_end]);
    for (i, &(start, value_start)) in fields.iter().enumerate() {
        let end = fields.get(i + 1).map_or(body.len(), |&(next, _)| next);
        pieces.push((Some("log-key"), &body[start..value_start]));
        push_word(&mut pieces, "log-value", &body[value_start..end]);
    }
    pieces.retain(|(_, text)| !text.is_empty());
    Some(Styled { alert, pieces })
}

/// `text` styled as `class`, the spaces after it plain.
fn push_word<'a>(
    pieces: &mut Vec<(Option<&'static str>, &'a str)>,
    class: &'static str,
    text: &'a str,
) {
    let word = text.trim_end();
    pieces.push((Some(class), word));
    pieces.push((None, &text[word.len()..]));
}

/// Whether `text` is a time as the log writes it, `2026-10-07T16:10:06.348247Z`.
fn is_time(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_digit())
        && text.contains('T')
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || "-:.TZ+".contains(c))
}

/// Where each field of `body` and its value start: a name and `=` begin a
/// word. A quoted value is skipped whole, so what it quotes is never a field.
fn fields(body: &str) -> Vec<(usize, usize)> {
    let bytes = body.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if at == 0 || bytes[at - 1] == b' ' {
            let name = bytes[at..]
                .iter()
                .take_while(|&&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
                .count();
            if name > 0 && bytes.get(at + name) == Some(&b'=') {
                let value = at + name + 1;
                found.push((at, value));
                at = value;
                if bytes.get(at) == Some(&b'"') {
                    at = after_quote(bytes, at);
                }
                continue;
            }
        }
        at += 1;
    }
    found
}

/// Just past the quote closing the one at `open`, or the end without one.
fn after_quote(bytes: &[u8], open: usize) -> usize {
    let mut at = open + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "kanaemi-settings-logs-{}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn a_short_log_is_read_whole() {
        let path = temp_path("short.log");
        fs::write(&path, "one\ntwo\n").unwrap();
        assert_eq!(tail(&path).unwrap(), "one\ntwo\n");
    }

    #[test]
    fn a_long_log_is_read_from_a_whole_line_near_its_end() {
        let path = temp_path("long.log");
        let line = "x".repeat(99) + "\n";
        fs::write(&path, line.repeat(5000) + "last\n").unwrap();
        let text = tail(&path).unwrap();
        assert!(text.len() as u64 <= TAIL_BYTES);
        assert!(text.starts_with(&line));
        assert!(text.ends_with("last\n"));
    }

    #[test]
    fn a_missing_log_is_empty() {
        assert_eq!(tail(&temp_path("missing.log")).unwrap(), "");
    }

    /// The pieces of `line`, checked to make it again.
    fn pieces(line: &str) -> Styled<'_> {
        let styled = style(line);
        assert_eq!(
            styled
                .pieces
                .iter()
                .map(|(_, text)| *text)
                .collect::<String>(),
            line,
            "the pieces make the line again"
        );
        styled
    }

    fn class_of<'a>(styled: &Styled<'a>, text: &str) -> Option<&'static str> {
        styled
            .pieces
            .iter()
            .find(|(_, t)| *t == text)
            .unwrap_or_else(|| panic!("no piece {text:?}"))
            .0
    }

    #[test]
    fn a_line_is_cut_into_time_level_target_message_and_fields() {
        let styled = pieces(
            "2026-10-07T16:10:06.348247Z  WARN kanaemi::secure_input: secure input is enabled pid=411 app=\"loginwindow\"",
        );
        assert_eq!(
            class_of(&styled, "2026-10-07T16:10:06.348247Z"),
            Some("log-time")
        );
        assert_eq!(class_of(&styled, "WARN"), Some("log-level"));
        assert_eq!(
            class_of(&styled, "kanaemi::secure_input"),
            Some("log-target")
        );
        assert_eq!(
            class_of(&styled, "secure input is enabled"),
            Some("log-message")
        );
        assert_eq!(class_of(&styled, "pid="), Some("log-key"));
        assert_eq!(class_of(&styled, "411"), Some("log-value"));
        assert_eq!(class_of(&styled, "app="), Some("log-key"));
        assert_eq!(class_of(&styled, "\"loginwindow\""), Some("log-value"));
    }

    #[test]
    fn warnings_and_errors_stand_out() {
        let line = |level| format!("2026-10-07T16:10:06.348247Z {level} kanaemi: m");
        assert_eq!(pieces(&line("ERROR")).alert, Some("log-error"));
        assert_eq!(pieces(&line(" WARN")).alert, Some("log-warn"));
        assert_eq!(pieces(&line(" INFO")).alert, None);
        assert_eq!(pieces(&line("DEBUG")).alert, None);
        assert_eq!(pieces(&line("TRACE")).alert, None);
    }

    #[test]
    fn a_level_padded_after_it_still_cuts() {
        let styled = pieces("2026-10-07T16:10:06.348247Z INFO  kanaemi: m");
        assert_eq!(class_of(&styled, "INFO"), Some("log-level"));
        assert_eq!(class_of(&styled, "kanaemi"), Some("log-target"));
    }

    #[test]
    fn a_value_runs_to_the_next_field_even_through_spaces() {
        let styled = pieces(
            "2026-10-06T18:32:38.318683Z  INFO kanaemi_runtime::dictionaries: ranking model loaded path=/Users/a/Application Support/ranking.model invalid=0",
        );
        assert_eq!(
            class_of(&styled, "/Users/a/Application Support/ranking.model"),
            Some("log-value")
        );
        assert_eq!(class_of(&styled, "invalid="), Some("log-key"));
        assert_eq!(class_of(&styled, "0"), Some("log-value"));
    }

    #[test]
    fn a_quoted_value_hides_what_looks_like_a_field() {
        let styled = pieces("2026-10-07T16:10:06.348247Z ERROR kanaemi: failed error=\"a b=c\"");
        assert_eq!(class_of(&styled, "\"a b=c\""), Some("log-value"));
    }

    #[test]
    fn a_line_of_fields_alone_has_no_message() {
        let styled = pieces("2026-10-07T16:10:06.348247Z  INFO kanaemi: port=1234");
        assert!(
            styled
                .pieces
                .iter()
                .all(|(class, _)| *class != Some("log-message"))
        );
        assert_eq!(class_of(&styled, "1234"), Some("log-value"));
    }

    #[test]
    fn a_line_in_another_shape_is_plain() {
        for line in [
            "",
            "    at the second line of a message",
            "2026-10-07T16:10:06.348247Z  NOTE kanaemi: m",
            "2026-10-07T16:10:06.348247Z  INFO no target here",
            "not a time  INFO kanaemi: m",
        ] {
            let styled = pieces(line);
            assert_eq!(styled.alert, None, "{line:?}");
            assert!(
                styled.pieces.iter().all(|(class, _)| class.is_none()),
                "{line:?}"
            );
        }
    }
}
