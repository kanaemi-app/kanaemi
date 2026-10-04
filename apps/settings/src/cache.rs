//! Answers worked out from files, kept while the files stay as they are:
//! hashing or reading a large dictionary on every redraw would be slow.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use kanaemi_engine::FileStamp;

/// `None` when the file is missing.
type Stamp = Option<FileStamp>;
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
        let stamps: Vec<Stamp> = paths.iter().map(FileStamp::of).collect();
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;

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

    /// A clock coarser than two writes leaves a file replaced with as many
    /// bytes at the same time, as when a dictionary is converted twice.
    #[cfg(unix)]
    #[test]
    fn a_file_replaced_within_the_same_tick_is_worked_out_again() {
        let path = std::env::temp_dir().join(format!(
            "kanaemi-settings-cache-tick-{}.txt",
            std::process::id()
        ));
        kanaemi_engine::replace_file(&path, "a").unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let cache = FileCache::default();
        let read = || {
            cache.get(std::slice::from_ref(&path), || {
                fs::read_to_string(&path).unwrap()
            })
        };
        assert_eq!(read(), "a");
        kanaemi_engine::replace_file(&path, "b").unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_eq!(read(), "b");
    }
}
