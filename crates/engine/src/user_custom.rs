//! The user custom dictionary: the text dictionary registrations and
//! deletions are written to, and the file it lives in.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::{InvalidLine, InvalidReason, TextDictionary, move_into_place};

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
/// memory and is written again before the next one, or when
/// [`UserCustom::flush`] is called.
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
        self.flush();
    }

    /// Writes the lines not yet written, oldest first, up to the first that
    /// fails again.
    pub(crate) fn flush(&mut self) {
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

/// Appends lines to a text dictionary file under the lock of [`FileLock`], so the
/// settings app and other IME processes never interleave a line.
///
/// It is called while the IME handles a key, so it waits for the lock only a
/// short while: when another holds it longer, such as the settings app
/// rewriting the file, the line is not written and the append fails with
/// [`io::ErrorKind::WouldBlock`], to be written again later.
pub struct FileSink {
    path: PathBuf,
    wait: Duration,
}

impl FileSink {
    /// How long [`FileSink::new`] waits for the lock.
    pub const WAIT: Duration = Duration::from_millis(50);

    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::waiting(path, Self::WAIT)
    }

    /// A sink that waits for the lock at most `wait`.
    pub fn waiting(path: impl Into<PathBuf>, wait: Duration) -> Self {
        Self {
            path: path.into(),
            wait,
        }
    }
}

impl LineSink for FileSink {
    fn append(&mut self, line: &str) -> io::Result<()> {
        // Locked before opening: a rewrite may replace the file meanwhile.
        let Some(_lock) = FileLock::try_hold(&self.path, self.wait)? else {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "locked elsewhere; written again later",
            ));
        };
        let mut file =
            private(OpenOptions::new().read(true).append(true).create(true)).open(&self.path)?;
        append_line(&mut file, line)
    }
}

/// Takes out every line of a user custom dictionary file that hides
/// (`reading`, `surface`), under the same lock as [`FileSink`]. The other
/// lines stay byte for byte, and a missing file stays missing.
pub fn unhide(path: impl AsRef<Path>, reading: &str, surface: &str) -> io::Result<()> {
    remove_lines(path.as_ref(), |text| {
        TextDictionary::hides(text, reading, surface)
    })
}

/// For each (reading, surface) of `pairs`, whether a word line of a user
/// custom dictionary file gives it, as a word registered and then hidden
/// is. The file is read once; a missing file gives nothing.
pub fn registered(path: impl AsRef<Path>, pairs: &[(&str, &str)]) -> io::Result<Vec<bool>> {
    let mut found = vec![false; pairs.len()];
    let bytes = match fs::read(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(found),
        bytes => bytes?,
    };
    for registration in lines(&bytes).filter_map(|(_, text)| TextDictionary::registration(text?)) {
        for (found, (reading, surface)) in found.iter_mut().zip(pairs) {
            *found = *found || registration.gives(reading, surface);
        }
    }
    Ok(found)
}

/// Takes out the lines that give (`reading`, `surface`) and every line that
/// hides it, as [`unhide`] does: the pair is as it was before it was
/// registered.
pub fn unregister(path: impl AsRef<Path>, reading: &str, surface: &str) -> io::Result<()> {
    remove_lines(path.as_ref(), |text| {
        TextDictionary::hides(text, reading, surface)
            || TextDictionary::registration(text).is_some_and(|r| r.gives(reading, surface))
    })
}

/// Rewrites the file without the lines `remove` picks, under the same lock as
/// [`FileSink`]. The other lines stay byte for byte, a missing file stays
/// missing, and a file that loses no line is left alone.
fn remove_lines(path: &Path, remove: impl Fn(&str) -> bool) -> io::Result<()> {
    let _lock = FileLock::hold(path)?;
    let bytes = match fs::read(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        bytes => bytes?,
    };
    let mut kept = Vec::with_capacity(bytes.len());
    for (line, text) in lines(&bytes) {
        if !text.is_some_and(&remove) {
            kept.extend_from_slice(line);
        }
    }
    if kept.len() == bytes.len() {
        return Ok(());
    }
    replace_file(path, kept)
}

/// Each line of a file with its line ending, and its text when it is UTF-8.
fn lines(bytes: &[u8]) -> impl Iterator<Item = (&[u8], Option<&str>)> {
    bytes
        .split_inclusive(|&b| b == b'\n')
        .enumerate()
        .map(|(i, line)| {
            // The parser drops a byte order mark at the start of the file only.
            let text = match line.strip_prefix("\u{feff}".as_bytes()) {
                Some(rest) if i == 0 => rest,
                _ => line,
            };
            let text = text.strip_suffix(b"\n").unwrap_or(text);
            let text = text.strip_suffix(b"\r").unwrap_or(text);
            (line, std::str::from_utf8(text).ok())
        })
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
    replace(path.as_ref(), bytes.as_ref(), true)
}

/// Like [`replace_file`], without waiting for the bytes to reach the disk
/// before the file is replaced, which may take hundreds of milliseconds. A
/// reader still never sees the file half written, and a process that
/// crashes loses nothing; only a crash of the whole system or a power cut
/// soon after may leave the file as it was before, or on some file systems
/// empty or zeroed. For files the IME writes while it handles a key.
pub fn replace_file_unsynced(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    replace(path.as_ref(), bytes.as_ref(), false)
}

fn replace(path: &Path, bytes: &[u8], sync: bool) -> io::Result<()> {
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
            if sync { file.sync_all() } else { Ok(()) }
        })
        .and_then(|()| move_into_place(&partial, path));
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

/// An exclusive lock on a file beside another, `<file>.lock`, held until
/// dropped. It is taken by every thread and process that writes the file, so
/// none writes between another's read and write. The lock is not on the file
/// itself, as a rewrite replaces that file with a new one.
#[derive(Debug)]
pub struct FileLock {
    _file: File,
}

impl FileLock {
    /// Waits as long as it takes for the lock on `path`.
    pub fn hold(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = Self::open(path.as_ref())?;
        file.lock()?;
        Ok(Self { _file: file })
    }

    /// The lock on `path` when it can be taken within `within`; `None` when
    /// another holds it all that time.
    pub fn try_hold(path: impl AsRef<Path>, within: Duration) -> io::Result<Option<Self>> {
        let file = Self::open(path.as_ref())?;
        let deadline = Instant::now() + within;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Some(Self { _file: file })),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(TryLockError::WouldBlock) => return Ok(None),
                Err(TryLockError::Error(error)) => return Err(error),
            }
        }
    }

    fn open(path: &Path) -> io::Result<File> {
        let mut name = path.as_os_str().to_owned();
        name.push(".lock");
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(PathBuf::from(name))
    }
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;

    use super::*;
    use crate::test_support::dictionary_text;

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

    proptest! {
        // A rewrite keeps every line it does not take out byte for byte.
        #[test]
        fn the_lines_of_any_file_make_up_the_file(
            bytes in prop_oneof![
                vec(any::<u8>(), 0..256),
                dictionary_text().prop_map(String::into_bytes),
            ],
        ) {
            let mut joined = Vec::new();
            for (line, text) in lines(&bytes) {
                joined.extend_from_slice(line);
                if let Some(text) = text {
                    prop_assert!(!text.contains('\n'));
                    TextDictionary::registration(text);
                }
            }
            prop_assert_eq!(joined, bytes);
        }
    }
}
