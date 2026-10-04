//! What a user wants a key to do, across every scene it works in: the
//! bindings of each scene, read and written together, so that a key is
//! chosen once rather than once per scene.

use kanaemi_config::{APPLICATION_TABLE, UNBOUND, bindings_table, format_action};
use kanaemi_core::{Action, Bindings, Form};

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
    pub fn adding(&self, key: &str, shipped: &Bindings) -> Vec<Change> {
        let value = self.value();
        self.scenes
            .iter()
            .map(|scene| {
                let written = (bound(shipped, scene, key).as_deref() != Some(value.as_str()))
                    .then(|| value.clone());
                (*scene, key.to_owned(), written)
            })
            .collect()
    }

    /// Takes `key` off it wherever it is bound to it.
    pub fn removing(&self, key: &str, current: &Bindings, shipped: &Bindings) -> Vec<Change> {
        let value = self.value();
        self.scenes
            .iter()
            .filter(|scene| bound(current, scene, key).as_deref() == Some(value.as_str()))
            .map(|scene| {
                let written = (bound(shipped, scene, key).as_deref() == Some(value.as_str()))
                    .then(|| UNBOUND.to_owned());
                (*scene, key.to_owned(), written)
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
        let scenes: Vec<&'static str> = changes.iter().map(|(scene, _, _)| *scene).collect();
        changes.extend(
            self.adding(new, shipped)
                .into_iter()
                .filter(|(scene, _, _)| scenes.contains(scene)),
        );
        changes
    }

    /// Puts back the default for every key bound to it, now or by default.
    pub fn resetting(&self, current: &Bindings, shipped: &Bindings) -> Vec<Change> {
        let value = self.value();
        let mut changes = Vec::new();
        for scene in self.scenes {
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
                    changes.push((*scene, key.clone(), None));
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

/// Turns sending keys as other keys on, as Kanaemi does it, or off.
pub fn send(on: bool, current: &Bindings, shipped: &Bindings) -> Vec<Change> {
    let (now, then) = (
        bindings_table(current, APPLICATION_TABLE),
        bindings_table(shipped, APPLICATION_TABLE),
    );
    let mut keys: Vec<&String> = Vec::new();
    for (k, _) in now.iter().chain(&then) {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    keys.into_iter()
        .filter_map(|key| {
            let default = then.iter().any(|(k, _)| k == key);
            let written = match (on, default) {
                (true, _) | (false, false) => None,
                (false, true) => Some(UNBOUND.to_owned()),
            };
            let current = now.iter().find(|(k, _)| k == key).map(|(_, v)| v);
            let unchanged = match &written {
                None => default && current.is_some() || !default && current.is_none(),
                Some(_) => current.is_none(),
            };
            (!unchanged).then(|| (APPLICATION_TABLE, key.clone(), written))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use kanaemi_config::Settings;
    use kanaemi_core::Gesture;

    use super::*;

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
    fn a_key_bound_in_some_scenes_only_says_where() {
        let shipped = Bindings::default();
        let next = *intents().find(|i| i.action == Action::Next).unwrap();
        assert_eq!(
            next.keys(&shipped).somewhere,
            [("down".to_owned(), vec!["candidates"])]
        );
        let mut changes = begin().adding("space#hold", &shipped);
        changes.retain(|(scene, _, _)| *scene == "reading");
        let keys = begin().keys(&written(&changes));
        assert_eq!(keys.everywhere, [";"]);
        assert_eq!(keys.somewhere, [("space#hold".to_owned(), vec!["reading"])]);
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
        changes.extend(begin().adding("space#hold", &shipped));
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
        let added = written(&begin().adding("q", &shipped));
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
                .adding(";", &shipped)
                .iter()
                .all(|(_, _, value)| value.is_none())
        );
    }

    #[test]
    fn resetting_puts_kanaemis_own_keys_back() {
        let shipped = Bindings::default();
        let mut changes = begin().removing(";", &shipped, &shipped);
        changes.extend(begin().adding("space#hold", &shipped));
        let bindings = written(&changes);
        assert!(begin().changed(&bindings, &shipped));
        let reset = begin().resetting(&bindings, &shipped);
        assert!(reset.iter().all(|(_, _, value)| value.is_none()));
        assert_eq!(reset.len(), 8, "; and space#hold in each of four scenes");
        assert!(!begin().changed(&shipped, &shipped));
    }

    #[test]
    fn sending_keys_as_other_keys_is_turned_off_and_on() {
        let shipped = Bindings::default();
        assert!(sending(&shipped));
        let off = written(&send(false, &shipped, &shipped));
        assert!(!sending(&off));
        assert!(
            send(true, &off, &shipped)
                .iter()
                .all(|(_, _, value)| value.is_none())
        );
        assert!(send(true, &shipped, &shipped).is_empty());
    }
}
