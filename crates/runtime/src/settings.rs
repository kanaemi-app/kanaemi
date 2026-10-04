//! The settings file, read when the IME starts and again whenever it has
//! changed by the time a field takes the focus.

use std::fs;
use std::io;
use std::path::Path;

use kanaemi_config::{FILE_NAME, Settings, read_or_create};
use kanaemi_engine::FileStamp;

use crate::Access;

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

/// The settings in `dir`, with the stamp of the file they were read from:
/// taken before reading, so a save meanwhile is read next time. With full
/// access the commented template is written there first when there is no
/// settings file, and a file it was just written to is stamped as written;
/// a sandbox writes nothing and reads a missing file as the defaults.
pub(crate) fn read_stamped(dir: &Path, access: Access) -> (Settings, Option<FileStamp>) {
    let before = stamp(dir);
    let text = match access {
        Access::Full => read_or_create(dir),
        Access::Sandboxed(_) => match fs::read_to_string(dir.join(FILE_NAME)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
            read => read,
        },
    };
    match text {
        Ok(text) => (load(dir, &text), before.or_else(|| stamp(dir))),
        // Not stamped, so it is read again once it can be.
        Err(error) => {
            tracing::warn!(path = %dir.join(FILE_NAME).display(), %error, "settings unreadable; using the defaults");
            (load(dir, ""), None)
        }
    }
}
