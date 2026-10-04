//! Everything the IME reads from the settings folder: the settings, and one
//! engine every field shares, so a word registered in one application is
//! offered in the next. Only one field has the focus at a time, so the
//! commit history the engine keeps is still that field's.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use kanaemi_config::DictionarySource;
use kanaemi_core::{Candidate, Config, Converter, Effect, Event, Mode};
use kanaemi_engine::{Engine, Selections};

use crate::control::{ControlRequest, Server, Wake};
use crate::dictionaries::{self, FileStamp, Stamp};
use crate::settings;

/// The settings and the engine every field shares, read from one settings
/// folder.
pub struct Profile {
    dir: PathBuf,
    config: Config,
    /// As the settings list them; `None` reads the defaults.
    dictionaries: Option<Vec<DictionarySource>>,
    /// Counts reloads of the settings, so a field knows when to rebuild its core.
    generation: u64,
    engine: SharedEngine,
    settings_modified: Option<SystemTime>,
    dictionary_stamp: Stamp,
    user_stamp: FileStamp,
    selections_stamp: FileStamp,
    /// Whether the record of picks on disk was read, or is missing; until
    /// then it is not written over.
    selections_read: bool,
    /// Picks a save failed to write, written at the next focus move.
    unsaved: Option<Selections>,
    /// The port the settings name for other programs' requests.
    control_port: Option<u16>,
    /// How to wake the thread fields are served on; requests are taken only
    /// once the platform gives one.
    wake: Option<Wake>,
    control: Option<Server>,
    /// The field with the focus, by its id, and its mode.
    focus: Option<(u64, Mode)>,
}

impl Profile {
    /// Reads the settings in `dir` and opens the dictionaries they list,
    /// creating the folder and a commented settings file when missing.
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        if let Err(error) = std::fs::create_dir_all(&dir) {
            tracing::warn!(path = %dir.display(), %error, "settings folder not created");
        }
        // Stamped before reading, so a change while reading is seen next time.
        let (settings, settings_modified) = settings::read_stamped(&dir);
        let dictionary_stamp = dictionaries::stamp(&dir, settings.dictionaries.as_deref());
        let user_stamp = dictionaries::user_stamp(&dir);
        let engine = dictionaries::open_engine(&dir, settings.dictionaries.as_deref());
        let mut profile = Self {
            config: settings.config,
            dictionaries: settings.dictionaries,
            generation: 0,
            engine: SharedEngine(Rc::new(RefCell::new(engine))),
            settings_modified,
            dictionary_stamp,
            user_stamp,
            selections_stamp: None,
            selections_read: false,
            unsaved: None,
            control_port: settings.control_port,
            wake: None,
            control: None,
            focus: None,
            dir,
        };
        let shared = profile.engine.0.clone();
        profile.read_selections(&mut shared.borrow_mut());
        profile
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// The engine as a field's converter. It follows every reload.
    pub(crate) fn converter(&self) -> SharedEngine {
        self.engine.clone()
    }

    /// Reads again what changed on disk since it was read: the settings,
    /// the dictionaries in use, and the record of picks.
    pub(crate) fn reload_if_changed(&mut self) {
        if settings::modified(&self.dir) != self.settings_modified {
            let (settings, modified) = settings::read_stamped(&self.dir);
            self.settings_modified = modified;
            self.config = settings.config;
            self.dictionaries = settings.dictionaries;
            self.control_port = settings.control_port;
            self.listen_on_port();
            self.generation += 1;
            tracing::info!("settings read again");
        }
        let stamp = dictionaries::stamp(&self.dir, self.dictionaries.as_deref());
        let user_stamp = dictionaries::user_stamp(&self.dir);
        let user_changed = std::mem::replace(&mut self.user_stamp, user_stamp) != user_stamp;
        if stamp != self.dictionary_stamp {
            self.dictionary_stamp = stamp;
            let engine = dictionaries::open_engine(&self.dir, self.dictionaries.as_deref());
            let mut shared = self.engine.0.borrow_mut();
            let previous = std::mem::replace(&mut *shared, engine);
            shared.take_over(previous);
            tracing::info!("dictionaries opened again");
        } else if user_changed {
            let user = dictionaries::read_user(&self.dir);
            self.engine.0.borrow_mut().replace_user(user);
            tracing::info!("user custom dictionary read again");
        }
        if !self.selections_read
            || dictionaries::selections_stamp(&self.dir) != self.selections_stamp
        {
            let shared = self.engine.0.clone();
            self.read_selections(&mut shared.borrow_mut());
            tracing::info!("record of picks read again");
        }
    }

    /// Teaches the engine what an event did. The record of picks is written
    /// when the focus moves, not on every pick.
    pub(crate) fn learn(&mut self, effects: &[Effect]) {
        {
            let mut engine = self.engine.0.borrow_mut();
            for effect in effects {
                engine.learn(effect);
            }
            for error in engine.take_write_errors() {
                tracing::warn!(%error, "user custom dictionary not written");
            }
        }
        if effects.contains(&Effect::FocusMoved) {
            self.save_selections();
        }
    }

    /// Writes the picks not yet written, unless the record on disk could
    /// not be read or changed since it was.
    pub(crate) fn save_selections(&mut self) {
        let shared = self.engine.0.clone();
        let mut engine = shared.borrow_mut();
        if dictionaries::selections_stamp(&self.dir) != self.selections_stamp {
            // Erased or changed elsewhere since it was read: that wins over
            // the picks held here, which would otherwise undo it.
            self.read_selections(&mut engine);
        } else if self.selections_read
            // The engine's record holds every pick, so a newer one replaces
            // what failed to be written, which must then never be written.
            && let Some(selections) = {
                let failed = self.unsaved.take();
                engine.take_selections().or(failed)
            }
        {
            if dictionaries::save_selections(&self.dir, &selections) {
                self.selections_stamp = dictionaries::selections_stamp(&self.dir);
            } else {
                self.unsaved = Some(selections);
            }
        }
    }

    /// Puts the record of picks on disk in place of the one held here, when
    /// it can be read.
    fn read_selections(&mut self, engine: &mut Engine) {
        // Stamped before reading, so a change while reading is seen next time.
        self.selections_stamp = dictionaries::selections_stamp(&self.dir);
        let read = dictionaries::read_selections(&self.dir);
        self.selections_read = read.is_some();
        if let Some(selections) = read {
            engine.replace_selections(selections);
            self.unsaved = None;
        }
    }
}

/// Requests from other programs, on the port the settings name.
impl Profile {
    /// Takes requests from now on, waking the thread fields are served on
    /// with `wake`, which may be called from any thread. The port follows
    /// the settings as they are read again.
    pub fn listen(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.wake = Some(std::sync::Arc::new(wake));
        self.listen_on_port();
    }

    /// The requests that came in since the last call, oldest first. Answer
    /// each with [`Profile::answer`], in order, after putting the field with
    /// the focus in the mode it asks for.
    pub fn take_control_requests(&mut self) -> Vec<ControlRequest> {
        self.control.as_mut().map(Server::take).unwrap_or_default()
    }

    /// Answers `request` with what the field with the focus is in now.
    pub fn answer(&mut self, request: ControlRequest) {
        let watcher = request.answer(self.focus.map(|(_, mode)| mode));
        if let (Some(watcher), Some(control)) = (watcher, self.control.as_mut()) {
            control.watch(watcher);
        }
    }

    /// Keeps track of the field with the focus after it handled `event`,
    /// telling watching programs when its mode changes.
    pub(crate) fn follow_focus(&mut self, field: u64, event: Event, mode: Mode) {
        let ours = self.focus.is_some_and(|(id, _)| id == field);
        match event {
            Event::FocusIn { .. } => self.focus = Some((field, mode)),
            // The focus may already be in another field.
            Event::FocusOut if ours => self.focus = None,
            _ if ours => self.focus = Some((field, mode)),
            _ => {}
        }
        if let Some(control) = &mut self.control {
            control.tell(self.focus.map(|(_, mode)| mode));
        }
    }

    /// Listens on the port the settings name, letting go of any other.
    fn listen_on_port(&mut self) {
        let Some(wake) = self.wake.clone() else {
            return;
        };
        if self.control.as_ref().map(Server::port) == self.control_port {
            return;
        }
        self.control = None;
        let Some(port) = self.control_port else {
            return;
        };
        match Server::listen(port, wake) {
            Ok(server) => {
                tracing::info!(port, "listening for other programs");
                self.control = Some(server);
            }
            Err(error) => tracing::warn!(port, %error, "control port not listened on"),
        }
    }
}

/// The profile's engine, as one field's converter.
#[derive(Clone)]
pub(crate) struct SharedEngine(Rc<RefCell<Engine>>);

impl Converter for SharedEngine {
    fn convert(&self, reading: &str, okurigana: Option<&str>) -> Vec<Candidate> {
        self.0.borrow().convert(reading, okurigana)
    }

    fn registered_text(&self, reading: &str, okurigana: Option<&str>, surface: &str) -> String {
        self.0.borrow().registered_text(reading, okurigana, surface)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use kanaemi_config::{DICTIONARY_DIR, FILE_NAME, SELECTIONS_FILE, USER_CUSTOM_FILE};
    use kanaemi_core::Action;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kanaemi-runtime-profile-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(DICTIONARY_DIR)).unwrap();
        dir
    }

    fn surfaces(profile: &Profile, reading: &str) -> Vec<String> {
        profile
            .converter()
            .convert(reading, None)
            .into_iter()
            .map(|c| c.surface)
            .collect()
    }

    fn commit(reading: &str, surface: &str) -> Effect {
        Effect::Committed {
            reading: reading.to_owned(),
            okurigana: None,
            surface: surface.to_owned(),
        }
    }

    /// Picks `surface` in a field of its own each time.
    fn pick(profile: &mut Profile, surface: &str, times: usize) {
        for _ in 0..times {
            profile.learn(&[commit("きしゃ", surface), Effect::FocusMoved]);
        }
    }

    /// Moves a file's time back, so the next write counts as a change even
    /// within the clock's resolution.
    fn age(path: &Path) {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH)
            .unwrap();
    }

    #[test]
    fn a_registered_word_enters_the_field_as_the_engine_writes_it() {
        let profile = Profile::open(temp_dir("registered-text"));
        let text = profile
            .converter()
            .registered_text("１２こ", None, "{kanji}個");
        assert_eq!(text, "十二個");
    }

    #[test]
    fn opening_writes_the_settings_template_and_folder() {
        let dir = temp_dir("open").join("nested");
        let profile = Profile::open(&dir);
        assert!(dir.join(FILE_NAME).exists());
        assert_eq!(profile.generation(), 0);
    }

    #[test]
    fn the_key_bindings_reach_the_core_config() {
        let dir = temp_dir("bindings");
        let begins = |profile: &Profile| {
            profile
                .config()
                .bindings
                .kana
                .iter()
                .any(|b| b.to == Action::Begin)
        };
        fs::write(dir.join(FILE_NAME), "[keys.kana]\n\";\" = \"@none\"\n").unwrap();
        let mut profile = Profile::open(&dir);
        assert!(!begins(&profile));
        fs::write(dir.join(FILE_NAME), "").unwrap();
        age(&dir.join(FILE_NAME));
        profile.reload_if_changed();
        assert!(begins(&profile));
    }

    #[test]
    fn a_settings_file_written_from_the_template_is_not_read_twice() {
        let dir = temp_dir("template");
        let mut profile = Profile::open(&dir);
        profile.reload_if_changed();
        assert_eq!(profile.generation(), 0);
        fs::remove_file(dir.join(FILE_NAME)).unwrap();
        profile.reload_if_changed();
        profile.reload_if_changed();
        assert_eq!(profile.generation(), 1, "read once after it was removed");
    }

    #[cfg(unix)]
    #[test]
    fn a_settings_file_it_could_not_read_is_read_once_it_can() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("unreadable");
        fs::write(dir.join(FILE_NAME), "mode_indicator = false\n").unwrap();
        let locked = fs::Permissions::from_mode(0o000);
        fs::set_permissions(dir.join(FILE_NAME), locked).unwrap();
        let mut profile = Profile::open(&dir);
        assert!(
            profile.config().mode_indicator,
            "the default, as it was unreadable"
        );
        fs::set_permissions(dir.join(FILE_NAME), fs::Permissions::from_mode(0o600)).unwrap();
        profile.reload_if_changed();
        assert!(!profile.config().mode_indicator);
    }

    #[test]
    fn a_changed_settings_file_is_read_again_and_counted() {
        let dir = temp_dir("settings");
        let mut profile = Profile::open(&dir);
        age(&dir.join(FILE_NAME));
        profile.reload_if_changed();
        assert_eq!(profile.generation(), 1);
    }

    #[test]
    fn a_dictionary_added_meanwhile_is_opened_again() {
        let dir = temp_dir("dictionary");
        let mut profile = Profile::open(&dir);
        let converter = profile.converter();
        fs::write(dir.join(DICTIONARY_DIR).join("a.tsv"), "きしゃ\t汽車\n").unwrap();
        profile.reload_if_changed();
        assert_eq!(surfaces(&profile, "きしゃ"), ["汽車"]);
        assert_eq!(
            converter.convert("きしゃ", None).len(),
            1,
            "a converter taken before follows"
        );
    }

    #[test]
    fn reopening_the_dictionaries_carries_the_commit_history_over() {
        let dir = temp_dir("history");
        fs::write(
            dir.join(DICTIONARY_DIR).join("a.tsv"),
            "きしゃ\t記者\t\t10\nきしゃ\t貴社\t\t20\n",
        )
        .unwrap();
        let mut profile = Profile::open(&dir);
        profile.learn(&[commit("きしゃ", "貴社")]);
        assert_eq!(surfaces(&profile, "きしゃ"), ["貴社", "記者"]);
        fs::write(dir.join(DICTIONARY_DIR).join("b.tsv"), "きしゃ\t汽車\n").unwrap();
        profile.reload_if_changed();
        assert_eq!(surfaces(&profile, "きしゃ")[0], "貴社");
    }

    #[test]
    fn a_hand_edit_of_the_user_custom_dictionary_reads_back() {
        let dir = temp_dir("user");
        let mut profile = Profile::open(&dir);
        fs::write(dir.join(USER_CUSTOM_FILE), "きしゃ\t記者\n").unwrap();
        profile.reload_if_changed();
        assert_eq!(surfaces(&profile, "きしゃ"), ["記者"]);
    }

    #[test]
    fn picks_are_kept_when_the_focus_moves_and_only_for_the_user() {
        let dir = temp_dir("picks");
        fs::write(
            dir.join(DICTIONARY_DIR).join("a.tsv"),
            "きしゃ\t記者\t\t10\nきしゃ\t貴社\t\t20\n",
        )
        .unwrap();
        let mut profile = Profile::open(&dir);
        profile.learn(&[commit("きしゃ", "貴社")]);
        assert!(!dir.join(SELECTIONS_FILE).exists(), "not on every pick");
        pick(&mut profile, "貴社", 3);
        assert!(dir.join(SELECTIONS_FILE).exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join(SELECTIONS_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert_eq!(surfaces(&Profile::open(&dir), "きしゃ"), ["貴社", "記者"]);
    }

    #[test]
    fn an_erased_record_is_not_written_back() {
        let dir = temp_dir("erased");
        fs::write(
            dir.join(DICTIONARY_DIR).join("a.tsv"),
            "きしゃ\t記者\t\t10\nきしゃ\t貴社\t\t20\n",
        )
        .unwrap();
        let mut profile = Profile::open(&dir);
        pick(&mut profile, "貴社", 3);
        fs::remove_file(dir.join(SELECTIONS_FILE)).unwrap();
        // The focus leaves the field before it comes back to another.
        pick(&mut profile, "貴社", 1);
        assert!(!dir.join(SELECTIONS_FILE).exists());
        profile.reload_if_changed();
        assert_eq!(surfaces(&profile, "きしゃ"), ["記者", "貴社"]);
    }

    #[test]
    fn an_erased_record_found_on_focus_is_forgotten() {
        let dir = temp_dir("erased-focus");
        fs::write(
            dir.join(DICTIONARY_DIR).join("a.tsv"),
            "きしゃ\t記者\t\t10\nきしゃ\t貴社\t\t20\n",
        )
        .unwrap();
        let mut profile = Profile::open(&dir);
        pick(&mut profile, "貴社", 3);
        fs::remove_file(dir.join(SELECTIONS_FILE)).unwrap();
        profile.reload_if_changed();
        assert_eq!(surfaces(&profile, "きしゃ"), ["記者", "貴社"]);
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

    const NOT_UTF8: &[u8] = b"\xff\xfe\xfd\n";

    #[test]
    fn a_record_that_cannot_be_read_is_never_written_over() {
        let dir = picks_dir("unreadable-record");
        fs::write(dir.join(SELECTIONS_FILE), NOT_UTF8).unwrap();
        let mut profile = Profile::open(&dir);
        pick(&mut profile, "貴社", 3);
        profile.reload_if_changed();
        pick(&mut profile, "貴社", 1);
        assert_eq!(fs::read(dir.join(SELECTIONS_FILE)).unwrap(), NOT_UTF8);
    }

    #[test]
    fn a_record_that_could_not_be_read_is_read_once_it_can_be() {
        let dir = picks_dir("record-readable-again");
        pick(&mut Profile::open(&dir), "貴社", 3);
        let record = fs::read(dir.join(SELECTIONS_FILE)).unwrap();
        fs::write(dir.join(SELECTIONS_FILE), NOT_UTF8).unwrap();
        let mut profile = Profile::open(&dir);
        assert_eq!(surfaces(&profile, "きしゃ"), ["記者", "貴社"]);
        fs::write(dir.join(SELECTIONS_FILE), &record).unwrap();
        profile.reload_if_changed();
        assert_eq!(surfaces(&profile, "きしゃ"), ["貴社", "記者"]);
        pick(&mut profile, "貴社", 1);
        assert_ne!(
            fs::read(dir.join(SELECTIONS_FILE)).unwrap(),
            NOT_UTF8,
            "written again once read"
        );
    }

    #[cfg(unix)]
    #[test]
    fn picks_that_could_not_be_saved_are_saved_at_the_next_focus_move() {
        use std::os::unix::fs::PermissionsExt;
        let dir = picks_dir("save-retried");
        let mut profile = Profile::open(&dir);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).unwrap();
        pick(&mut profile, "貴社", 2);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(!dir.join(SELECTIONS_FILE).exists(), "the folder refused it");
        profile.learn(&[Effect::FocusMoved]);
        assert!(dir.join(SELECTIONS_FILE).exists());
        pick(&mut profile, "貴社", 1);
        assert_eq!(
            surfaces(&Profile::open(&dir), "きしゃ"),
            ["貴社", "記者"],
            "the picks before and after the failure are all kept"
        );
    }

    #[cfg(unix)]
    #[test]
    fn picks_saved_after_a_failure_are_not_written_over_by_the_failed_ones() {
        use std::os::unix::fs::PermissionsExt;
        let dir = picks_dir("save-superseded");
        let mut profile = Profile::open(&dir);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).unwrap();
        pick(&mut profile, "貴社", 2);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        pick(&mut profile, "貴社", 1);
        let saved = fs::read(dir.join(SELECTIONS_FILE)).unwrap();
        profile.learn(&[Effect::FocusMoved]);
        assert_eq!(
            fs::read(dir.join(SELECTIONS_FILE)).unwrap(),
            saved,
            "a focus move without picks keeps the newer record"
        );
        assert_eq!(surfaces(&Profile::open(&dir), "きしゃ"), ["貴社", "記者"]);
    }
}
