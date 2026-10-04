//! Files opened once per process. On Windows the IME runs in every thread
//! of an application that takes text, and each thread has an engine of its
//! own; a large dictionary read again for each would be held many times over.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use crate::dictionaries::{FileStamp, file_stamp};

/// What was opened from each file, while anything still holds it.
pub(crate) struct SharedFiles<T: ?Sized> {
    opened: Mutex<Vec<(PathBuf, FileStamp, Weak<T>)>>,
}

impl<T: ?Sized> SharedFiles<T> {
    pub(crate) const fn new() -> Self {
        Self {
            opened: Mutex::new(Vec::new()),
        }
    }

    /// What `path` holds, opened by `open` only when no one holds it as the
    /// file is now.
    pub(crate) fn get<E>(
        &self,
        path: &Path,
        open: impl FnOnce() -> Result<Arc<T>, E>,
    ) -> Result<Arc<T>, E> {
        let stamp = file_stamp(path);
        // Held while opening, so threads that want the same file wait for
        // it rather than read it again.
        let mut opened = self.opened.lock().unwrap_or_else(PoisonError::into_inner);
        opened.retain(|(_, _, held)| held.strong_count() > 0);
        if let Some(held) = opened
            .iter()
            .find(|(p, s, _)| p == path && *s == stamp)
            .and_then(|(_, _, held)| held.upgrade())
        {
            return Ok(held);
        }
        let fresh = open()?;
        opened.push((path.to_owned(), stamp, Arc::downgrade(&fresh)));
        Ok(fresh)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;

    use super::*;

    #[test]
    fn a_file_is_opened_once_while_it_is_held_and_unchanged() {
        let path =
            std::env::temp_dir().join(format!("kanaemi-runtime-shared-{}.txt", std::process::id()));
        fs::write(&path, "a").unwrap();
        let files = SharedFiles::<String>::new();
        let opened = Cell::new(0);
        let open = || {
            files.get(&path, || {
                opened.set(opened.get() + 1);
                Ok::<_, ()>(Arc::new(fs::read_to_string(&path).unwrap()))
            })
        };
        let first = open().unwrap();
        let second = open().unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(opened.get(), 1);
        fs::write(&path, "bb").unwrap();
        let changed = open().unwrap();
        assert_eq!(*changed, "bb", "a changed file is opened again");
        drop((first, second));
        assert!(Arc::ptr_eq(&open().unwrap(), &changed));
        assert_eq!(
            files.opened.lock().unwrap().len(),
            1,
            "what no one holds goes"
        );
    }

    #[test]
    fn a_file_no_one_holds_is_opened_again() {
        let path = std::env::temp_dir().join(format!(
            "kanaemi-runtime-shared-dropped-{}.txt",
            std::process::id()
        ));
        fs::write(&path, "a").unwrap();
        let files = SharedFiles::<String>::new();
        let opened = Cell::new(0);
        let open = || {
            files.get(&path, || {
                opened.set(opened.get() + 1);
                Ok::<_, ()>(Arc::new(fs::read_to_string(&path).unwrap()))
            })
        };
        drop(open().unwrap());
        drop(open().unwrap());
        assert_eq!(opened.get(), 2);
    }
}
