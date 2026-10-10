//! What a user wants a key to do, across every scene it works in: the
//! bindings of each scene, read and written together, so that a key is
//! chosen once rather than once per scene.

use kanaemi_config::{ApplicationTables, UNBOUND, bindings_table, format_action};
use kanaemi_core::{Action, Bindings, Form};

use crate::remaps::{self, Edit};

/// One thing a key can do, and the scenes it is bound in to do it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intent {
    pub title: &'static str,
    pub note: &'static str,
    pub action: Action,
    pub scenes: &'static [&'static str],
}

const COMPOSING: &[&str] = &["reading", "candidates", "registration"];
const CONVERTING: &[&str] = &["reading", "candidates"];
const EDITING: &[&str] = &["reading", "registration"];
const COMPLETION: &str = "completion";
const COMPLETING: &[&str] = &["reading", COMPLETION, "candidates"];

/// The intents, by the group they are shown in.
pub const GROUPS: &[(&str, &[Intent])] = &[
    (
        "読みと変換",
        &[
            Intent {
                title: "読みを始める",
                note: "読みの途中では送り仮名を始め、候補を選んでいるときは確定して次の読みを始めます",
                action: Action::Begin,
                scenes: &["kana", "reading", "candidates", "registration"],
            },
            Intent {
                title: "変換する・次の候補",
                note: "",
                action: Action::Next,
                scenes: CONVERTING,
            },
            Intent {
                title: "前の候補",
                note: "読みの途中では、変換して最後の候補を選びます",
                action: Action::Previous,
                scenes: CONVERTING,
            },
            Intent {
                title: "読みを補完する",
                note: "読みを、それで始まるより長い読みに置き換えます。続けて押すと次の読みにします。候補を選んでいるときは、読みに戻して補完を続けます",
                action: Action::Complete,
                scenes: COMPLETING,
            },
            Intent {
                title: "読みを逆向きに補完する",
                note: "",
                action: Action::CompletePrevious,
                scenes: COMPLETING,
            },
            Intent {
                title: "確定する",
                note: "",
                action: Action::Commit,
                scenes: COMPOSING,
            },
            Intent {
                title: "取り消す",
                note: "1 つ前の段階に戻ります",
                action: Action::Cancel,
                scenes: COMPOSING,
            },
            Intent {
                title: "打ったかなを読みに戻す",
                note: "読みを始めずに打ったかなを読みに戻して、変換できるようにします。戻した読みでもう一度押すと、最初のかなを確定します",
                action: Action::RereadKana,
                scenes: &["kana", "reading"],
            },
            Intent {
                title: "直前の確定を取り消す",
                note: "何も打っていないときに押すと、直前に確定した候補を選び直せるように戻します",
                action: Action::UndoCommit,
                scenes: &["kana"],
            },
        ],
    ),
    (
        "文字の編集",
        &[
            Intent {
                title: "前の 1 文字を消す",
                note: "",
                action: Action::Backspace,
                scenes: COMPOSING,
            },
            Intent {
                title: "後ろの 1 文字を消す",
                note: "",
                action: Action::Delete,
                scenes: EDITING,
            },
            Intent {
                title: "カーソルを左へ",
                note: "",
                action: Action::Left,
                scenes: EDITING,
            },
            Intent {
                title: "カーソルを右へ",
                note: "",
                action: Action::Right,
                scenes: EDITING,
            },
            Intent {
                title: "カーソルを先頭へ",
                note: "",
                action: Action::Home,
                scenes: EDITING,
            },
            Intent {
                title: "カーソルを末尾へ",
                note: "",
                action: Action::End,
                scenes: EDITING,
            },
        ],
    ),
    (
        "モードの切り替え",
        &[
            Intent {
                title: "ABC モードにする",
                note: "打っている途中のものは確定してから切り替えます",
                action: Action::Abc,
                scenes: &["kana", "reading", "candidates", "registration"],
            },
            Intent {
                title: "かなモードにする",
                note: "",
                action: Action::Kana,
                scenes: &["abc", "registration"],
            },
        ],
    ),
    (
        "候補と登録",
        &[
            Intent {
                title: "ひらがなで確定",
                note: "",
                action: Action::CommitForm(Form::Hiragana),
                scenes: CONVERTING,
            },
            Intent {
                title: "カタカナで確定",
                note: "",
                action: Action::CommitForm(Form::Katakana),
                scenes: CONVERTING,
            },
            Intent {
                title: "半角カタカナで確定",
                note: "",
                action: Action::CommitForm(Form::HalfKatakana),
                scenes: CONVERTING,
            },
            Intent {
                title: "全角英数で確定",
                note: "",
                action: Action::CommitForm(Form::FullAlphanumeric),
                scenes: CONVERTING,
            },
            Intent {
                title: "半角英数で確定",
                note: "",
                action: Action::CommitForm(Form::Alphanumeric),
                scenes: CONVERTING,
            },
            Intent {
                title: "語を登録する",
                note: "",
                action: Action::Register,
                scenes: CONVERTING,
            },
            Intent {
                title: "この候補を以後出さない",
                note: "",
                action: Action::Forget,
                scenes: &["candidates"],
            },
        ],
    ),
];

/// The keys an intent has, as the settings file writes them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IntentKeys {
    /// Bound to it in every one of its scenes.
    pub everywhere: Vec<String>,
    /// Bound to it in some of its scenes only, with those scenes.
    pub somewhere: Vec<(String, Vec<&'static str>)>,
}

/// A line to write: its table, its key, and its value, or `None` to take the
/// line out so the default stands.
pub type Change = (&'static str, String, Option<String>);

fn bound(bindings: &Bindings, scene: &str, key: &str) -> Option<String> {
    bindings_table(bindings, scene)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

impl Intent {
    fn value(&self) -> String {
        format_action(self.action)
    }

    pub fn keys(&self, bindings: &Bindings) -> IntentKeys {
        let value = self.value();
        let per_scene: Vec<Vec<String>> = self
            .scenes
            .iter()
            .map(|scene| {
                bindings_table(bindings, scene)
                    .into_iter()
                    .filter(|(_, v)| *v == value)
                    .map(|(k, _)| k)
                    .collect()
            })
            .collect();
        let mut keys = IntentKeys::default();
        for key in per_scene.iter().flatten() {
            if keys.everywhere.contains(key) || keys.somewhere.iter().any(|(k, _)| k == key) {
                continue;
            }
            let scenes: Vec<&'static str> = self
                .scenes
                .iter()
                .zip(&per_scene)
                .filter(|(_, keys)| keys.contains(key))
                .map(|(scene, _)| *scene)
                .collect();
            if scenes.len() == self.scenes.len() {
                keys.everywhere.push(key.clone());
            } else {
                keys.somewhere.push((key.clone(), scenes));
            }
        }
        keys
    }

    /// Binds `key` to it in every one of its scenes.
    pub fn adding(&self, key: &str, current: &Bindings, shipped: &Bindings) -> Vec<Change> {
        let value = self.value();
        let mut changes: Vec<Change> = self
            .scenes
            .iter()
            .map(|scene| {
                let written = (bound(shipped, scene, key).as_deref() != Some(value.as_str()))
                    .then(|| value.clone());
                (*scene, key.to_owned(), written)
            })
            .collect();
        // A key the completion list binds to something else would hide the
        // reading's binding while the list is shown.
        if self.reads_through_completion()
            && bound(current, COMPLETION, key).is_some_and(|there| there != value)
        {
            let written = (bound(shipped, COMPLETION, key).as_deref() != Some(value.as_str()))
                .then_some(value);
            changes.push((COMPLETION, key.to_owned(), written));
        }
        changes
    }

    /// Whether it works in a reading but has no binding of its own while a
    /// completion is listed, where a key bound nowhere does what it does in
    /// the reading.
    fn reads_through_completion(&self) -> bool {
        self.scenes.contains(&"reading") && !self.scenes.contains(&COMPLETION)
    }

    /// Its scenes, and the completion list where adding it may write it too.
    fn written_scenes(&self) -> impl Iterator<Item = &'static str> {
        let through = self.reads_through_completion().then_some(COMPLETION);
        self.scenes.iter().copied().chain(through)
    }

    /// Takes `key` off it wherever it is bound to it.
    pub fn removing(&self, key: &str, current: &Bindings, shipped: &Bindings) -> Vec<Change> {
        let value = self.value();
        self.written_scenes()
            .filter(|scene| bound(current, scene, key).as_deref() == Some(value.as_str()))
            .map(|scene| {
                let written = (bound(shipped, scene, key).as_deref() == Some(value.as_str()))
                    .then(|| UNBOUND.to_owned());
                (scene, key.to_owned(), written)
            })
            .collect()
    }

    /// Puts `new` in place of `old`, in the scenes `old` is bound in.
    pub fn replacing(
        &self,
        old: &str,
        new: &str,
        current: &Bindings,
        shipped: &Bindings,
    ) -> Vec<Change> {
        if old == new {
            return Vec::new();
        }
        let mut changes = self.removing(old, current, shipped);
        let mut scenes: Vec<&'static str> = changes.iter().map(|(scene, _, _)| *scene).collect();
        if scenes.contains(&"reading") && self.reads_through_completion() {
            scenes.push(COMPLETION);
        }
        changes.extend(
            self.adding(new, current, shipped)
                .into_iter()
                .filter(|(scene, _, _)| scenes.contains(scene)),
        );
        changes
    }

    /// Puts back the default for every key bound to it, now or by default.
    pub fn resetting(&self, current: &Bindings, shipped: &Bindings) -> Vec<Change> {
        let value = self.value();
        let mut changes = Vec::new();
        for scene in self.written_scenes() {
            let (now, then) = (
                bindings_table(current, scene),
                bindings_table(shipped, scene),
            );
            let mut keys: Vec<&String> = Vec::new();
            for (k, v) in now.iter().chain(&then) {
                if *v == value && !keys.contains(&k) {
                    keys.push(k);
                }
            }
            for key in keys {
                if bound(current, scene, key) != bound(shipped, scene, key) {
                    changes.push((scene, key.clone(), None));
                }
            }
        }
        changes
    }

    /// Whether its keys are not Kanaemi's own.
    pub fn changed(&self, current: &Bindings, shipped: &Bindings) -> bool {
        !self.resetting(current, shipped).is_empty()
    }
}

/// Whether keys are sent as other keys while nothing is being typed, as
/// Kanaemi does by default.
pub fn sending(current: &Bindings) -> bool {
    !current.application.is_empty()
}

/// Turns sending keys as other keys on, as Kanaemi does it on each OS, or
/// off on every OS.
pub fn send(on: bool, tables: &ApplicationTables) -> Vec<Edit> {
    if on {
        remaps::all_default(tables)
    } else {
        remaps::all_off(tables)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use kanaemi_config::Settings;
    use kanaemi_core::Gesture;

    use super::*;
    use crate::remaps::tests::edited;

    fn intents() -> impl Iterator<Item = &'static Intent> {
        GROUPS.iter().flat_map(|(_, intents)| intents.iter())
    }

    fn begin() -> Intent {
        *intents().find(|i| i.action == Action::Begin).unwrap()
    }

    /// The bindings a settings file with `changes` written reads as.
    fn written(changes: &[Change]) -> Bindings {
        let mut text = String::new();
        for scene in [
            "kana",
            "abc",
            "reading",
            "completion",
            "candidates",
            "registration",
            "application",
        ] {
            text.push_str(&format!("[keys.{scene}]\n"));
            for (s, key, value) in changes {
                if *s == scene
                    && let Some(value) = value
                {
                    text.push_str(&format!("{key:?} = {value:?}\n"));
                }
            }
        }
        let (settings, problems) = Settings::load(&text, Path::new("/nonexistent"));
        assert_eq!(problems, Vec::new(), "{text}");
        settings.config.bindings
    }

    #[test]
    fn every_intent_works_in_each_of_its_scenes() {
        for intent in intents() {
            for scene in intent.scenes {
                assert!(
                    kanaemi_config::actions(scene).contains(&intent.action),
                    "{} in {scene}",
                    intent.title
                );
            }
        }
    }

    #[test]
    fn taking_text_back_is_shown_with_its_shipped_keys() {
        let shipped = Bindings::default();
        let keys = |action| {
            intents()
                .find(|i| i.action == action)
                .map(|i| i.keys(&shipped))
        };
        assert_eq!(
            keys(Action::RereadKana).map(|k| k.everywhere),
            Some(vec!["ctrl+;".to_owned()])
        );
        assert_eq!(
            keys(Action::UndoCommit).map(|k| k.everywhere),
            Some(vec!["shift+backspace".to_owned()])
        );
    }

    #[test]
    fn a_key_bound_in_some_scenes_only_says_where() {
        let shipped = Bindings::default();
        let next = *intents().find(|i| i.action == Action::Next).unwrap();
        assert_eq!(
            next.keys(&shipped).somewhere,
            [("down".to_owned(), vec!["candidates"])]
        );
        let mut changes = begin().adding("space#hold", &shipped, &shipped);
        changes.retain(|(scene, _, _)| *scene == "reading");
        let keys = begin().keys(&written(&changes));
        assert_eq!(keys.everywhere, [";"]);
        assert_eq!(keys.somewhere, [("space#hold".to_owned(), vec!["reading"])]);
    }

    #[test]
    fn a_key_added_in_the_reading_works_while_a_completion_is_listed_too() {
        let shipped = Bindings::default();
        let commit = *intents().find(|i| i.action == Action::Commit).unwrap();
        let changes = commit.adding("1", &shipped, &shipped);
        assert!(
            changes.contains(&("completion", "1".to_owned(), Some("@commit".to_owned()))),
            "1 picks a listed reading by default: {changes:?}"
        );
        let changes = commit.adding("ctrl+o", &shipped, &shipped);
        assert!(
            changes.iter().all(|(scene, _, _)| *scene != "completion"),
            "bound nowhere there, the reading's binding does: {changes:?}"
        );
        let bindings = written(&commit.adding("1", &shipped, &shipped));
        let changes = commit.removing("1", &bindings, &shipped);
        assert!(
            changes.contains(&("completion", "1".to_owned(), None)),
            "taken off there too, back to picking: {changes:?}"
        );
    }

    #[test]
    fn a_key_moved_to_the_reading_from_the_completion_list_works_there_too() {
        let shipped = Bindings::default();
        let complete = *intents().find(|i| i.action == Action::Complete).unwrap();
        let commit = *intents().find(|i| i.action == Action::Commit).unwrap();
        let bindings = written(&complete.adding("ctrl+o", &shipped, &shipped));
        let changes = commit.adding("ctrl+o", &bindings, &shipped);
        assert!(
            changes.contains(&(
                "completion",
                "ctrl+o".to_owned(),
                Some("@commit".to_owned())
            )),
            "{changes:?}"
        );
    }

    #[test]
    fn a_key_put_in_place_in_the_reading_works_while_a_completion_is_listed_too() {
        let shipped = Bindings::default();
        let commit = *intents().find(|i| i.action == Action::Commit).unwrap();
        let changes = commit.replacing("enter", "1", &shipped, &shipped);
        assert!(
            changes.contains(&("completion", "1".to_owned(), Some("@commit".to_owned()))),
            "{changes:?}"
        );
    }

    #[test]
    fn resetting_takes_back_what_adding_wrote_for_a_completion_listed() {
        let shipped = Bindings::default();
        let commit = *intents().find(|i| i.action == Action::Commit).unwrap();
        let only_there = written(&[("completion", "1".to_owned(), Some("@commit".to_owned()))]);
        assert!(commit.changed(&only_there, &shipped));
        let bindings = written(&commit.adding("1", &shipped, &shipped));
        let changes = commit.resetting(&bindings, &shipped);
        assert!(
            changes.contains(&("completion", "1".to_owned(), None)),
            "{changes:?}"
        );
    }

    #[test]
    fn a_key_taken_off_goes_only_from_where_it_is() {
        let shipped = Bindings::default();
        let next = *intents().find(|i| i.action == Action::Next).unwrap();
        let changes = next.removing("down", &shipped, &shipped);
        assert_eq!(
            changes,
            [("candidates", "down".to_owned(), Some(UNBOUND.to_owned()))]
        );
    }

    #[test]
    fn semicolon_is_swapped_for_space_held_in_every_scene_at_once() {
        let shipped = Bindings::default();
        let mut changes = begin().removing(";", &shipped, &shipped);
        changes.extend(begin().adding("space#hold", &shipped, &shipped));
        let bindings = written(&changes);
        assert_eq!(begin().keys(&bindings).everywhere, ["space#hold"]);
        let held = |scene: &[kanaemi_core::Binding]| {
            scene.iter().any(|b| {
                b.from.key == kanaemi_core::Key::Space
                    && b.gesture == Gesture::Hold
                    && b.to == Action::Begin
            })
        };
        assert!(held(&bindings.kana) && held(&bindings.reading));
        assert!(held(&bindings.candidates) && held(&bindings.registration));
    }

    #[test]
    fn a_default_key_taken_off_is_written_unbound_and_an_added_one_is_taken_out() {
        let shipped = Bindings::default();
        assert!(
            begin()
                .removing(";", &shipped, &shipped)
                .iter()
                .all(|(_, _, value)| value.as_deref() == Some(UNBOUND))
        );
        let added = written(&begin().adding("q", &shipped, &shipped));
        assert!(
            begin()
                .removing("q", &added, &shipped)
                .iter()
                .all(|(_, _, value)| value.is_none())
        );
    }

    #[test]
    fn a_key_changed_takes_the_place_of_the_old_one_where_it_was() {
        let shipped = Bindings::default();
        let bindings = written(&begin().replacing(";", "space#hold", &shipped, &shipped));
        assert_eq!(begin().keys(&bindings).everywhere, ["space#hold"]);

        let next = *intents().find(|i| i.action == Action::Next).unwrap();
        let bindings = written(&next.replacing("down", "f4", &shipped, &shipped));
        let keys = next.keys(&bindings);
        assert_eq!(keys.somewhere, [("f4".to_owned(), vec!["candidates"])]);
        assert!(!keys.everywhere.contains(&"down".to_owned()));
    }

    #[test]
    fn a_key_changed_to_itself_writes_nothing() {
        let shipped = Bindings::default();
        assert!(begin().replacing(";", ";", &shipped, &shipped).is_empty());
    }

    #[test]
    fn adding_a_default_key_back_writes_no_line() {
        let shipped = Bindings::default();
        assert!(
            begin()
                .adding(";", &shipped, &shipped)
                .iter()
                .all(|(_, _, value)| value.is_none())
        );
    }

    #[test]
    fn resetting_puts_kanaemis_own_keys_back() {
        let shipped = Bindings::default();
        let mut changes = begin().removing(";", &shipped, &shipped);
        changes.extend(begin().adding("space#hold", &shipped, &shipped));
        let bindings = written(&changes);
        assert!(begin().changed(&bindings, &shipped));
        let reset = begin().resetting(&bindings, &shipped);
        assert!(reset.iter().all(|(_, _, value)| value.is_none()));
        assert_eq!(reset.len(), 8, "; and space#hold in each of four scenes");
        assert!(!begin().changed(&shipped, &shipped));
    }

    fn loaded(text: &str) -> Settings {
        let (settings, problems) = Settings::load(text, Path::new("/nonexistent"));
        assert!(problems.is_empty(), "{problems:?}\n{text}");
        settings
    }

    #[test]
    fn every_os_sends_keys_as_other_keys_by_default_so_the_switch_starts_on() {
        for os in kanaemi_core::Os::ALL {
            let shipped = Bindings {
                application: kanaemi_core::default_remaps(Some(os)),
                ..Bindings::default()
            };

            assert!(sending(&shipped), "{os:?}");
        }
    }

    #[test]
    fn sending_keys_as_other_keys_is_turned_off_and_on_on_every_os() {
        let text = "[keys.application.linux]\n\"ctrl+k\" = \"end\"\n\
                    [keys.application.windows]\n\"ctrl+h\" = \"@none\"\n";
        assert!(sending(&loaded(text).config.bindings));

        let off = edited(text, &send(false, &loaded(text).application_tables));

        let tables = loaded(&off).application_tables;
        assert!(!sending(&loaded(&off).config.bindings));
        for row in remaps::rows(&tables) {
            assert!(row.sent.iter().all(|(_, to)| to.is_none()), "{row:?}");
        }

        let on = edited(&off, &send(true, &tables));

        assert_eq!(loaded(&on).application_tables, ApplicationTables::default());
        assert!(sending(&loaded(&on).config.bindings));
        assert!(send(true, &loaded(&on).application_tables).is_empty());
    }
}
