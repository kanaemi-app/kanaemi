//! One input field's core, kept in step with the profile every field shares.

use std::sync::atomic::{AtomicU64, Ordering};

use kanaemi_core::{Core, Effect, Event, Output};

use crate::Profile;
use crate::profile::SharedEngine;

/// The core of one field the IME serves.
pub struct Field {
    /// Tells this field from the others the profile follows the focus of.
    id: u64,
    core: Core<SharedEngine>,
    /// The core's engine, to try a key without the user's functions.
    engine: SharedEngine,
    /// The settings the core was built from.
    generation: u64,
    private: bool,
}

impl Field {
    pub fn new(profile: &Profile) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            core: Core::new(profile.converter(), profile.config().clone()),
            engine: profile.converter(),
            generation: profile.generation(),
            private: false,
        }
    }

    /// Marks the field as one that asks not to be recorded, such as one in a
    /// browser's private window. What is typed and committed there is
    /// neither counted as a pick nor kept as what the field holds, and is
    /// left out of the effects; a word registered or forgotten there still
    /// changes the user's dictionary, as the user asked for it.
    pub fn set_private(&mut self, private: bool) {
        self.private = private;
    }

    /// Feeds `event` to the core and teaches the profile's engine what it
    /// did, writing the picks not yet written when the focus goes. When the
    /// focus comes in, what changed in the settings folder is
    /// read first, and a core built from older settings is built again. The
    /// mode the field was in before still counts, so a rebuilt core shows
    /// the switch to the mode it starts in as the old one would have.
    pub fn handle(&mut self, profile: &mut Profile, event: Event) -> Output {
        self.run(profile, event, !self.private)
    }

    /// Ends the focus as [`Event::FocusOut`] does, but drops what is being
    /// typed instead of committing it, and learns nothing of it. For a
    /// platform that moves the IME to the next field before it tells the
    /// last one's focus went, where a commit would land in the wrong field.
    pub fn drop_focus(&mut self, profile: &mut Profile) -> Output {
        let mut output = self.run(profile, Event::FocusOut, false);
        output.commit = None;
        output
    }

    /// Starts the field over with a new core, for a platform that caught a
    /// panic in it: the state the panic left may be broken, and would panic
    /// again on every key. The field stays the same one to the profile and
    /// stays private if it was. Returns what to show now: nothing typed, no
    /// candidates, and the key passed on to the application, as the event
    /// that panicked is left out.
    pub fn restart(&mut self, profile: &Profile) -> Output {
        *self = Self {
            id: self.id,
            private: self.private,
            ..Self::new(profile)
        };
        Output {
            consumed: false,
            erase: None,
            commit: None,
            preedit: String::new(),
            cursor: 0,
            candidates: None,
            mode: self.core.mode(),
            indicator: None,
            send: None,
            effects: Vec::new(),
        }
    }

    fn run(&mut self, profile: &mut Profile, event: Event, learn_typed: bool) -> Output {
        let before = self.core.mode();
        if let Event::FocusIn { .. } = event {
            profile.reload_if_changed();
            if self.generation != profile.generation() {
                *self = Self {
                    id: self.id,
                    private: self.private,
                    ..Self::new(profile)
                };
            }
        }
        let mut output = self.core.handle(event);
        profile.log_functions();
        // A mode another program set is shown there, not by the indicator.
        let from_outside = matches!(event, Event::SetMode(_));
        if output.mode != before && profile.config().mode_indicator && !from_outside {
            output.indicator = Some(output.mode);
        }
        if !learn_typed {
            output.effects.retain(|effect| {
                !matches!(
                    effect,
                    Effect::Committed { .. }
                        | Effect::Withdrawn { .. }
                        | Effect::Typed(_)
                        | Effect::Erased(_)
                )
            });
        }
        profile.learn(&output.effects);
        profile.follow_focus(self.id, event, output.mode);
        // The focus may not come back to any field before the IME stops.
        if let Event::FocusOut = event {
            profile.save_selections();
        }
        output
    }

    /// What the core would answer to `event`, tried on a copy: neither the
    /// field nor the profile changes. A platform that asks whether a key
    /// will be used before it sends the key answers from this.
    pub fn preview(&self, event: Event) -> Output {
        self.engine
            .without_functions(|| self.core.clone().handle(event))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::SystemTime;

    use kanaemi_config::{
        DICTIONARY_DIR, FILE_NAME, FUNCTIONS_DIR, SELECTIONS_FILE, USER_CUSTOM_FILE,
    };
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

    #[test]
    fn a_key_tried_beforehand_leaves_the_field_as_it_was() {
        let dir = temp_dir("preview");
        let mut profile = Profile::open(&dir);
        let mut field = Field::new(&profile);
        field.handle(&mut profile, FOCUS_IN);
        field.handle(&mut profile, press(Key::Kana));
        field.handle(&mut profile, press(Key::Char('k')));
        let tried = field.preview(press(Key::Char('a')));
        assert_eq!(tried.commit.as_deref(), Some("か"));
        assert_eq!(field.handle(&mut profile, press(Key::Char('a'))), tried);
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

    fn typ(field: &mut Field, profile: &mut Profile, text: &str) -> Output {
        let mut output = None;
        for c in text.chars() {
            output = Some(field.handle(profile, press(Key::Char(c))));
        }
        output.expect("something typed")
    }

    #[test]
    fn trying_a_key_runs_no_function_of_the_user() {
        let dir = temp_dir("preview-functions");
        fs::write(
            dir.join(DICTIONARY_DIR).join("words.tsv"),
            "よみ\t{-:count}\n",
        )
        .unwrap();
        let functions = dir.join(FUNCTIONS_DIR);
        fs::create_dir_all(functions.join("lib")).unwrap();
        fs::write(
            functions.join("lib").join("counter.luau"),
            "return { count = 0 }",
        )
        .unwrap();
        fs::write(
            functions.join("count.luau"),
            "local c = require('./lib/counter')\nreturn function() c.count += 1 return tostring(c.count) end",
        )
        .unwrap();
        let mut profile = Profile::open(&dir);
        let mut field = Field::new(&profile);
        field.handle(&mut profile, FOCUS_IN);
        field.handle(&mut profile, press(Key::Kana));
        typ(&mut field, &mut profile, ";yomi");
        field.preview(press(Key::Space));
        let output = field.handle(&mut profile, press(Key::Space));
        assert_eq!(output.preedit, "»1");
    }

    /// A focused field in kana mode with 貴社 picked for きしゃ, not yet
    /// committed.
    fn picking_kisha(profile: &mut Profile, private: bool) -> Field {
        let mut field = Field::new(profile);
        field.set_private(private);
        field.handle(profile, FOCUS_IN);
        field.handle(profile, press(Key::Kana));
        typ(&mut field, profile, ";kisha");
        field.handle(profile, press(Key::Space));
        let output = field.handle(profile, press(Key::Space));
        assert_eq!(output.preedit, "»貴社");
        field
    }

    fn commit_kisha_three_times(profile: &mut Profile, private: bool) {
        for _ in 0..3 {
            let mut field = picking_kisha(profile, private);
            let output = field.handle(profile, press(Key::Enter));
            assert_eq!(output.commit.as_deref(), Some("貴社"));
            field.handle(profile, Event::FocusOut);
        }
    }

    #[test]
    fn picks_in_a_field_that_is_not_private_are_counted() {
        let dir = picks_dir("not-private");
        commit_kisha_three_times(&mut Profile::open(&dir), false);
        assert_eq!(first(&Profile::open(&dir)), "貴社");
    }

    #[test]
    fn picks_in_a_private_field_are_neither_counted_nor_kept() {
        let dir = picks_dir("private");
        let mut profile = Profile::open(&dir);
        commit_kisha_three_times(&mut profile, true);
        assert_eq!(first(&profile), "記者");
        assert!(!dir.join(SELECTIONS_FILE).exists());
    }

    #[test]
    fn what_is_committed_in_a_private_field_is_left_out_of_the_effects() {
        let dir = picks_dir("private-effects");
        let mut profile = Profile::open(&dir);
        let mut field = picking_kisha(&mut profile, true);
        let output = field.handle(&mut profile, press(Key::Enter));
        assert_eq!(output.commit.as_deref(), Some("貴社"), "still typed");
        assert_eq!(output.effects, []);
    }

    #[test]
    fn what_is_undone_in_a_private_field_is_left_out_of_the_effects() {
        let dir = picks_dir("private-undo");
        let mut profile = Profile::open(&dir);
        let mut field = picking_kisha(&mut profile, true);
        field.handle(&mut profile, press(Key::Enter));
        let undo = Event::Key(KeyEvent {
            key: Key::Backspace,
            mods: Modifiers {
                shift: true,
                ..Modifiers::default()
            },
            kind: KeyKind::Press,
            time_ms: 0,
        });
        let output = field.handle(&mut profile, undo);
        assert_eq!(output.erase.as_deref(), Some("貴社"));
        let output = field.handle(&mut profile, Event::Erased(true));
        assert_eq!(output.preedit, "»貴社", "still undone");
        assert_eq!(output.effects, []);
    }

    #[test]
    fn a_word_registered_in_a_private_field_still_goes_to_the_dictionary() {
        let dir = temp_dir("private-register");
        let mut profile = Profile::open(&dir);
        let mut field = Field::new(&profile);
        field.set_private(true);
        field.handle(&mut profile, FOCUS_IN);
        field.handle(&mut profile, press(Key::Kana));
        typ(&mut field, &mut profile, ";nunu");
        let mut output = field.handle(&mut profile, press(Key::Space));
        while !output.preedit.ends_with(" « ") {
            output = field.handle(&mut profile, press(Key::Space));
        }
        typ(&mut field, &mut profile, "a");
        let output = field.handle(&mut profile, press(Key::Enter));
        assert_eq!(output.commit.as_deref(), Some("あ"));
        assert!(
            fs::read_to_string(dir.join(USER_CUSTOM_FILE))
                .unwrap()
                .contains("ぬぬ\tあ")
        );
    }

    #[test]
    fn a_field_stays_private_when_changed_settings_rebuild_its_core() {
        let dir = picks_dir("private-rebuilt");
        let mut profile = Profile::open(&dir);
        let mut field = Field::new(&profile);
        field.set_private(true);
        change_settings(&dir, "");
        field.handle(&mut profile, FOCUS_IN);
        field.handle(&mut profile, press(Key::Kana));
        let output = typ(&mut field, &mut profile, "a");
        assert_eq!(output.commit.as_deref(), Some("あ"));
        assert_eq!(output.effects, []);
    }

    #[test]
    fn a_restarted_field_shows_nothing_and_starts_over_in_abc() {
        let dir = picks_dir("restart");
        let mut profile = Profile::open(&dir);
        let mut field = picking_kisha(&mut profile, false);
        let output = field.restart(&profile);
        assert_eq!(
            (
                output.consumed,
                output.commit,
                output.preedit.as_str(),
                output.candidates,
                output.mode,
                output.send,
            ),
            (false, None, "", None, Mode::Abc, None)
        );
        let output = typ(&mut field, &mut profile, "a");
        assert_eq!(output.preedit, "", "nothing left of what was typed");
        assert_eq!(output.mode, Mode::Abc);
    }

    #[test]
    fn a_restarted_field_stays_private() {
        let dir = picks_dir("restart-private");
        let mut profile = Profile::open(&dir);
        let mut field = picking_kisha(&mut profile, true);
        field.restart(&profile);
        field.handle(&mut profile, press(Key::Kana));
        let output = typ(&mut field, &mut profile, "a");
        assert_eq!(output.commit.as_deref(), Some("あ"));
        assert_eq!(output.effects, []);
    }

    #[test]
    fn a_focus_that_goes_normally_commits_the_pick_and_keeps_it() {
        let dir = picks_dir("focus-out-commits");
        let mut profile = Profile::open(&dir);
        let mut field = picking_kisha(&mut profile, false);
        let output = field.handle(&mut profile, Event::FocusOut);
        assert_eq!(output.commit.as_deref(), Some("貴社"));
        assert!(dir.join(SELECTIONS_FILE).exists());
    }

    #[test]
    fn a_dropped_focus_drops_what_is_typed_without_learning_it() {
        let dir = picks_dir("focus-dropped");
        let mut profile = Profile::open(&dir);
        let mut field = picking_kisha(&mut profile, false);
        let output = field.drop_focus(&mut profile);
        assert_eq!(
            (output.commit, output.preedit.as_str(), output.candidates),
            (None, "", None)
        );
        assert_eq!(output.effects, []);
        assert!(!dir.join(SELECTIONS_FILE).exists());
    }

    #[test]
    fn a_field_whose_focus_was_dropped_starts_afresh_when_it_comes_back() {
        let dir = picks_dir("focus-dropped-back");
        let mut profile = Profile::open(&dir);
        let mut field = picking_kisha(&mut profile, false);
        field.drop_focus(&mut profile);
        let output = field.handle(&mut profile, FOCUS_IN);
        assert_eq!(
            (output.commit, output.preedit.as_str(), output.mode),
            (None, "", Mode::Abc)
        );
    }
}
