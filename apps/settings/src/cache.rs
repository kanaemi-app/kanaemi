//! Answers worked out from files, kept while the files stay as they are:
//! hashing or reading a large dictionary on every redraw would be slow.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// When a file last changed and how long it is; `None` when it is missing.
type Stamp = Option<(SystemTime, u64)>;
/// An answer with the stamps of the files it was worked out from.
type Known<T> = (Vec<Stamp>, T);

pub struct FileCache<T> {
    /// The last answer for each set of files, with their stamps then.
    known: RefCell<HashMap<Vec<PathBuf>, Known<T>>>,
}

impl<T> Default for FileCache<T> {
    fn default() -> Self {
        Self {
            known: RefCell::new(HashMap::new()),
        }
    }
}

impl<T: Clone> FileCache<T> {
    /// The answer for `paths`, worked out again only when one of them
    /// changed since. Only the latest answer is kept for each set of files.
    pub fn get(&self, paths: &[PathBuf], work_out: impl FnOnce() -> T) -> T {
        let stamps: Vec<Stamp> = paths.iter().map(|p| stamp(p)).collect();
        if let Some((known, answer)) = self.known.borrow().get(paths)
            && *known == stamps
        {
            return answer.clone();
        }
        let answer = work_out();
        self.known
            .borrow_mut()
            .insert(paths.to_vec(), (stamps, answer.clone()));
        answer
    }
}

fn stamp(path: &Path) -> Stamp {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn an_answer_is_worked_out_again_only_after_a_change() {
        let path =
            std::env::temp_dir().join(format!("kanaemi-settings-cache-{}.txt", std::process::id()));
        fs::write(&path, "a").unwrap();
        let cache = FileCache::default();
        let worked = Cell::new(0);
        let read = || {
            cache.get(std::slice::from_ref(&path), || {
                worked.set(worked.get() + 1);
                fs::read_to_string(&path).unwrap()
            })
        };
        assert_eq!(read(), "a");
        assert_eq!(read(), "a");
        assert_eq!(worked.get(), 1);
        fs::write(&path, "bb").unwrap();
        assert_eq!(read(), "bb");
        assert_eq!(worked.get(), 2);
        assert_eq!(cache.known.borrow().len(), 1, "the old answer is gone");
    }
}
