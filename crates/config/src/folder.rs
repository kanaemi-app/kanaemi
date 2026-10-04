//! The settings folder: where it is, the files in it, and the dictionaries
//! read from it.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The settings file, in the settings folder.
pub const FILE_NAME: &str = "config.toml";
/// The user custom dictionary's file, in the settings folder.
pub const USER_CUSTOM_FILE: &str = "custom.tsv";
/// The ranking model, trained with the dictionaries; without it the rules rank.
pub const MODEL_FILE: &str = "ranking.model";
/// The record of which candidates the user picks. It tells what they type,
/// so only they may read it.
pub const SELECTIONS_FILE: &str = "selections.tsv";
/// The folder of dictionary files, in the settings folder.
pub const DICTIONARY_DIR: &str = "dictionaries";
/// The folder of romaji table files, in the settings folder.
pub const ROMAJI_DIR: &str = "romaji";
/// The extension of text dictionaries and romaji tables.
pub const TEXT_EXTENSION: &str = "tsv";
/// The extension of binary dictionaries.
pub(crate) const BINARY_EXTENSION: &str = "kdic";

/// Every setting with its default, commented out: what the IME puts in place
/// when there is no settings file yet.
pub const TEMPLATE: &str = include_str!("../assets/config.toml");

/// The settings folder on this platform; `None` where it is not known, as
/// without a home folder.
pub fn dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    return Some(
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/kanaemi"),
    );
    #[cfg(windows)]
    return Some(PathBuf::from(std::env::var_os("APPDATA")?).join("kanaemi"));
    #[cfg(all(unix, not(target_os = "macos")))]
    return Some(xdg_dir("XDG_CONFIG_HOME", ".config")?.join("kanaemi"));
    #[cfg(not(any(unix, windows)))]
    return None;
}

/// Where the IME writes its log: outside the settings folder.
pub fn log_file() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    return Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Logs/kanaemi.log"));
    #[cfg(windows)]
    return Some(
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
            .join("kanaemi")
            .join("kanaemi.log"),
    );
    #[cfg(all(unix, not(target_os = "macos")))]
    return Some(
        xdg_dir("XDG_STATE_HOME", ".local/state")?
            .join("kanaemi")
            .join("kanaemi.log"),
    );
    #[cfg(not(any(unix, windows)))]
    return None;
}

/// The XDG base directory `variable` names on this system.
#[cfg(all(unix, not(target_os = "macos")))]
fn xdg_dir(variable: &str, fallback: &str) -> Option<PathBuf> {
    xdg_base(
        std::env::var_os(variable),
        std::env::var_os("HOME"),
        fallback,
    )
}

/// An XDG base directory: `value` when it is an absolute path, as the
/// specification asks so that a stray relative value points nowhere
/// unexpected, or else `fallback` in the `home` folder.
#[cfg(all(unix, any(test, not(target_os = "macos"))))]
fn xdg_base(
    value: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    fallback: &str,
) -> Option<PathBuf> {
    value
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| Some(PathBuf::from(home?).join(fallback)))
}

/// The text of the settings file in `dir`, writing the template there first
/// when there is no file, so that what can be set is there to read.
pub fn read_or_create(dir: impl AsRef<Path>) -> io::Result<String> {
    let dir = dir.as_ref();
    let path = dir.join(FILE_NAME);
    if !path.exists() {
        fs::create_dir_all(dir)?;
        // The IME and the settings app may both start without a file: the
        // first to create it writes the template, and the other reads it.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => io::Write::write_all(&mut file, TEMPLATE.as_bytes())?,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    fs::read_to_string(path)
}

/// What a romaji table or dictionary file says it is: its first line, when
/// that line is a comment.
pub fn description(text: impl AsRef<str>) -> Option<String> {
    let first = text
        .as_ref()
        .trim_start_matches('\u{feff}')
        .lines()
        .next()?;
    let comment = first.strip_prefix('#')?.trim();
    (!comment.is_empty()).then(|| comment.to_owned())
}

/// A dictionary in the ordered list of dictionaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DictionarySource {
    UserCustom,
    File(PathBuf),
    /// A binary dictionary standing for the text dictionary of the same name
    /// it was converted from. Should the binary one fail to open, the text
    /// one is read instead, so the words are not lost.
    Converted {
        binary: PathBuf,
        text: PathBuf,
    },
}

/// The dictionaries to read, in order: `listed`, or else the user custom
/// dictionary followed by the dictionary files in `dir`'s dictionary folder.
pub fn dictionary_sources(
    dir: impl AsRef<Path>,
    listed: Option<&[DictionarySource]>,
) -> Vec<DictionarySource> {
    if let Some(listed) = listed {
        return listed.to_vec();
    }
    let folder = dir.as_ref().join(DICTIONARY_DIR);
    let files = dictionary_files(&folder);
    let text_of = |name: &String| {
        let text = Path::new(name).with_extension(TEXT_EXTENSION);
        let text = text.to_string_lossy().into_owned();
        (Path::new(name).extension().and_then(|e| e.to_str()) == Some(BINARY_EXTENSION)
            && files.contains(&text))
        .then_some(text)
    };
    let shadowed = |name: &String| {
        Path::new(name).extension().and_then(|e| e.to_str()) == Some(TEXT_EXTENSION)
            && files.contains(&binary_name(name))
    };
    std::iter::once(DictionarySource::UserCustom)
        .chain(
            files
                .iter()
                .filter(|name| !shadowed(name))
                .map(|name| match text_of(name) {
                    Some(text) => DictionarySource::Converted {
                        binary: folder.join(name),
                        text: folder.join(text),
                    },
                    None => DictionarySource::File(folder.join(name)),
                }),
        )
        .collect()
}

/// The text and binary dictionary files in `folder` and the folders in it,
/// by name: a file in a folder is named with `/` after the folder's name, as
/// the dictionary list writes it.
pub fn dictionary_files(folder: impl AsRef<Path>) -> Vec<String> {
    let mut names = Vec::new();
    collect_dictionary_files(folder.as_ref(), "", &mut names);
    names.sort();
    names
}

fn collect_dictionary_files(folder: &Path, prefix: &str, names: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        // A link to a folder is not followed, so a link back up cannot loop.
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            collect_dictionary_files(&entry.path(), &format!("{prefix}{name}/"), names);
        } else if Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| [TEXT_EXTENSION, BINARY_EXTENSION].contains(&e))
        {
            names.push(format!("{prefix}{name}"));
        }
    }
}

/// The binary dictionary converted from the text dictionary `name`: the same
/// name with the binary extension in place of its own.
pub fn binary_name(name: impl AsRef<str>) -> String {
    Path::new(name.as_ref())
        .with_extension(BINARY_EXTENSION)
        .to_string_lossy()
        .into_owned()
}

/// Whether a file named `name` in a folder stays in that folder: not
/// absolute, and never going up with `..`.
pub(crate) fn stays_inside(name: impl AsRef<Path>) -> bool {
    name.as_ref()
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

// The paths are Unix paths, which Windows does not count as absolute.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn an_absolute_xdg_variable_names_the_base_folder() {
        assert_eq!(
            xdg_base(
                Some("/xdg/config".into()),
                Some("/home/u".into()),
                ".config"
            ),
            Some(PathBuf::from("/xdg/config"))
        );
    }

    #[test]
    fn a_relative_or_missing_xdg_variable_falls_back_to_the_home_folder() {
        for value in [Some("relative/config".into()), Some("".into()), None] {
            assert_eq!(
                xdg_base(value, Some("/home/u".into()), ".config"),
                Some(PathBuf::from("/home/u/.config"))
            );
        }
    }

    #[test]
    fn without_a_home_folder_only_an_absolute_xdg_variable_counts() {
        assert_eq!(xdg_base(None, None, ".config"), None);
    }
}
