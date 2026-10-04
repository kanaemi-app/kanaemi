//! What tells that a file changed since it was last looked at, without
//! reading it.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// A file as it stands: equal stamps mean the file is taken to be unchanged.
///
/// When it last changed and how long it is miss a rewrite as long as the one
/// before within one tick of the file system's clock, which on Linux is a few
/// milliseconds. On Unix the stamp also holds which file it is and when its
/// inode last changed, so a file replaced whole with [`crate::replace_file`]
/// always gets another stamp. Windows keeps times to 100 ns and gives no file
/// identity on stable Rust. A file rewritten in place by another program,
/// with as many bytes and within one tick, still goes unseen; reading it to
/// tell would cost what the stamp is there to save.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
    #[cfg(unix)]
    identity: (u64, u64),
    #[cfg(unix)]
    changed: (i64, i64),
}

impl FileStamp {
    /// The stamp of the file at `path`; `None` when it cannot be read.
    pub fn of(path: impl AsRef<Path>) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Some(Self {
            modified: metadata.modified().ok(),
            len: metadata.len(),
            #[cfg(unix)]
            identity: (metadata.dev(), metadata.ino()),
            #[cfg(unix)]
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::path::PathBuf;

    use super::*;
    #[cfg(unix)]
    use crate::replace_file;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kanaemi-engine-stamp-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("file")
    }

    /// Sets the time the file last changed, as a clock too coarse to tell two
    /// writes apart would leave it.
    fn set_modified(path: &Path, time: std::time::SystemTime) {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }

    #[test]
    fn a_file_left_alone_keeps_its_stamp() {
        let path = temp_file("alone");
        fs::write(&path, "a").unwrap();
        assert_eq!(FileStamp::of(&path), FileStamp::of(&path));
    }

    #[test]
    fn a_missing_file_has_no_stamp() {
        assert_eq!(FileStamp::of(temp_file("missing")), None);
    }

    #[test]
    fn a_longer_file_has_another_stamp() {
        let path = temp_file("longer");
        fs::write(&path, "a").unwrap();
        let before = FileStamp::of(&path);
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, "ab").unwrap();
        set_modified(&path, modified);
        assert_ne!(FileStamp::of(&path), before);
    }

    /// A clock coarser than two writes leaves a file replaced with as many
    /// bytes at the same time: what tells them apart is that the file was
    /// replaced. Windows keeps times to 100 ns, and the stamp relies on that.
    #[cfg(unix)]
    #[test]
    fn a_file_replaced_within_the_same_tick_has_another_stamp() {
        let path = temp_file("replaced");
        replace_file(&path, "a").unwrap();
        let before = FileStamp::of(&path);
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        replace_file(&path, "b").unwrap();
        set_modified(&path, modified);
        assert_ne!(FileStamp::of(&path), before);
    }
}
