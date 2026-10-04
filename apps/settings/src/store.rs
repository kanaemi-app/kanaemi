//! The settings file as the app edits it: every change is checked, written
//! straight away, and read back, so what the app shows is what the IME reads.

use std::path::{Path, PathBuf};

use kanaemi_config::{
    DictionarySource, Editor, FILE_NAME, Problem, ProblemKind, Settings, Value, parse_binding_key,
    read_or_create,
};
use kanaemi_engine::replace_file;

use crate::convert::{is_binary, verify};

pub struct Store {
    pub dir: PathBuf,
    /// The settings, or why the file cannot be edited.
    pub state: Result<Loaded, String>,
}

pub struct Loaded {
    editor: Editor,
    pub settings: Settings,
    pub problems: Vec<Problem>,
}

impl Loaded {
    fn new(editor: Editor, dir: &Path) -> Self {
        let (settings, problems) = Settings::load(editor.text(), dir);
        Self {
            editor,
            settings,
            problems,
        }
    }
}

impl Store {
    pub fn open(dir: PathBuf) -> Self {
        let state = read_or_create(&dir)
            .map_err(|error| error.to_string())
            .and_then(|text| Editor::new(&text))
            .map(|editor| Loaded::new(editor, &dir));
        Self { dir, state }
    }

    /// Writes `value` at `path`, or with `None` removes what is written there
    /// so the setting takes its default. A value the IME could not read is
    /// refused with the reason, and nothing is written.
    pub fn change(&mut self, path: &[&str], value: Option<Value>) -> Result<(), String> {
        self.change_many(&[(path.to_vec(), value)])
    }

    /// Makes every change of `changes` in one write, or none of them when
    /// one is refused, so a binding moved to another key is never left
    /// bound to both or neither.
    pub fn change_many(&mut self, changes: &[(Vec<&str>, Option<Value>)]) -> Result<(), String> {
        // The file may have been edited by hand since it was read: edit that.
        let on_disk = read_or_create(&self.dir).map_err(|e| e.to_string())?;
        if self
            .state
            .as_ref()
            .is_ok_and(|l| l.editor.text() != on_disk)
        {
            self.state = Editor::new(&on_disk).map(|editor| Loaded::new(editor, &self.dir));
        }
        let loaded = self.state.as_mut().map_err(|error| error.clone())?;
        let mut editor = loaded.editor.clone();
        for (path, value) in changes {
            edit(&mut editor, path, value.clone());
        }
        let next = Loaded::new(editor, &self.dir);
        // Only a problem in a setting left untouched may stay, as it was
        // before: what is written now must be readable, even where the file
        // already held something unreadable.
        for (path, _) in changes {
            let item = path.join(".");
            if let Some(problem) = next
                .problems
                .iter()
                .find(|p| refuses(&p.kind) && is_within(&p.item, &item))
            {
                return Err(crate::messages::describe(&problem.kind));
            }
        }
        if changes.iter().any(|(path, _)| path == &["dictionaries"]) {
            check_new_binaries(
                loaded.settings.dictionaries.as_deref().unwrap_or_default(),
                next.settings.dictionaries.as_deref().unwrap_or_default(),
            )?;
        }
        replace_file(self.dir.join(FILE_NAME), next.editor.text()).map_err(|e| e.to_string())?;
        *loaded = next;
        Ok(())
    }
}

/// Whether the IME could not use a value with this problem. A romaji table
/// with invalid lines is still used, without those lines.
pub fn refuses(kind: &ProblemKind) -> bool {
    !matches!(kind, ProblemKind::RomajiInvalidLines { .. })
}

/// Whether the item at `problem` is the setting `item` or inside it.
fn is_within(problem: &str, item: &str) -> bool {
    problem
        .strip_prefix(item)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// Writes `value` at `path`, or with `None` removes what is written there.
fn edit(editor: &mut Editor, path: &[&str], value: Option<Value>) {
    // A key bound under another spelling (`shift+ctrl+h` for
    // `ctrl+shift+h`) is the same key: it goes, so only one line binds it.
    if let ["keys", mode, key] = path
        && let Some(chord) = parse_binding_key(key)
    {
        for written in editor.keys(&["keys", mode]) {
            if written != *key && parse_binding_key(&written) == Some(chord) {
                editor.reset(&["keys", mode, &written]);
            }
        }
    }
    match value {
        Some(value) => editor.set(path, value),
        None => editor.reset(path),
    }
}

/// Checks every byte of each binary dictionary that joins the list. The IME
/// only checks that a file is well formed, to open it fast.
fn check_new_binaries(
    before: &[DictionarySource],
    after: &[DictionarySource],
) -> Result<(), String> {
    for source in after.iter().filter(|s| !before.contains(s)) {
        if let DictionarySource::File(path) = source
            && is_binary(path)
        {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            verify(path).map_err(|reason| format!("{name} が壊れています（{reason}）"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use kanaemi_config::{DICTIONARY_DIR, ROMAJI_DIR, TEMPLATE};

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kanaemi-settings-store-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn written(dir: &Path) -> String {
        fs::read_to_string(dir.join(FILE_NAME)).unwrap()
    }

    #[test]
    fn opening_without_a_file_writes_the_template() {
        let dir = temp_dir("open");
        let store = Store::open(dir.clone());
        assert_eq!(store.state.unwrap().settings, Settings::load("", &dir).0);
        assert_eq!(written(&dir), TEMPLATE);
    }

    #[test]
    fn a_change_is_written_and_read_back() {
        let dir = temp_dir("change");
        let mut store = Store::open(dir.clone());
        store
            .change(&["marks", "reading"], Some("▽".into()))
            .unwrap();
        assert_eq!(
            store.state.as_ref().unwrap().settings.config.marks.reading,
            "▽"
        );
        assert!(written(&dir).contains("\nreading = \"▽\"\n"));
        store.change(&["marks", "reading"], None).unwrap();
        assert_eq!(written(&dir), TEMPLATE);
    }

    #[test]
    fn several_changes_are_written_together_or_not_at_all() {
        let dir = temp_dir("many");
        let mut store = Store::open(dir.clone());
        assert!(
            store
                .change_many(&[
                    (vec!["marks", "reading"], Some("▽".into())),
                    (vec!["marks", "candidate"], Some("".into())),
                ])
                .is_err()
        );
        assert_eq!(written(&dir), TEMPLATE);
        store
            .change_many(&[
                (vec!["marks", "reading"], Some("▽".into())),
                (vec!["marks", "candidate"], Some("▼".into())),
            ])
            .unwrap();
        let marks = &store.state.as_ref().unwrap().settings.config.marks;
        assert_eq!(
            (marks.reading.as_str(), marks.candidate.as_str()),
            ("▽", "▼")
        );
    }

    #[test]
    fn a_value_the_ime_cannot_read_is_refused() {
        let dir = temp_dir("refuse");
        let mut store = Store::open(dir.clone());
        assert!(
            store
                .change(&["marks", "reading"], Some("".into()))
                .is_err()
        );
        assert_eq!(written(&dir), TEMPLATE);
        assert_eq!(
            store.state.unwrap().settings.config.marks,
            kanaemi_core::Marks::default()
        );
    }

    fn names(names: &[&str]) -> Option<Value> {
        Some(Value::Array(names.iter().copied().collect()))
    }

    #[test]
    fn a_binary_dictionary_is_checked_whole_when_it_joins_the_list() {
        let dir = temp_dir("binary");
        let folder = dir.join(DICTIONARY_DIR);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("a.tsv"), "きしゃ\t記者\n").unwrap();
        crate::convert::convert(&folder, "a.tsv").unwrap();
        let mut bytes = fs::read(folder.join("a.kdic")).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(folder.join("broken.kdic"), bytes).unwrap();
        let mut store = Store::open(dir.clone());
        assert!(
            store
                .change(&["dictionaries"], names(&["broken.kdic"]))
                .is_err()
        );
        store.change(&["dictionaries"], names(&["a.kdic"])).unwrap();
        assert!(written(&dir).contains("\"a.kdic\""));
    }

    #[test]
    fn a_dictionary_already_listed_is_not_checked_again() {
        let dir = temp_dir("listed");
        let folder = dir.join(DICTIONARY_DIR);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("a.tsv"), "きしゃ\t記者\n").unwrap();
        crate::convert::convert(&folder, "a.tsv").unwrap();
        let mut store = Store::open(dir.clone());
        store
            .change(&["dictionaries"], names(&["a.kdic", "custom"]))
            .unwrap();
        let mut bytes = fs::read(folder.join("a.kdic")).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(folder.join("a.kdic"), bytes).unwrap();
        store
            .change(&["dictionaries"], names(&["custom", "a.kdic"]))
            .unwrap();
    }

    #[test]
    fn a_problem_elsewhere_does_not_block_a_change() {
        let dir = temp_dir("elsewhere");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(FILE_NAME), "[marks]\ncursor = \"\"\n").unwrap();
        let mut store = Store::open(dir.clone());
        store
            .change(&["mode_indicator"], Some(false.into()))
            .unwrap();
        assert_eq!(store.state.unwrap().problems.len(), 1);
    }

    #[test]
    fn a_value_the_ime_cannot_read_is_refused_even_where_one_was_written_by_hand() {
        let dir = temp_dir("already-broken");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(FILE_NAME), "[marks]\nreading = \"\\t\"\n").unwrap();
        let mut store = Store::open(dir.clone());

        let changed = store.change(&["marks", "reading"], Some("".into()));

        assert!(changed.is_err());
        assert_eq!(written(&dir), "[marks]\nreading = \"\\t\"\n");
    }

    #[test]
    fn a_setting_whose_name_starts_like_a_broken_one_is_not_blamed_for_it() {
        let dir = temp_dir("prefix");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(FILE_NAME),
            "[keys.reading]\n\"ctrl+xx\" = \"@commit\"\n",
        )
        .unwrap();
        let mut store = Store::open(dir.clone());

        let changed = store.change(&["keys", "reading", "ctrl+x"], Some("@cancel".into()));

        assert_eq!(changed, Ok(()));
    }

    #[test]
    fn a_change_keeps_what_was_written_to_the_file_meanwhile() {
        let dir = temp_dir("meanwhile");
        let mut store = Store::open(dir.clone());
        fs::write(dir.join(FILE_NAME), "mode_indicator = false\n").unwrap();
        store
            .change(&["marks", "cursor"], Some("_".into()))
            .unwrap();
        let written = written(&dir);
        assert!(written.contains("mode_indicator = false"), "{written}");
        assert!(written.contains("cursor = \"_\""), "{written}");
    }

    #[test]
    fn a_key_written_another_way_is_replaced_not_doubled() {
        let dir = temp_dir("alias");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(FILE_NAME),
            "[keys.reading]\n\"shift+ctrl+h\" = \"@cancel\"\n",
        )
        .unwrap();
        let mut store = Store::open(dir.clone());
        store
            .change(&["keys", "reading", "ctrl+shift+h"], Some("@commit".into()))
            .unwrap();
        let written = written(&dir);
        assert!(!written.contains("shift+ctrl+h"), "{written}");
        assert!(
            written.contains("\"ctrl+shift+h\" = \"@commit\""),
            "{written}"
        );
    }

    #[test]
    fn a_romaji_table_with_invalid_lines_can_be_chosen_as_the_ime_uses_its_other_lines() {
        let dir = temp_dir("romaji-invalid");
        fs::create_dir_all(dir.join(ROMAJI_DIR)).unwrap();
        fs::write(
            dir.join(ROMAJI_DIR).join("mine.tsv"),
            "qq\tくく\nbad line\n",
        )
        .unwrap();
        let mut store = Store::open(dir.clone());

        let changed = store.change(&["romaji", "tables"], names(&["hepburn", "mine.tsv"]));

        assert_eq!(changed, Ok(()));
        assert!(written(&dir).contains("tables = [\"hepburn\", \"mine.tsv\"]"));
    }

    #[test]
    fn a_dictionary_outside_its_folder_is_refused() {
        let dir = temp_dir("outside");
        let mut store = Store::open(dir.clone());

        let changed = store.change(&["dictionaries"], names(&["../custom.tsv"]));

        assert!(changed.is_err());
        assert_eq!(written(&dir), TEMPLATE);
    }

    #[test]
    fn a_file_that_is_not_toml_is_not_edited() {
        let dir = temp_dir("broken");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(FILE_NAME), "[marks\n").unwrap();
        let mut store = Store::open(dir.clone());
        assert!(store.state.is_err());
        assert!(
            store
                .change(&["mode_indicator"], Some(false.into()))
                .is_err()
        );
        assert_eq!(written(&dir), "[marks\n");
    }
}
