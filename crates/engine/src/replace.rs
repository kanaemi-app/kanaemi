//! A file put in place of another whole, even one a reader has mapped into
//! memory, as the IME does a binary dictionary.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// What follows a file's name in the name it is set aside under.
const SET_ASIDE: &str = "replaced";

/// Moves the file `from` to `to`, in place of the file there.
///
/// Windows refuses to replace a file that a process has mapped into memory,
/// as each application the IME runs in does its binary dictionaries, but lets
/// it be renamed. So when the move is refused and a file is in the way, that
/// file is set aside under another name, `<name>.<process>-<n>.replaced`,
/// first. It is removed at once if it can be; if not, by
/// [`remove_set_aside`] once no one reads it.
pub fn move_into_place(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    move_with(from.as_ref(), to.as_ref(), &|from, to| fs::rename(from, to))
}

fn move_with(
    from: &Path,
    to: &Path,
    rename: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let refused = match rename(from, to) {
        Ok(()) => return Ok(()),
        Err(refused) => refused,
    };
    let is_file = |path: &Path| fs::symlink_metadata(path).is_ok_and(|m| m.is_file());
    if !is_file(from) || !is_file(to) {
        return Err(refused);
    }
    let aside = set_aside_name(to);
    if rename(to, &aside).is_err() {
        return Err(refused);
    }
    if let Err(error) = rename(from, to) {
        // Back where it was, so a failed move loses nothing.
        return match rename(&aside, to) {
            Ok(()) => Err(error),
            Err(restore) => Err(io::Error::new(
                error.kind(),
                format!(
                    "{error}; the file it was to replace is left at {}: {restore}",
                    aside.display()
                ),
            )),
        };
    }
    let _ = fs::remove_file(&aside);
    Ok(())
}

/// A name beside `path` that no other move takes.
fn set_aside_name(path: &Path) -> PathBuf {
    // Unique to each call: threads of one process may move the same file.
    static MOVES: AtomicU64 = AtomicU64::new(0);
    let n = MOVES.fetch_add(1, Ordering::Relaxed);
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{}-{n}.{SET_ASIDE}", std::process::id()));
    PathBuf::from(name)
}

/// Removes the files [`move_into_place`] set aside from `path` and could not
/// remove then. One still mapped stays, to be removed another time.
pub fn remove_set_aside(path: impl AsRef<Path>) {
    let path = path.as_ref();
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if is_set_aside(&entry.file_name(), name) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Whether `candidate` is the name a file named `name` is set aside under.
fn is_set_aside(candidate: &OsStr, name: &OsStr) -> bool {
    let (Some(candidate), Some(name)) = (candidate.to_str(), name.to_str()) else {
        return false;
    };
    candidate
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('.'))
        .and_then(|rest| rest.strip_suffix(SET_ASIDE))
        .and_then(|rest| rest.strip_suffix('.'))
        .and_then(|stamp| stamp.split_once('-'))
        .is_some_and(|(process, n)| {
            [process, n]
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashSet;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kanaemi-engine-replace-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Renames as Windows does with a file mapped into memory: it can be
    /// moved away but nothing can be moved over it.
    struct Mapped(RefCell<HashSet<PathBuf>>);

    impl Mapped {
        fn new(paths: &[&Path]) -> Self {
            Self(RefCell::new(
                paths.iter().map(|p| p.to_path_buf()).collect(),
            ))
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            if self.0.borrow().contains(to) && to.exists() {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            fs::rename(from, to)?;
            let mut mapped = self.0.borrow_mut();
            if mapped.remove(from) {
                mapped.insert(to.to_owned());
            }
            Ok(())
        }
    }

    #[test]
    fn a_file_moves_over_another() {
        let dir = temp_dir("plain");
        fs::write(dir.join("a.kdic.partial"), "new").unwrap();
        fs::write(dir.join("a.kdic"), "old").unwrap();
        move_into_place(dir.join("a.kdic.partial"), dir.join("a.kdic")).unwrap();
        assert_eq!(fs::read_to_string(dir.join("a.kdic")).unwrap(), "new");
        assert_eq!(names(&dir), ["a.kdic"]);
    }

    #[test]
    fn a_file_that_cannot_be_moved_over_is_set_aside_first() {
        let dir = temp_dir("mapped");
        let (from, to) = (dir.join("a.kdic.partial"), dir.join("a.kdic"));
        fs::write(&from, "new").unwrap();
        fs::write(&to, "old").unwrap();
        let mapped = Mapped::new(&[&to]);
        move_with(&from, &to, &|f, t| mapped.rename(f, t)).unwrap();
        assert_eq!(fs::read_to_string(&to).unwrap(), "new");
        assert_eq!(names(&dir), ["a.kdic"], "the old file is removed at once");
    }

    #[test]
    fn only_a_file_is_set_aside() {
        let dir = temp_dir("folder");
        let (from, to) = (dir.join("a.kdic.partial"), dir.join("a.kdic"));
        fs::write(&from, "new").unwrap();
        fs::create_dir(&to).unwrap();
        fs::write(to.join("inside"), "").unwrap();
        assert!(move_into_place(&from, &to).is_err());
        assert_eq!(names(&dir), ["a.kdic", "a.kdic.partial"]);
        assert_eq!(names(&to), ["inside"]);
    }

    #[test]
    fn files_left_set_aside_are_removed_later() {
        let dir = temp_dir("left");
        let to = dir.join("a.kdic");
        fs::write(&to, "new").unwrap();
        fs::write(dir.join("a.kdic.3-4.replaced"), "old").unwrap();
        fs::write(dir.join("a.kdic.5-0.replaced"), "older").unwrap();
        // A folder stands for a file still mapped, which Windows refuses to
        // remove as `remove_file` does a folder.
        fs::create_dir(dir.join("a.kdic.6-1.replaced")).unwrap();
        fs::write(dir.join("b.kdic.1-2.replaced"), "another file's").unwrap();
        fs::write(dir.join("a.kdic.mine.replaced"), "the user's").unwrap();
        remove_set_aside(&to);
        assert_eq!(
            names(&dir),
            [
                "a.kdic",
                "a.kdic.6-1.replaced",
                "a.kdic.mine.replaced",
                "b.kdic.1-2.replaced"
            ]
        );
    }

    #[test]
    fn a_failed_move_leaves_the_old_file_in_place() {
        let dir = temp_dir("failed");
        let (from, to) = (dir.join("a.kdic.partial"), dir.join("a.kdic"));
        fs::write(&from, "new").unwrap();
        fs::write(&to, "old").unwrap();
        // Refuses the new file but not the old one: the old one goes back.
        let refusing = |f: &Path, t: &Path| {
            if f == from {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            fs::rename(f, t)
        };
        assert!(move_with(&from, &to, &refusing).is_err());
        assert_eq!(fs::read_to_string(&to).unwrap(), "old");
        assert_eq!(names(&dir), ["a.kdic", "a.kdic.partial"]);
    }

    #[test]
    fn a_missing_file_is_not_moved() {
        let dir = temp_dir("missing");
        fs::write(dir.join("a.kdic"), "old").unwrap();
        assert!(move_into_place(dir.join("a.kdic.partial"), dir.join("a.kdic")).is_err());
        assert_eq!(names(&dir), ["a.kdic"]);
    }

    #[test]
    fn only_names_set_aside_from_the_file_are_known() {
        let is = |c: &str| is_set_aside(OsStr::new(c), OsStr::new("a.kdic"));
        assert!(is("a.kdic.12-0.replaced"));
        for other in [
            "a.kdic",
            "a.kdic.replaced",
            "a.kdic.-0.replaced",
            "a.kdic.12-.replaced",
            "a.kdic.1x-0.replaced",
            "a.kdic.12-0.partial",
            "b.a.kdic.12-0.replaced",
            "a.kdic12-0.replaced",
        ] {
            assert!(!is(other), "{other}");
        }
    }
}
