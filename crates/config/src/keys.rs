//! How the settings file names keys, actions and the bindings tables.

use kanaemi_core::{Action, Binding, Bindings, Chord, Form, Gesture, Key, Modifiers, Os};

/// Takes a binding out.
pub const UNBOUND: &str = "@none";

/// The bindings table that remaps keys sent to the application, beside the
/// table of each scene.
pub const APPLICATION_TABLE: &str = "application";

/// The table inside the application table that remaps keys on `os` only.
pub fn os_table(os: Os) -> &'static str {
    match os {
        Os::MacOs => "macos",
        Os::Windows => "windows",
        Os::Linux => "linux",
    }
}

/// How many candidates `@select-N` can pick, each by its place on a page.
const PICKABLE: u8 = 9;

/// Where a key is pressed, by the table its bindings are written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scene {
    Reading,
    Completion,
    Candidates,
    Registration,
    Kana,
    Abc,
}

impl Scene {
    const ALL: [Scene; 6] = [
        Self::Reading,
        Self::Completion,
        Self::Candidates,
        Self::Registration,
        Self::Kana,
        Self::Abc,
    ];

    /// The name of its table under `[keys]`.
    const fn name(self) -> &'static str {
        match self {
            Self::Reading => "reading",
            Self::Completion => "completion",
            Self::Candidates => "candidates",
            Self::Registration => "registration",
            Self::Kana => "kana",
            Self::Abc => "abc",
        }
    }

    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// What a key can be bound to here.
    pub(crate) fn allows(self, action: Action) -> bool {
        use Action::*;
        match self {
            Self::Reading => !matches!(action, Forget | Pick(_) | UndoCommit),
            Self::Completion => !matches!(action, Forget | UndoCommit | RereadKana),
            Self::Candidates => !matches!(
                action,
                Delete | Left | Right | Home | End | UndoCommit | RereadKana
            ),
            Self::Registration => matches!(
                action,
                Commit
                    | Cancel
                    | Backspace
                    | Delete
                    | Left
                    | Right
                    | Home
                    | End
                    | Abc
                    | Kana
                    | Begin
            ),
            Self::Kana => matches!(action, Abc | Kana | Begin | UndoCommit | RereadKana),
            Self::Abc => matches!(action, Abc | Kana),
        }
    }

    pub(crate) fn bindings(self, bindings: &Bindings) -> &Vec<Binding> {
        match self {
            Self::Reading => &bindings.reading,
            Self::Completion => &bindings.completion,
            Self::Candidates => &bindings.candidates,
            Self::Registration => &bindings.registration,
            Self::Kana => &bindings.kana,
            Self::Abc => &bindings.abc,
        }
    }

    pub(crate) fn bindings_mut(self, bindings: &mut Bindings) -> &mut Vec<Binding> {
        match self {
            Self::Reading => &mut bindings.reading,
            Self::Completion => &mut bindings.completion,
            Self::Candidates => &mut bindings.candidates,
            Self::Registration => &mut bindings.registration,
            Self::Kana => &mut bindings.kana,
            Self::Abc => &mut bindings.abc,
        }
    }
}

/// Every bindings table under `[keys]`: each scene's, then the application's.
pub fn binding_tables() -> Vec<&'static str> {
    Scene::ALL
        .into_iter()
        .map(Scene::name)
        .chain([APPLICATION_TABLE])
        .collect()
}

/// The bindings of `table` as the settings file writes them, as (pressed key,
/// action or key sent). Empty for a table that does not exist.
pub fn bindings_table(bindings: &Bindings, table: &str) -> Vec<(String, String)> {
    match Scene::named(table) {
        Some(scene) => scene
            .bindings(bindings)
            .iter()
            .filter_map(|b| {
                let mut from = format_chord(b.from)?;
                match b.gesture {
                    Gesture::Press => {}
                    Gesture::Tap => from.push_str(TAP),
                    Gesture::Hold => from.push_str(HOLD),
                }
                Some((from, format_action(b.to)))
            })
            .collect(),
        None if table == APPLICATION_TABLE => bindings
            .application
            .iter()
            .filter_map(|r| Some((format_chord(r.from)?, format_chord(r.to)?)))
            .collect(),
        None => Vec::new(),
    }
}

/// What a key can be bound to in the scene table `table`, in the order to
/// list them; none for the application table, which sends keys instead.
pub fn actions(table: &str) -> Vec<Action> {
    let Some(scene) = Scene::named(table) else {
        return Vec::new();
    };
    every_action()
        .into_iter()
        .filter(|a| scene.allows(*a))
        .collect()
}

fn every_action() -> Vec<Action> {
    const FORMS: [Form; 5] = [
        Form::Hiragana,
        Form::Katakana,
        Form::HalfKatakana,
        Form::FullAlphanumeric,
        Form::Alphanumeric,
    ];
    let mut all = vec![Action::Next, Action::Previous, Action::Commit];
    all.extend(FORMS.map(Action::Form));
    all.extend(FORMS.map(Action::CommitForm));
    all.extend([
        Action::Cancel,
        Action::Backspace,
        Action::Delete,
        Action::Left,
        Action::Right,
        Action::Home,
        Action::End,
        Action::Complete,
        Action::CompletePrevious,
        Action::Register,
        Action::Forget,
        Action::Abc,
        Action::Kana,
        Action::Begin,
    ]);
    all.extend((0..PICKABLE).map(Action::Pick));
    all.extend([Action::UndoCommit, Action::RereadKana]);
    all
}

/// An action as the settings file writes it, `@next`.
pub fn format_action(action: Action) -> String {
    let form = |form| match form {
        Form::Hiragana => "hiragana",
        Form::Katakana => "katakana",
        Form::HalfKatakana => "half-katakana",
        Form::FullAlphanumeric => "full-alphanumeric",
        Form::Alphanumeric => "alphanumeric",
    };
    let name = match action {
        Action::Next => "next".to_owned(),
        Action::Previous => "previous".to_owned(),
        Action::Commit => "commit".to_owned(),
        Action::Form(f) => form(f).to_owned(),
        Action::CommitForm(f) => format!("commit-{}", form(f)),
        Action::Cancel => "cancel".to_owned(),
        Action::Backspace => "backspace".to_owned(),
        Action::Delete => "delete".to_owned(),
        Action::Left => "left".to_owned(),
        Action::Right => "right".to_owned(),
        Action::Home => "home".to_owned(),
        Action::End => "end".to_owned(),
        Action::Register => "register".to_owned(),
        Action::Forget => "forget".to_owned(),
        Action::Abc => "abc".to_owned(),
        Action::Kana => "kana".to_owned(),
        Action::Begin => "begin".to_owned(),
        Action::Pick(n) => format!("select-{}", u16::from(n) + 1),
        Action::UndoCommit => "undo-commit".to_owned(),
        Action::Complete => "complete".to_owned(),
        Action::CompletePrevious => "complete-previous".to_owned(),
        Action::RereadKana => "reread-kana".to_owned(),
    };
    format!("@{name}")
}

/// The action `@name` names, as [`format_action`] writes it.
pub fn parse_action(text: impl AsRef<str>) -> Option<Action> {
    let text = text.as_ref();
    every_action()
        .into_iter()
        .find(|a| format_action(*a) == text)
}

/// A key of a scene's bindings table and how it is pressed: a chord as
/// [`parse_chord`] reads it; a modifier key alone with `#tap` after it for
/// its tap (`left-shift#tap`); or a key that types a character with `#hold`
/// after it for holding it (`space#hold`).
pub fn parse_binding_key(text: impl AsRef<str>) -> Option<(Chord, Gesture)> {
    let text = text.as_ref();
    let annotated = |suffix| text.strip_suffix(suffix).filter(|k: &&str| !k.is_empty());
    if let Some(key) = annotated(TAP) {
        let chord = parse_chord(key)?;
        return (is_modifier(chord.key) && chord.mods == Modifiers::default())
            .then_some((chord, Gesture::Tap));
    }
    if let Some(key) = annotated(HOLD) {
        // Pressed alone, a held key acts only once it is let go, when the
        // press can no longer go on to the application; only a character
        // can still be typed in its place.
        let chord = parse_chord(key)?;
        let typing = matches!(chord.key, Key::Char(_) | Key::Space)
            && !(chord.mods.ctrl || chord.mods.cmd || chord.mods.alt);
        return typing.then_some((chord, Gesture::Hold));
    }
    Some((parse_chord(text)?, Gesture::Press))
}

/// Written after a modifier key for its tap.
const TAP: &str = "#tap";
/// Written after a key that types a character for holding it.
const HOLD: &str = "#hold";

/// A key with its modifiers, as `ctrl+h` or `shift+space`.
pub fn parse_chord(text: impl AsRef<str>) -> Option<Chord> {
    let mut mods = Modifiers::default();
    let mut rest = text.as_ref();
    // Modifiers come first; whatever is left is the key, so `+` itself can be one.
    while let Some((modifier, key)) = rest.split_once('+')
        && !key.is_empty()
    {
        match modifier {
            "ctrl" => mods.ctrl = true,
            "cmd" => mods.cmd = true,
            "alt" => mods.alt = true,
            "shift" => mods.shift = true,
            _ => return None,
        }
        rest = key;
    }
    let key = match parse_key(rest)? {
        // Hosts report a shifted letter as the capital it types.
        Key::Char(c) if mods.shift => Key::Char(c.to_ascii_uppercase()),
        key => key,
    };
    Some(Chord { key, mods })
}

/// How a chord is written in the settings file; `None` for a key the file has
/// no name for.
pub fn format_chord(chord: Chord) -> Option<String> {
    let key = match chord.key {
        // A shifted letter is read back as its capital.
        Key::Char(c) if chord.mods.shift => c.to_ascii_lowercase().to_string(),
        key => format_key(key)?,
    };
    let mods = chord.mods;
    let names = [
        (mods.ctrl, "ctrl+"),
        (mods.cmd, "cmd+"),
        (mods.alt, "alt+"),
        (mods.shift, "shift+"),
    ];
    let mut text: String = names
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, n)| *n)
        .collect();
    text.push_str(&key);
    Some(text)
}

/// The keys the settings file writes by name, then the function keys: what
/// can be written after any modifiers besides a single character.
pub fn key_names() -> Vec<String> {
    NAMED_KEYS
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .chain((1..=12).map(|n| format!("f{n}")))
        .collect()
}

/// How a key is written in the settings file.
fn format_key(key: Key) -> Option<String> {
    match key {
        Key::Char(c) => Some(c.to_string()),
        Key::F(n) => Some(format!("f{n}")),
        key => NAMED_KEYS
            .iter()
            .find(|(_, named)| *named == key)
            .map(|(name, _)| (*name).to_owned()),
    }
}

/// The keys the application table may send, as the file writes them.
pub fn sendable_keys() -> Vec<&'static str> {
    NAMED_KEYS
        .iter()
        .filter(|(_, key)| sendable(*key))
        .map(|(name, _)| *name)
        .collect()
}

const NAMED_KEYS: &[(&str, Key)] = &[
    ("space", Key::Space),
    ("tab", Key::Tab),
    ("enter", Key::Enter),
    ("esc", Key::Esc),
    ("backspace", Key::Backspace),
    ("delete", Key::Delete),
    ("left", Key::Left),
    ("right", Key::Right),
    ("up", Key::Up),
    ("down", Key::Down),
    ("home", Key::Home),
    ("end", Key::End),
    ("left-shift", Key::ShiftLeft),
    ("right-shift", Key::ShiftRight),
    ("left-ctrl", Key::CtrlLeft),
    ("right-ctrl", Key::CtrlRight),
    ("left-cmd", Key::CmdLeft),
    ("right-cmd", Key::CmdRight),
    ("left-alt", Key::AltLeft),
    ("right-alt", Key::AltRight),
    ("eisu", Key::Eisu),
    ("kana", Key::Kana),
    ("henkan", Key::Henkan),
    ("muhenkan", Key::Muhenkan),
];

/// Only named keys can be sent: a character's key code depends on the
/// keyboard layout.
pub(crate) fn sendable(key: Key) -> bool {
    matches!(
        key,
        Key::Backspace
            | Key::Delete
            | Key::Enter
            | Key::Esc
            | Key::Space
            | Key::Left
            | Key::Right
            | Key::Up
            | Key::Down
            | Key::Home
            | Key::End
    )
}

/// A modifier key, which a host passes on to the application whatever it is
/// bound to.
pub(crate) fn is_modifier(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::CtrlLeft
            | Key::CtrlRight
            | Key::CmdLeft
            | Key::CmdRight
            | Key::AltLeft
            | Key::AltRight
    )
}

fn parse_key(name: &str) -> Option<Key> {
    if let Some((_, key)) = NAMED_KEYS.iter().find(|(named, _)| *named == name) {
        return Some(*key);
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=12).contains(&n)
    {
        return Some(Key::F(n));
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_control() => Some(Key::Char(c)),
        _ => None,
    }
}
