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
}
