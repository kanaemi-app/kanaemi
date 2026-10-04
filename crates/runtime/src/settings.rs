//! The settings file, read when the IME starts and again whenever it has
//! changed by the time a field takes the focus.

use std::path::Path;

use kanaemi_config::{FILE_NAME, Settings, read_or_create};
use kanaemi_engine::FileStamp;

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

/// The settings file as it stands, to tell whether to read it again.
pub(crate) fn stamp(dir: &Path) -> Option<FileStamp> {
    FileStamp::of(dir.join(FILE_NAME))
}

/// The settings in `dir`, writing the commented template there first when
/// there is no settings file, with the stamp of the file they were read
/// from: taken before reading, so a save meanwhile is read next time. A file
/// the template was just written to is stamped as written.
pub(crate) fn read_stamped(dir: &Path) -> (Settings, Option<FileStamp>) {
    let before = stamp(dir);
    match read_or_create(dir) {
        Ok(text) => (load(dir, &text), before.or_else(|| stamp(dir))),
        // Not stamped, so it is read again once it can be.
        Err(error) => {
            tracing::warn!(path = %dir.join(FILE_NAME).display(), %error, "settings unreadable; using the defaults");
            (load(dir, ""), None)
        }
    }
}
