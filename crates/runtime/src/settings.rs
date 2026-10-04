//! The settings file, read when the IME starts and again whenever it has
//! changed by the time a field takes the focus.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use kanaemi_config::{FILE_NAME, Settings, read_or_create};

/// The settings `text` says, with each setting that could not be read
/// logged.
fn load(dir: &Path, text: &str) -> Settings {
    let (settings, problems) = Settings::load(text, dir);
    for problem in problems {
        tracing::warn!(
            item = problem.item,
            message = %problem.kind,
            "setting skipped"
        );
    }
    settings
}

/// When the settings file last changed, to tell whether to read it again.
pub(crate) fn modified(dir: &Path) -> Option<SystemTime> {
    fs::metadata(dir.join(FILE_NAME)).ok()?.modified().ok()
}

/// The settings in `dir`, writing the commented template there first when
/// there is no settings file, with the time of the file they were read
/// from: taken before reading, so a save meanwhile is read next time. A file
/// the template was just written to is stamped as written.
pub(crate) fn read_stamped(dir: &Path) -> (Settings, Option<SystemTime>) {
    let before = modified(dir);
    match read_or_create(dir) {
        Ok(text) => (load(dir, &text), before.or_else(|| modified(dir))),
        // Not stamped, so it is read again once it can be.
        Err(error) => {
            tracing::warn!(path = %dir.join(FILE_NAME).display(), %error, "settings unreadable; using the defaults");
            (load(dir, ""), None)
        }
    }
}
