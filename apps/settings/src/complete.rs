//! Completing key names as they are typed: each with what it is called on a
//! keyboard and what it is, found by either name.

use dioxus::prelude::*;

use crate::keys::shown;

/// One completion: the text it puts in the field, and what it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub text: String,
    /// As the settings file writes it.
    pub name: String,
    /// As a keyboard labels it.
    pub label: String,
    pub note: &'static str,
    /// A modifier, which the key it goes with follows.
    pub modifier: bool,
}

/// Other names a key goes by, so a key is found however it is called:
/// Option for Alt, Win or Command for Cmd, and the signs on the keys.
fn aliases(name: &str) -> &'static [&'static str] {
    if name.contains("ctrl") {
        &["control", "⌃"]
    } else if name.contains("cmd") {
        &["command", "win", "windows", "super", "⌘"]
    } else if name.contains("alt") {
        &["option", "opt", "⌥"]
    } else if name.contains("shift") {
        &["⇧"]
    } else {
        &[]
    }
}

const MODIFIERS: [(&str, &str); 4] = [
    ("ctrl", "Ctrl と一緒に押す"),
    ("cmd", "Cmd と一緒に押す"),
    ("alt", "Option・Alt と一緒に押す"),
    ("shift", "Shift と一緒に押す"),
];

fn note(name: &str) -> &'static str {
    match name {
        "left-shift" => "左の Shift キー",
        "right-shift" => "右の Shift キー",
        "left-ctrl" => "左の Ctrl キー",
        "right-ctrl" => "右の Ctrl キー",
        "left-cmd" => "左の Cmd・Win キー",
        "right-cmd" => "右の Cmd・Win キー",
        "left-alt" => "左の Option・Alt キー",
        "right-alt" => "右の Option・Alt キー",
        "eisu" => "JIS キーボードの英数キー",
        "kana" => "JIS キーボードのかなキー",
        "henkan" => "Windows の JIS キーボードの変換キー",
        "muhenkan" => "Windows の JIS キーボードの無変換キー",
        "space" => "スペースキー",
        "enter" => "Enter・Return キー",
        "esc" => "Esc キー",
        "backspace" => "前の文字を消すキー",
        "delete" => "後ろの文字を消すキー",
        "left" => "左の矢印キー",
        "right" => "右の矢印キー",
        "up" => "上の矢印キー",
        "down" => "下の矢印キー",
        "home" => "先頭へ動くキー",
        "end" => "末尾へ動くキー",
        _ if name.starts_with('f') => "ファンクションキー",
        _ => "",
    }
}

/// What `typed` can be completed to. The part after the last `+` is
/// completed, matching a key's name or its label; the modifiers before it
/// stay. Names that start with what is typed come first.
pub fn suggest(typed: &str) -> Vec<Suggestion> {
    let (held, part) = match typed.rfind('+') {
        Some(i) if i + 1 < typed.len() || typed.ends_with("++") => typed.split_at(i + 1),
        Some(i) => typed.split_at(i + 1),
        None => ("", typed),
    };
    let part = part.to_lowercase();
    let matches = |name: &str, label: &str| {
        part.is_empty()
            || name.contains(&part)
            || label.to_lowercase().contains(&part)
            || aliases(name).iter().any(|a| a.starts_with(&part))
    };
    let mut out: Vec<Suggestion> = MODIFIERS
        .iter()
        .filter(|(m, _)| !held.split('+').any(|h| h == *m))
        .filter(|(m, _)| {
            !part.is_empty()
                && (m.starts_with(&part) || aliases(m).iter().any(|a| a.starts_with(&part)))
        })
        .map(|(m, note)| Suggestion {
            text: format!("{held}{m}+"),
            name: format!("{m}+"),
            label: shown(&format!("{m}+…")).trim_end_matches('…').to_owned(),
            note,
            modifier: true,
        })
        .collect();
    let mut keys: Vec<Suggestion> = kanaemi_config::key_names()
        .into_iter()
        .filter(|name| matches(name, &shown(name)))
        .map(|name| Suggestion {
            text: format!("{held}{name}"),
            label: shown(&name),
            note: note(&name),
            name,
            modifier: false,
        })
        .collect();
    keys.sort_by_key(|s| !s.name.starts_with(&part));
    out.extend(keys);
    out
}

/// A field for a key, written as the settings file writes it, that offers
/// key names as they are typed. `onkeydown` gets every key the list does
/// not use.
#[component]
pub fn KeyInput(
    value: String,
    placeholder: String,
    class: String,
    readonly: bool,
    oninput: EventHandler<String>,
    onkeydown: EventHandler<KeyboardEvent>,
    /// Opens the list leftwards from the field's right edge, for a field at
    /// the right of the window.
    #[props(default)]
    from_right: bool,
) -> Element {
    let mut open = use_signal(|| false);
    let mut active = use_signal(|| 0usize);
    // Whether a suggestion was moved to: only then does Enter take one over
    // a key already typed in full (`b`, not `backspace`).
    let mut picked = use_signal(|| false);
    let suggestions = if readonly {
        Vec::new()
    } else {
        suggest(&value)
    };
    let shown_list = open() && !suggestions.is_empty();
    let count = suggestions.len();
    let listed = suggestions.clone();
    let typed = value.clone();
    let mut accept = move |s: &Suggestion| {
        oninput.call(s.text.clone());
        active.set(0);
        // A modifier is followed by its key, so the list stays.
        open.set(s.modifier);
    };
    rsx! {
        div { class: "key-input",
            input {
                class: "{class}",
                value: "{value}",
                placeholder: "{placeholder}",
                readonly,
                autocomplete: "off",
                spellcheck: "false",
                oninput: move |e| {
                    oninput.call(e.value());
                    active.set(0);
                    picked.set(false);
                    open.set(true);
                },
                onfocus: move |_| open.set(true),
                onblur: move |_| open.set(false),
                onkeydown: {
                    move |e: KeyboardEvent| {
                        if shown_list {
                            match e.key() {
                                Key::ArrowDown => {
                                    e.prevent_default();
                                    active.set((active() + 1) % count);
                                    picked.set(true);
                                    return;
                                }
                                Key::ArrowUp => {
                                    e.prevent_default();
                                    active.set((active() + count - 1) % count);
                                    picked.set(true);
                                    return;
                                }
                                Key::Enter | Key::Tab => {
                                    let typed_a_key = kanaemi_config::parse_binding_key(&typed).is_some();
                                    let takes = e.key() == Key::Tab || picked() || !typed_a_key;
                                    if takes
                                        && let Some(s) = listed.get(active())
                                        && s.text != typed
                                    {
                                        e.prevent_default();
                                        accept(s);
                                        return;
                                    }
                                }
                                Key::Escape => {
                                    e.prevent_default();
                                    e.stop_propagation();
                                    open.set(false);
                                    return;
                                }
                                _ => {}
                            }
                        }
                        onkeydown.call(e);
                    }
                },
            }
            if shown_list {
                ul { class: if from_right { "suggestions from-right" } else { "suggestions" },
                    for (i , s) in suggestions.into_iter().enumerate() {
                        li {
                            key: "{s.text}",
                            class: if i == active() { "active" },
                            onmouseenter: move |_| active.set(i),
                            // Before the field loses the focus and closes the list.
                            onmousedown: {
                                let s = s.clone();
                                move |e: MouseEvent| {
                                    e.prevent_default();
                                    accept(&s);
                                }
                            },
                            span { class: "key-badge", "{s.label}" }
                            code { class: "suggestion-name", "{s.name}" }
                            span { class: "suggestion-note", "{s.note}" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(typed: &str) -> Vec<String> {
        suggest(typed).into_iter().map(|s| s.text).collect()
    }

    #[test]
    fn names_that_start_with_what_is_typed_come_first() {
        let found = texts("le");
        assert_eq!(&found[..2], ["left", "left-shift"]);
        assert!(found.contains(&"delete".to_owned()));
    }

    #[test]
    fn a_key_is_found_by_its_japanese_name() {
        assert_eq!(texts("変換"), ["henkan", "muhenkan"]);
        assert_eq!(
            texts("左"),
            ["left-shift", "left-ctrl", "left-cmd", "left-alt"]
        );
        assert_eq!(texts("左 shift"), ["left-shift"]);
    }

    #[test]
    fn modifiers_are_offered_and_kept() {
        assert_eq!(texts("ct")[0], "ctrl+");
        assert!(texts("ctrl+")[0].starts_with("ctrl+"));
        assert!(texts("ctrl+sp").contains(&"ctrl+space".to_owned()));
        assert!(
            !texts("ctrl+c").contains(&"ctrl+ctrl+".to_owned()),
            "a modifier held is not offered again"
        );
    }

    #[test]
    fn modifiers_are_found_by_their_other_names() {
        for (typed, found) in [
            ("option", "alt+"),
            ("opt", "alt+"),
            ("win", "cmd+"),
            ("command", "cmd+"),
            ("⌘", "cmd+"),
            ("control", "ctrl+"),
        ] {
            assert!(texts(typed).contains(&found.to_owned()), "{typed}");
        }
    }

    #[test]
    fn either_side_of_any_modifier_is_offered() {
        let found = texts("option");
        assert!(found.contains(&"left-alt".to_owned()));
        assert!(found.contains(&"right-alt".to_owned()));
        assert!(texts("win").contains(&"left-cmd".to_owned()));
        assert!(texts("ctrl").contains(&"right-ctrl".to_owned()));
    }

    #[test]
    fn how_a_key_is_pressed_is_chosen_beside_it_rather_than_offered() {
        assert!(suggest("").iter().all(|s| !s.text.contains('#')));
    }

    #[test]
    fn every_key_is_offered_before_anything_is_typed() {
        assert_eq!(suggest("").len(), kanaemi_config::key_names().len());
    }
}
