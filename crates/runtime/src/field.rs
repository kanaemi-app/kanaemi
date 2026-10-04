//! One input field's core, kept in step with the profile every field shares.

use kanaemi_core::{Core, Event, Output};

use crate::Profile;
use crate::profile::SharedEngine;

/// The core of one field the IME serves.
pub struct Field {
    core: Core<SharedEngine>,
    /// The settings the core was built from.
    generation: u64,
}

impl Field {
    pub fn new(profile: &Profile) -> Self {
        Self {
            core: Core::new(profile.converter(), profile.config().clone()),
            generation: profile.generation(),
        }
    }

    /// Feeds `event` to the core and teaches the profile's engine what it
    /// did, writing the picks not yet written when the focus goes. When the
    /// focus comes in, what changed in the settings folder is
    /// read first, and a core built from older settings is built again. The
    /// mode the field was in before still counts, so a rebuilt core shows
    /// the switch to the mode it starts in as the old one would have.
    pub fn handle(&mut self, profile: &mut Profile, event: Event) -> Output {
        let before = self.core.mode();
        if let Event::FocusIn { .. } = event {
            profile.reload_if_changed();
            if self.generation != profile.generation() {
                *self = Self::new(profile);
            }
        }
        let mut output = self.core.handle(event);
        if output.mode != before && profile.config().mode_indicator {
            output.indicator = Some(output.mode);
        }
        profile.learn(&output.effects);
        // The focus may not come back to any field before the IME stops.
        if let Event::FocusOut = event {
            profile.save_selections();
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::SystemTime;

    use kanaemi_config::{DICTIONARY_DIR, FILE_NAME, SELECTIONS_FILE};
    use kanaemi_core::{Converter, Effect, Key, KeyEvent, KeyKind, Mode, Modifiers};

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kanaemi-runtime-field-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(DICTIONARY_DIR)).unwrap();
        dir
    }

    fn press(key: Key) -> Event {
        Event::Key(KeyEvent {
            key,
            mods: Modifiers::default(),
            kind: KeyKind::Press,
            time_ms: 0,
        })
    }

    const FOCUS_IN: Event = Event::FocusIn { password: false };

    /// Rewrites the settings file so it reads as changed even within the
    /// clock's resolution.
    fn change_settings(dir: &Path, text: &str) {
        let path = dir.join(FILE_NAME);
        fs::write(&path, text).unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH)
            .unwrap();
    }

    /// A field left in kana mode when the focus went elsewhere.
    fn left_in_kana(profile: &mut Profile) -> Field {
        let mut field = Field::new(profile);
        field.handle(profile, FOCUS_IN);
        field.handle(profile, press(Key::Kana));
        field.handle(profile, Event::FocusOut);
        field
    }

    #[test]
    fn returning_to_a_field_left_in_kana_shows_the_switch_to_abc() {
        let dir = temp_dir("kana");
        let mut profile = Profile::open(&dir);
        let mut field = left_in_kana(&mut profile);
        let output = field.handle(&mut profile, FOCUS_IN);
        assert_eq!(
            (output.mode, output.indicator),
            (Mode::Abc, Some(Mode::Abc))
        );
    }

    #[test]
    fn returning_after_the_settings_changed_shows_the_switch_to_abc_all_the_same() {
        let dir = temp_dir("kana-reloaded");
        let mut profile = Profile::open(&dir);
        let mut field = left_in_kana(&mut profile);
        change_settings(&dir, "");
        let output = field.handle(&mut profile, FOCUS_IN);
        assert_eq!(
            (output.mode, output.indicator),
            (Mode::Abc, Some(Mode::Abc))
        );
    }

    #[test]
    fn returning_to_a_field_left_in_abc_shows_nothing_whether_or_not_the_settings_changed() {
        let dir = temp_dir("abc");
        let mut profile = Profile::open(&dir);
        let mut field = Field::new(&profile);
        field.handle(&mut profile, FOCUS_IN);
        field.handle(&mut profile, Event::FocusOut);
        assert_eq!(field.handle(&mut profile, FOCUS_IN).indicator, None);
        field.handle(&mut profile, Event::FocusOut);
        change_settings(&dir, "");
        assert_eq!(field.handle(&mut profile, FOCUS_IN).indicator, None);
    }

    #[test]
    fn settings_that_turn_the_indicator_off_hide_the_switch_on_focus() {
        let dir = temp_dir("indicator-off");
        let mut profile = Profile::open(&dir);
        let mut field = left_in_kana(&mut profile);
        change_settings(&dir, "mode_indicator = false\n");
        assert_eq!(field.handle(&mut profile, FOCUS_IN).indicator, None);
    }

    #[test]
    fn changed_settings_apply_from_the_next_focus() {
        let dir = temp_dir("applied");
        let mut profile = Profile::open(&dir);
        let mut field = Field::new(&profile);
        field.handle(&mut profile, FOCUS_IN);
        change_settings(&dir, "mode_indicator = false\n");
        assert_eq!(
            field.handle(&mut profile, press(Key::Kana)).indicator,
            Some(Mode::Kana),
            "not while the field has the focus"
        );
        field.handle(&mut profile, Event::FocusOut);
        field.handle(&mut profile, FOCUS_IN);
        assert_eq!(field.handle(&mut profile, press(Key::Kana)).indicator, None);
    }

    /// A folder with a dictionary where 記者 comes before 貴社 until 貴社 is
    /// picked again and again.
    fn picks_dir(name: &str) -> PathBuf {
        let dir = temp_dir(name);
        fs::write(
            dir.join(DICTIONARY_DIR).join("a.tsv"),
            "きしゃ\t記者\t\t10\nきしゃ\t貴社\t\t20\n",
        )
        .unwrap();
        dir
    }

    /// 貴社 committed in the field with the focus often enough to come first.
    fn pick_in(profile: &mut Profile) {
        let commit = Effect::Committed {
            reading: "きしゃ".to_owned(),
            okurigana: None,
            surface: "貴社".to_owned(),
        };
        profile.learn(&[commit.clone(), commit.clone(), commit]);
    }

    fn first(profile: &Profile) -> String {
        profile
            .converter()
            .convert("きしゃ", None)
            .remove(0)
            .surface
    }

    #[test]
    fn picks_in_the_last_field_are_kept_when_its_focus_goes() {
        let dir = picks_dir("focus-out");
        let mut profile = Profile::open(&dir);
        let mut field = Field::new(&profile);
        field.handle(&mut profile, FOCUS_IN);
        pick_in(&mut profile);
        field.handle(&mut profile, Event::FocusOut);
        assert_eq!(first(&Profile::open(&dir)), "貴社");
    }

    #[test]
    fn a_record_erased_meanwhile_is_not_written_back_when_the_focus_goes() {
        let dir = picks_dir("focus-out-erased");
        let mut profile = Profile::open(&dir);
        let mut field = Field::new(&profile);
        field.handle(&mut profile, FOCUS_IN);
        pick_in(&mut profile);
        field.handle(&mut profile, Event::FocusOut);
        field.handle(&mut profile, FOCUS_IN);
        fs::remove_file(dir.join(SELECTIONS_FILE)).unwrap();
        pick_in(&mut profile);
        field.handle(&mut profile, Event::FocusOut);
        assert!(!dir.join(SELECTIONS_FILE).exists());
    }

    #[test]
    fn a_record_that_cannot_be_read_is_not_written_over_when_the_focus_goes() {
        let dir = picks_dir("focus-out-unreadable");
        fs::write(dir.join(SELECTIONS_FILE), b"\xff\xfe\n").unwrap();
        let mut profile = Profile::open(&dir);
        let mut field = Field::new(&profile);
        field.handle(&mut profile, FOCUS_IN);
        pick_in(&mut profile);
        field.handle(&mut profile, Event::FocusOut);
        assert_eq!(fs::read(dir.join(SELECTIONS_FILE)).unwrap(), b"\xff\xfe\n");
    }

    #[cfg(unix)]
    #[test]
    fn picks_that_could_not_be_saved_when_the_focus_went_are_saved_next_time() {
        use std::os::unix::fs::PermissionsExt;
        let dir = picks_dir("focus-out-retried");
        let mut profile = Profile::open(&dir);
        let mut field = Field::new(&profile);
        field.handle(&mut profile, FOCUS_IN);
        pick_in(&mut profile);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).unwrap();
        field.handle(&mut profile, Event::FocusOut);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(!dir.join(SELECTIONS_FILE).exists(), "the folder refused it");
        field.handle(&mut profile, FOCUS_IN);
        field.handle(&mut profile, Event::FocusOut);
        assert_eq!(first(&Profile::open(&dir)), "貴社");
    }
}
