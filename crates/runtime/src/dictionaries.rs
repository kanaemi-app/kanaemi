//! The engine opened from the settings folder, and the stamps that tell when
//! its files changed. The user custom dictionary has a stamp of its own: it is
//! small, and the IME writes it itself, so a change reads only it again.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kanaemi_config::{
    DictionarySource, MODEL_FILE, SELECTIONS_FILE, USER_CUSTOM_FILE, dictionary_sources,
};
use kanaemi_engine::{
    Dictionary, DictionaryError, Engine, FileSink, RankingModel, Selections, Slot, TextDictionary,
    open_dictionary, replace_file,
};

/// A dictionary that cannot be read is skipped; without any the IME still
/// types kana and katakana.
pub(crate) fn open_engine(support_dir: &Path, sources: Option<&[DictionarySource]>) -> Engine {
    let slots = dictionary_sources(support_dir, sources)
        .into_iter()
        .filter_map(|source| match source {
            DictionarySource::UserCustom => Some(Slot::UserCustom),
            DictionarySource::File(path) => open(&path).map(Slot::Dictionary),
            DictionarySource::Converted { binary, text } => open_converted(&binary, &text),
        })
        .collect::<Vec<_>>();
    let mut engine = Engine::new(
        slots,
        read_user(support_dir),
        FileSink::new(support_dir.join(USER_CUSTOM_FILE)),
    );
    engine.set_model(read_model(support_dir).map(Arc::new));
    engine
}

/// The dictionary in `path`, or `None` after a warning when it cannot be read.
fn open(path: &Path) -> Option<Box<dyn Dictionary>> {
    try_open(path)
        .inspect_err(
            |error| tracing::warn!(path = %path.display(), %error, "dictionary unreadable"),
        )
        .ok()
}

/// The binary dictionary, or the text one it was converted from when the
/// binary one cannot be opened, so its words are not lost.
fn open_converted(binary: &Path, text: &Path) -> Option<Slot> {
    let dictionary = try_open(binary).or_else(|error| {
        tracing::warn!(path = %binary.display(), %error, text = %text.display(), "binary dictionary unreadable; reading its text dictionary");
        try_open(text)
    });
    dictionary
        .inspect_err(
            |error| tracing::warn!(path = %text.display(), %error, "dictionary unreadable"),
        )
        .ok()
        .map(Slot::Dictionary)
}

fn try_open(path: &Path) -> Result<Box<dyn Dictionary>, DictionaryError> {
    let (dictionary, invalid) = open_dictionary(path)?;
    tracing::info!(path = %path.display(), invalid = invalid.len(), "dictionary loaded");
    Ok(dictionary)
}

/// A missing model ranks by the rules; a broken one too, after a warning.
fn read_model(support_dir: &Path) -> Option<RankingModel> {
    let path = support_dir.join(MODEL_FILE);
    if !path.exists() {
        return None;
    }
    RankingModel::open(&path)
        .inspect(|_| tracing::info!(path = %path.display(), "ranking model loaded"))
        .inspect_err(
            |error| tracing::warn!(path = %path.display(), %error, "ranking model unreadable"),
        )
        .ok()
}

/// The record of picks; a missing one is empty. `None` when it is there but
/// cannot be read, so that it is not written over with picks that lack it.
pub(crate) fn read_selections(support_dir: &Path) -> Option<Selections> {
    let path = support_dir.join(SELECTIONS_FILE);
    match fs::read_to_string(&path) {
        Ok(text) => Some(Selections::parse(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Some(Selections::default()),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "record of picks unreadable; not written until it can be read");
            None
        }
    }
}

/// Whether the record was written.
pub(crate) fn save_selections(support_dir: &Path, selections: &Selections) -> bool {
    let path = support_dir.join(SELECTIONS_FILE);
    replace_file(&path, selections.to_text())
        .inspect_err(
            |error| tracing::warn!(path = %path.display(), %error, "record of picks not written"),
        )
        .is_ok()
}

/// An unreadable user custom dictionary reads as an empty one.
pub(crate) fn read_user(support_dir: &Path) -> TextDictionary {
    let path = support_dir.join(USER_CUSTOM_FILE);
    match TextDictionary::read_user_custom(&path) {
        Ok((dictionary, invalid)) => {
            if !invalid.is_empty() {
                tracing::warn!(path = %path.display(), ?invalid, "invalid dictionary lines skipped");
            }
            dictionary
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "user custom dictionary unreadable");
            TextDictionary::parse_user_custom("").0
        }
    }
}

/// A file as it stands, to tell when it changed; `None` when it is missing.
pub(crate) type FileStamp = Option<kanaemi_engine::FileStamp>;

fn file_stamp(path: &Path) -> FileStamp {
    kanaemi_engine::FileStamp::of(path)
}

pub(crate) fn user_stamp(support_dir: &Path) -> FileStamp {
    file_stamp(&support_dir.join(USER_CUSTOM_FILE))
}

pub(crate) fn selections_stamp(support_dir: &Path) -> FileStamp {
    file_stamp(&support_dir.join(SELECTIONS_FILE))
}

/// The dictionary files in use with the stamp of each: when it changes, the
/// dictionaries are opened again.
pub(crate) type Stamp = Vec<(PathBuf, FileStamp)>;

pub(crate) fn stamp(support_dir: &Path, sources: Option<&[DictionarySource]>) -> Stamp {
    let stamped = |path: PathBuf| {
        let stamp = file_stamp(&path);
        (path, stamp)
    };
    dictionary_sources(support_dir, sources)
        .into_iter()
        .flat_map(|source| match source {
            // Its place counts; its own stamp tells when it changes.
            DictionarySource::UserCustom => vec![(support_dir.join(USER_CUSTOM_FILE), None)],
            DictionarySource::File(path) => vec![stamped(path)],
            // The text one counts too: it is read when the binary one is not.
            DictionarySource::Converted { binary, text } => vec![stamped(binary), stamped(text)],
        })
        .chain(std::iter::once(stamped(support_dir.join(MODEL_FILE))))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use kanaemi_config::DICTIONARY_DIR;
    use kanaemi_core::{Converter, Effect};
    use kanaemi_engine::convert_text;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kanaemi-runtime-dictionaries-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(DICTIONARY_DIR)).unwrap();
        dir
    }

    fn registered(reading: &str, surface: &str) -> Effect {
        Effect::Registered {
            reading: reading.to_owned(),
            okurigana: None,
            surface: surface.to_owned(),
        }
    }

    fn surfaces(engine: &Engine, reading: &str) -> Vec<String> {
        engine
            .convert(reading, None)
            .into_iter()
            .map(|c| c.surface)
            .collect()
    }

    #[test]
    fn the_user_custom_dictionary_comes_first_then_files_by_name() {
        let dir = temp_dir("order");
        fs::write(dir.join(USER_CUSTOM_FILE), "きしゃ\t貴社\n").unwrap();
        fs::write(dir.join(DICTIONARY_DIR).join("20-b.tsv"), "きしゃ\t記者\n").unwrap();
        fs::write(dir.join(DICTIONARY_DIR).join("10-a.tsv"), "きしゃ\t汽車\n").unwrap();
        fs::write(dir.join(DICTIONARY_DIR).join("notes.txt"), "きしゃ\t帰社\n").unwrap();
        assert_eq!(
            surfaces(&open_engine(&dir, None), "きしゃ"),
            ["貴社", "汽車", "記者"]
        );
    }

    #[test]
    fn a_ranking_model_beside_the_settings_ranks_the_candidates() {
        let dir = temp_dir("model");
        fs::write(
            dir.join(DICTIONARY_DIR).join("a.tsv"),
            "きしゃ\t記者\nきしゃ\t汽車\n",
        )
        .unwrap();
        assert_eq!(
            surfaces(&open_engine(&dir, None), "きしゃ"),
            ["汽車", "記者"]
        );
        fs::write(dir.join(MODEL_FILE), b"broken").unwrap();
        assert_eq!(
            surfaces(&open_engine(&dir, None), "きしゃ"),
            ["汽車", "記者"],
            "a broken model ranks by the rules"
        );
    }

    #[test]
    fn the_stamp_changes_when_the_model_changes() {
        let dir = temp_dir("model-stamp");
        let before = stamp(&dir, None);
        fs::write(dir.join(MODEL_FILE), b"").unwrap();
        assert_ne!(stamp(&dir, None), before);
    }

    #[test]
    fn a_binary_dictionary_stands_for_the_text_one_it_was_converted_from() {
        let dir = temp_dir("binary");
        let (binary, _) = convert_text("きしゃ\t記者\n");
        fs::write(dir.join(DICTIONARY_DIR).join("b.kdic"), binary).unwrap();
        fs::write(dir.join(DICTIONARY_DIR).join("a.tsv"), "きしゃ\t汽車\n").unwrap();
        assert_eq!(
            surfaces(&open_engine(&dir, None), "きしゃ"),
            ["汽車", "記者"]
        );
        fs::write(dir.join(DICTIONARY_DIR).join("b.tsv"), "きしゃ\t帰社\n").unwrap();
        assert_eq!(
            surfaces(&open_engine(&dir, None), "きしゃ"),
            ["汽車", "記者"],
            "b.kdic stands for b.tsv"
        );
    }

    #[test]
    fn a_binary_dictionary_that_cannot_be_opened_falls_back_to_its_text_one() {
        let dir = temp_dir("fallback");
        let (binary, _) = convert_text("きしゃ\t記者\n");
        // It starts as a binary dictionary does, but ends too soon to open.
        fs::write(dir.join(DICTIONARY_DIR).join("b.kdic"), &binary[..16]).unwrap();
        fs::write(dir.join(DICTIONARY_DIR).join("b.tsv"), "きしゃ\t記者\n").unwrap();
        assert_eq!(surfaces(&open_engine(&dir, None), "きしゃ"), ["記者"]);
    }

    #[test]
    fn a_list_from_the_settings_decides_the_dictionaries_and_their_order() {
        let dir = temp_dir("listed");
        fs::write(dir.join(USER_CUSTOM_FILE), "きしゃ\t貴社\n").unwrap();
        fs::write(dir.join("a.tsv"), "きしゃ\t汽車\n").unwrap();
        fs::write(
            dir.join(DICTIONARY_DIR).join("ignored.tsv"),
            "きしゃ\t記者\n",
        )
        .unwrap();
        let sources = [
            DictionarySource::File(dir.join("a.tsv")),
            DictionarySource::UserCustom,
        ];
        assert_eq!(
            surfaces(&open_engine(&dir, Some(&sources)), "きしゃ"),
            ["汽車", "貴社"]
        );
    }

    #[test]
    fn nothing_on_disk_still_gives_an_engine_that_registers() {
        let dir = temp_dir("empty");
        fs::remove_dir_all(dir.join(DICTIONARY_DIR)).unwrap();
        let mut engine = open_engine(&dir, None);
        assert_eq!(surfaces(&engine, "きしゃ"), Vec::<String>::new());
        engine.learn(&registered("きしゃ", "記者"));
        assert!(engine.take_write_errors().is_empty());
        assert_eq!(
            fs::read_to_string(dir.join(USER_CUSTOM_FILE)).unwrap(),
            "きしゃ\t記者\n"
        );
    }

    #[test]
    fn the_stamp_changes_when_a_dictionary_file_is_added_or_rewritten() {
        let dir = temp_dir("stamp");
        let first = stamp(&dir, None);
        fs::write(dir.join(DICTIONARY_DIR).join("a.tsv"), "きしゃ\t汽車\n").unwrap();
        let added = stamp(&dir, None);
        assert_ne!(first, added);
        assert_eq!(stamp(&dir, None), added, "nothing changed");
        let file = fs::File::options()
            .write(true)
            .open(dir.join(DICTIONARY_DIR).join("a.tsv"))
            .unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH).unwrap();
        assert_ne!(stamp(&dir, None), added, "rewritten");
    }

    /// A clock coarser than two writes leaves a file replaced with as many
    /// bytes at the same time, as when a dictionary is converted twice.
    #[cfg(unix)]
    #[test]
    fn the_stamps_change_when_a_file_is_replaced_within_the_same_tick() {
        let dir = temp_dir("stamp-tick");
        let dictionary = dir.join(DICTIONARY_DIR).join("a.tsv");
        let user = dir.join(USER_CUSTOM_FILE);
        replace_file(&dictionary, "きしゃ\t汽車\n").unwrap();
        replace_file(&user, "きしゃ\t汽車\n").unwrap();
        let (before, user_before) = (stamp(&dir, None), user_stamp(&dir));
        for path in [&dictionary, &user] {
            let modified = fs::metadata(path).unwrap().modified().unwrap();
            replace_file(path, "きしゃ\t記者\n").unwrap();
            let file = fs::File::options().write(true).open(path).unwrap();
            file.set_modified(modified).unwrap();
        }
        assert_ne!(stamp(&dir, None), before);
        assert_ne!(user_stamp(&dir), user_before);
    }

    #[test]
    fn the_stamp_changes_when_the_text_behind_a_binary_dictionary_changes() {
        let dir = temp_dir("stamp-converted");
        let (binary, _) = convert_text("きしゃ\t記者\n");
        fs::write(dir.join(DICTIONARY_DIR).join("b.kdic"), binary).unwrap();
        fs::write(dir.join(DICTIONARY_DIR).join("b.tsv"), "きしゃ\t記者\n").unwrap();
        let before = stamp(&dir, None);
        fs::File::options()
            .write(true)
            .open(dir.join(DICTIONARY_DIR).join("b.tsv"))
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH)
            .unwrap();
        assert_ne!(stamp(&dir, None), before);
    }

    #[test]
    fn the_user_stamp_changes_on_every_append() {
        let dir = temp_dir("user-stamp");
        assert_eq!(user_stamp(&dir), None);
        let mut engine = open_engine(&dir, None);
        engine.learn(&registered("きしゃ", "記者"));
        let first = user_stamp(&dir);
        assert!(first.is_some());
        engine.learn(&registered("きしゃ", "汽車"));
        assert_ne!(user_stamp(&dir), first, "even within the same instant");
    }

    #[test]
    fn a_hand_edit_reads_back_into_the_engine() {
        let dir = temp_dir("user-edit");
        let mut engine = open_engine(&dir, None);
        fs::write(dir.join(USER_CUSTOM_FILE), "きしゃ\t記者\n").unwrap();
        engine.replace_user(read_user(&dir));
        assert_eq!(surfaces(&engine, "きしゃ"), ["記者"]);
    }

    #[test]
    fn the_stamp_changes_when_the_user_custom_dictionary_moves() {
        let dir = temp_dir("stamp-order");
        let a = DictionarySource::File(dir.join(DICTIONARY_DIR).join("a.tsv"));
        let first = [DictionarySource::UserCustom, a.clone()];
        let last = [a, DictionarySource::UserCustom];
        assert_ne!(stamp(&dir, Some(&first)), stamp(&dir, Some(&last)));
    }

    #[test]
    fn the_stamp_follows_the_listed_files() {
        let dir = temp_dir("stamp-listed");
        let sources = [DictionarySource::File(
            dir.join(DICTIONARY_DIR).join("a.tsv"),
        )];
        let missing = stamp(&dir, Some(&sources));
        fs::write(dir.join(DICTIONARY_DIR).join("a.tsv"), "").unwrap();
        assert_ne!(stamp(&dir, Some(&sources)), missing);
        let unlisted = stamp(&dir, Some(&sources));
        fs::write(dir.join(DICTIONARY_DIR).join("b.tsv"), "").unwrap();
        assert_eq!(
            stamp(&dir, Some(&sources)),
            unlisted,
            "an unlisted file does not count"
        );
    }
}
