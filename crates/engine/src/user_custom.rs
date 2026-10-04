//! The user custom dictionary: the text dictionary registrations and
//! deletions are written to, and the file it lives in.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{InvalidLine, InvalidReason, TextDictionary};

/// Where registrations and deletions are written as text dictionary lines.
pub trait LineSink {
    fn append(&mut self, line: &str) -> io::Result<()>;
}

/// A sink chosen when the engine is opened, as a host that writes either to a
/// file or elsewhere does.
impl<S: LineSink + ?Sized> LineSink for Box<S> {
    fn append(&mut self, line: &str) -> io::Result<()> {
        (**self).append(line)
    }
}

/// A registration or deletion that was not written. Its effect is kept in
/// memory all the same.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The line would not read back; what the user typed stays out of the
    /// message, which may end up in a log.
    #[error("not a valid dictionary line: {0}")]
    Invalid(InvalidReason),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The user custom dictionary as the engine holds it: what is in memory, and
/// where its new lines go. A line the sink fails to write still applies in
/// memory and is written again before the next one.
pub(crate) struct UserCustom {
    dictionary: TextDictionary,
    sink: Box<dyn LineSink>,
    /// Oldest first.
    unwritten: VecDeque<String>,
    errors: Vec<WriteError>,
}

impl UserCustom {
    pub(crate) fn new(dictionary: TextDictionary, sink: Box<dyn LineSink>) -> Self {
        Self {
            dictionary,
            sink,
            unwritten: VecDeque::new(),
            errors: Vec::new(),
        }
    }

    pub(crate) fn dictionary(&self) -> &TextDictionary {
        &self.dictionary
    }

    pub(crate) fn write(&mut self, line: String) {
        if let Err(reason) = self.dictionary.append(&line) {
            self.errors.push(WriteError::Invalid(reason));
            return;
        }
        self.unwritten.push_back(line);
        while let Some(line) = self.unwritten.front() {
            if let Err(e) = self.sink.append(line) {
                self.errors.push(e.into());
                return;
            }
            self.unwritten.pop_front();
        }
    }

    /// Puts the dictionary read again in place, with the lines not yet
    /// written on top.
    pub(crate) fn replace(&mut self, dictionary: TextDictionary) {
        self.dictionary = dictionary;
        self.reapply();
    }

    /// Takes over the lines `previous` did not write and its errors.
    pub(crate) fn absorb(&mut self, previous: UserCustom) {
        let mut unwritten = previous.unwritten;
        unwritten.append(&mut self.unwritten);
        self.unwritten = unwritten;
        self.errors.extend(previous.errors);
        self.reapply();
    }

    /// Writes that failed since the last call.
    pub(crate) fn take_errors(&mut self) -> Vec<WriteError> {
        std::mem::take(&mut self.errors)
    }

    fn reapply(&mut self) {
        for line in &self.unwritten {
            let _ = self.dictionary.append(line);
        }
    }
}

impl TextDictionary {
    /// A missing file is an empty dictionary: it is created on the first write.
    pub fn read_user_custom(path: impl AsRef<Path>) -> io::Result<(Self, Vec<InvalidLine>)> {
        match fs::read(path) {
            Ok(bytes) => Ok(Self::parse_bytes(&bytes, true)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::parse_user_custom("")),
            Err(e) => Err(e),
        }
    }
}

/// Appends lines to a text dictionary file under the lock of [`lock`], so the
/// settings app and other IME processes never interleave a line.
pub struct FileSink {
    path: PathBuf,
}

impl FileSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl LineSink for FileSink {
    fn append(&mut self, line: &str) -> io::Result<()> {
        // Locked before opening: a rewrite may replace the file meanwhile.
        let _lock = lock(&self.path)?;
        let mut file =
            private(OpenOptions::new().read(true).append(true).create(true)).open(&self.path)?;
        append_line(&mut file, line)
    }
}

/// Takes out every line of a user custom dictionary file that hides
/// (`reading`, `surface`), under the same lock as [`FileSink`]. The other
/// lines stay byte for byte, and a missing file stays missing.
pub fn unhide(path: impl AsRef<Path>, reading: &str, surface: &str) -> io::Result<()> {
    let path = path.as_ref();
    let _lock = lock(path)?;
    let bytes = match fs::read(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        bytes => bytes?,
    };
    let mut kept = Vec::with_capacity(bytes.len());
    for (i, line) in bytes.split_inclusive(|&b| b == b'\n').enumerate() {
        // The parser drops a byte order mark at the start of the file only.
        let text = match line.strip_prefix("\u{feff}".as_bytes()) {
            Some(rest) if i == 0 => rest,
            _ => line,
        };
        let text = text.strip_suffix(b"\n").unwrap_or(text);
        let text = text.strip_suffix(b"\r").unwrap_or(text);
        let hides = std::str::from_utf8(text)
            .is_ok_and(|text| TextDictionary::hides(text, reading, surface));
        if !hides {
            kept.extend_from_slice(line);
        }
    }
    if kept.len() == bytes.len() {
        return Ok(());
    }
    replace_file(path, kept)
}

/// What a line is appended to: the file, or in tests one that fails.
trait LineFile: Write {
    fn len(&mut self) -> io::Result<u64>;
    fn last_byte(&mut self) -> io::Result<u8>;
    fn set_len(&mut self, len: u64) -> io::Result<()>;
}

impl LineFile for File {
    fn len(&mut self) -> io::Result<u64> {
        Ok(self.metadata()?.len())
    }

    fn last_byte(&mut self) -> io::Result<u8> {
        self.seek(SeekFrom::End(-1))?;
        let mut last = [0];
        self.read_exact(&mut last)?;
        Ok(last[0])
    }

    fn set_len(&mut self, len: u64) -> io::Result<()> {
        File::set_len(self, len)
    }
}

/// Appends `line` on a line of its own, or nothing: a write that fails part
/// way is cut off again, since a fragment of a character would make the whole
/// file unreadable as UTF-8 and the next line would join the fragment.
fn append_line(file: &mut impl LineFile, line: &str) -> io::Result<()> {
    let len = file.len()?;
    let mut text = String::new();
    if len > 0 && file.last_byte()? != b'\n' {
        text.push('\n');
    }
    text.push_str(line);
    text.push('\n');
    file.write_all(text.as_bytes())
        .map_err(|error| match file.set_len(len) {
            Ok(()) => error,
            Err(restore) => io::Error::new(
                error.kind(),
                format!("{error}; the part written was not cut off: {restore}"),
            ),
        })
}

/// Writes `bytes` to `path` by replacing the file whole, so a reader never
/// sees it half written and a failed write loses nothing. The file is the
/// user's alone, as everything it is used for holds what they type.
pub fn replace_file(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    let (path, bytes) = (path.as_ref(), bytes.as_ref());
    let mut partial = path.as_os_str().to_owned();
    // Unique to each call: threads of one process may write the same file.
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let write = WRITES.fetch_add(1, Ordering::Relaxed);
    partial.push(format!(".{}-{write}.partial", std::process::id()));
    let partial = PathBuf::from(partial);
    let written = private(OpenOptions::new().write(true).create(true).truncate(true))
        .open(&partial)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&partial, path));
    if written.is_err() {
        let _ = fs::remove_file(&partial);
    }
    written
}

/// Makes a file it creates readable and writable by its owner only.
fn private(options: &mut OpenOptions) -> &mut OpenOptions {
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(options, 0o600);
    options
}

/// Holds an exclusive lock on a file beside `path` until dropped. The lock is
/// not on `path` itself, as a rewrite replaces that file with a new one.
fn lock(path: &Path) -> io::Result<File> {
    let mut name = path.as_os_str().to_owned();
    name.push(".lock");
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(PathBuf::from(name))?;
    file.lock()?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file with room for only `room` more bytes.
    struct Full {
        bytes: Vec<u8>,
        room: usize,
        truncates: bool,
    }

    impl Write for Full {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let n = buf.len().min(self.room);
            if n == 0 {
                return Err(io::Error::other("disk full"));
            }
            self.bytes.extend_from_slice(&buf[..n]);
            self.room -= n;
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl LineFile for Full {
        fn len(&mut self) -> io::Result<u64> {
            Ok(self.bytes.len() as u64)
        }

        fn last_byte(&mut self) -> io::Result<u8> {
            Ok(*self.bytes.last().expect("not empty"))
        }

        fn set_len(&mut self, len: u64) -> io::Result<()> {
            if !self.truncates {
                return Err(io::Error::other("read-only"));
            }
            self.bytes.truncate(len as usize);
            Ok(())
        }
    }

    #[test]
    fn a_line_that_does_not_fit_leaves_the_file_as_it_was() {
        let before = "かく\t書く".as_bytes().to_vec();
        // Room for the newline and half of き.
        let mut file = Full {
            bytes: before.clone(),
            room: 2,
            truncates: true,
        };
        assert!(append_line(&mut file, "きしゃ\t記者").is_err());
        assert_eq!(file.bytes, before);
    }
    #[test]
    fn a_line_that_cannot_be_cut_off_again_says_so() {
        let mut file = Full {
            bytes: Vec::new(),
            room: 2,
            truncates: false,
        };
        let error = append_line(&mut file, "きしゃ\t記者").unwrap_err();
        assert!(error.to_string().contains("disk full"), "{error}");
        assert!(error.to_string().contains("read-only"), "{error}");
    }

    #[test]
    fn a_line_that_fits_is_appended_after_a_newline_it_lacked() {
        let mut file = Full {
            bytes: "かく\t書く".as_bytes().to_vec(),
            room: 100,
            truncates: true,
        };
        append_line(&mut file, "きしゃ\t記者").unwrap();
        assert_eq!(file.bytes, "かく\t書く\nきしゃ\t記者\n".as_bytes());
    }
}
