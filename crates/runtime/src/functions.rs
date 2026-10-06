//! The functions the user writes in the settings folder, read again when a
//! file of theirs changes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use kanaemi_config::{BUILTIN_PREFIX, FUNCTIONS_DIR};
use kanaemi_functions::{EXTENSION, LuauFunctions, Without};

use crate::dictionaries::{FileStamp, file_stamp};

/// Every file of the functions and their modules, with its stamp.
pub(crate) type Stamp = Vec<(PathBuf, FileStamp)>;

pub(crate) fn stamp(support_dir: &Path) -> Stamp {
    let mut files = Vec::new();
    collect(
        &support_dir.join(FUNCTIONS_DIR),
        &mut HashSet::new(),
        &mut files,
    );
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let stamp = file_stamp(&path);
            (path, stamp)
        })
        .collect()
}

/// The module files in `dir` at any depth, as `require` reaches them. A
/// folder already `seen` is not entered again, so a symlink that loops ends.
fn collect(dir: &Path, seen: &mut HashSet<PathBuf>, files: &mut Vec<PathBuf>) {
    let Ok(real) = dir.canonicalize() else {
        return;
    };
    if !seen.insert(real) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            collect(&path, seen, files);
        } else if path.is_file() && path.extension().is_some_and(|e| e == EXTENSION) {
            files.push(path);
        }
    }
}

/// The functions in the settings folder but those `disabled` names, as the
/// settings write them, with what was wrong in them logged.
pub(crate) fn open(support_dir: &Path, disabled: &[String]) -> Rc<LuauFunctions> {
    let mut without = Without::default();
    for name in disabled {
        match name.strip_prefix(BUILTIN_PREFIX) {
            Some(builtin) => without.builtins.push(builtin.to_owned()),
            None => without.files.push(name.clone()),
        }
    }
    let functions = LuauFunctions::open_without(support_dir.join(FUNCTIONS_DIR), &without);
    let count = functions.names().count();
    if count > 0 {
        tracing::info!(count, "functions loaded");
    }
    log(&functions);
    Rc::new(functions)
}

/// Logs what the functions printed and how they failed since the last call.
pub(crate) fn log(functions: &LuauFunctions) {
    for printed in functions.take_printed() {
        tracing::info!(
            function = printed.function,
            text = printed.text,
            "function printed"
        );
    }
    for error in functions.take_errors() {
        tracing::warn!(%error, "function skipped");
    }
}
