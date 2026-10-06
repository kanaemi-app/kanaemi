use crate::{Key, Modifiers, RomajiTable};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub marks: Marks,
    /// Empty unless the host gives one: which tables to stack is a setting.
    pub romaji: RomajiTable,
    /// What keys do, per scene, and the keys sent to the application as
    /// other keys.
    pub bindings: Bindings,
    /// While something is being typed, an unbound key with Cmd, Ctrl or Option
    /// is ignored, so it cannot end the preedit by accident; with one of these
    /// modifiers it commits what is visible and passes on instead.
    pub pass_while_composing: Modifiers,
    /// How long a key may be down and still be tapped.
    pub tap_timeout_ms: u64,
    /// Whether to show the input mode for a moment when it changes.
    pub mode_indicator: bool,
    /// Whether romaji left unfinished outside a reading is committed as
    /// typed. Otherwise the letters forming nothing are dropped; inside a
    /// reading they always are, so that they do not end up in what is
    /// converted.
    pub keep_unfinished_romaji: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            marks: Marks::default(),
            romaji: RomajiTable::empty(),
            bindings: Bindings::default(),
            pass_while_composing: Modifiers::default(),
            tap_timeout_ms: 300,
            mode_indicator: true,
            keep_unfinished_romaji: true,
        }
    }
}

/// A key with the modifiers held exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    pub mods: Modifiers,
}

/// A form of the reading itself, offered among the candidates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Hiragana,
    Katakana,
    HalfKatakana,
    /// The letters typed for the reading, full-width.
    FullAlphanumeric,
    /// The letters typed for the reading.
    Alphanumeric,
}

impl Form {
    pub(crate) const ALL: [Form; 5] = [
        Self::Hiragana,
        Self::Katakana,
        Self::HalfKatakana,
        Self::FullAlphanumeric,
        Self::Alphanumeric,
    ];
}

/// How many candidates a page shows, each picked by its place on the page.
pub(crate) const PAGE_LEN: usize = 9;

/// What a key does while something is being typed. Keys are bound to these
/// per scene, so that Space, Ctrl+N or any other key can do each of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Convert a reading; the next candidate.
    Next,
    /// Convert a reading from its last candidate; the previous candidate.
    Previous,
    /// Commit the reading as kana, the selected candidate, or the word being
    /// registered.
    Commit,
    /// Select the reading in a form, converting it first.
    Form(Form),
    /// Select the reading in a form and commit it.
    CommitForm(Form),
    /// Drop the reading; go back from candidates to the reading; stop
    /// registering.
    Cancel,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    /// Register a word for the reading straight away.
    Register,
    /// Stop offering the selected candidate.
    Forget,
    /// Commit what is being typed and go to ABC mode.
    Abc,
    /// Go to kana mode.
    Kana,
    /// Start a reading; inside one, mark where the okurigana starts; while
    /// choosing, commit the selected candidate and start the next reading.
    Begin,
    /// Commit the candidate at this place on the shown page, from 0.
    Pick(u8),
    /// Take the candidate last committed back to choosing it again.
    UndoCommit,
}

/// Where a key is pressed, which decides the list of bindings it is looked up in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scene {
    /// A reading being typed, or only unfinished romaji.
    Reading,
    Candidates,
    /// The text to register being typed.
    Registration,
    /// Kana mode with nothing typed.
    Kana,
    /// ABC mode with nothing typed.
    Abc,
}

/// A key, how it is pressed, and what it does while typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub from: Chord,
    pub gesture: Gesture,
    pub to: Action,
}

/// How a key is pressed for its binding to act.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    /// As the key goes down. A modifier key acts so even when another key
    /// follows it.
    Press,
    /// A modifier key pressed with no other modifier, let go before any other
    /// key and within [`Config::tap_timeout_ms`]. A key that is not a modifier
    /// is never tapped.
    Tap,
    /// A key that types a character, Space among them, held while another
    /// character is typed, which the binding acts before. Where it is bound,
    /// the key pressed alone acts as it is let go, and if it is to go on to
    /// the application, it types its character.
    Hold,
}

/// A key sent to the application as another key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Remap {
    pub from: Chord,
    pub to: Chord,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bindings {
    pub reading: Vec<Binding>,
    pub candidates: Vec<Binding>,
    /// While typing the text to register.
    pub registration: Vec<Binding>,
    /// In kana mode with nothing typed. Of the characters only `;` is bound
    /// by default, to begin: some romaji tables use every letter.
    pub kana: Vec<Binding>,
    /// In ABC mode with nothing typed, where every key is the application's
    /// by default.
    pub abc: Vec<Binding>,
    /// Sent to the application as other keys while nothing is being typed, so
    /// the same Emacs keys work in every application and on every OS.
    pub application: Vec<Remap>,
}

impl Bindings {
    /// The bindings of a scene.
    pub(crate) fn get(&self, scene: Scene) -> &[Binding] {
        match scene {
            Scene::Reading => &self.reading,
            Scene::Candidates => &self.candidates,
            Scene::Registration => &self.registration,
            Scene::Kana => &self.kana,
            Scene::Abc => &self.abc,
        }
    }

    /// Whether any scene binds a key to be held, which needs the host to
    /// tell when keys are let go.
    pub fn hold_a_key(&self) -> bool {
        [
            &self.reading,
            &self.candidates,
            &self.registration,
            &self.kana,
            &self.abc,
        ]
        .into_iter()
        .flatten()
        .any(|binding| binding.gesture == Gesture::Hold)
    }

    /// Whether the host is asked to send keys to the application: keys sent
    /// in place of others, or Backspaces that erase a commit undone.
    pub fn sends_keys(&self) -> bool {
        !self.application.is_empty() || self.kana.iter().any(|b| b.to == Action::UndoCommit)
    }
}

/// The keys an IME is expected to have, with the Emacs keys SKK users
/// expect beside them.
impl Default for Bindings {
    fn default() -> Self {
        use Action::*;
        let plain = Modifiers::default();
        let shift = Modifiers {
            shift: true,
            ..plain
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..plain
        };
        let key = |key, mods, to| Binding {
            from: Chord { key, mods },
            gesture: Gesture::Press,
            to,
        };
        let tap = |key, to| Binding {
            from: Chord { key, mods: plain },
            gesture: Gesture::Tap,
            to,
        };
        let begin = key(Key::Char(';'), plain, Begin);
        let deciding = [
            key(Key::Enter, plain, Commit),
            key(Key::Char('j'), ctrl, Commit),
            key(Key::Char('m'), ctrl, Commit),
            key(Key::Esc, plain, Cancel),
            key(Key::Char('g'), ctrl, Cancel),
            key(Key::Backspace, plain, Backspace),
            key(Key::Char('h'), ctrl, Backspace),
        ];
        let choosing = [
            key(Key::Space, plain, Next),
            key(Key::Char('n'), ctrl, Next),
            key(Key::Space, shift, Previous),
            key(Key::Char('p'), ctrl, Previous),
        ];
        let editing = [
            key(Key::Delete, plain, Delete),
            key(Key::Char('d'), ctrl, Delete),
            key(Key::Left, plain, Left),
            key(Key::Char('b'), ctrl, Left),
            key(Key::Right, plain, Right),
            key(Key::Char('f'), ctrl, Right),
            key(Key::Home, plain, Home),
            key(Key::Char('a'), ctrl, Home),
            key(Key::End, plain, End),
            key(Key::Char('e'), ctrl, End),
        ];
        // The function keys of Japanese IMEs on every OS.
        let forms = [
            key(Key::F(6), plain, CommitForm(crate::Form::Hiragana)),
            key(Key::F(7), plain, CommitForm(crate::Form::Katakana)),
            key(Key::F(8), plain, CommitForm(crate::Form::HalfKatakana)),
            key(Key::F(9), plain, CommitForm(crate::Form::FullAlphanumeric)),
            key(Key::F(10), plain, CommitForm(crate::Form::Alphanumeric)),
        ];
        // Shift taps and the JIS keys switch modes: left, 英数 and 無変換 to
        // ABC, right, かな and 変換 to kana. While typing, 変換 and 無変換 do
        // what Windows IMEs make them do there.
        let to_abc = [tap(Key::ShiftLeft, Abc), key(Key::Eisu, plain, Abc)];
        let to_kana = [tap(Key::ShiftRight, Kana), key(Key::Kana, plain, Kana)];
        let off = key(Key::Muhenkan, plain, Abc);
        let on = key(Key::Henkan, plain, Kana);
        let converting = [
            key(Key::Henkan, plain, Next),
            key(Key::Muhenkan, plain, Form(crate::Form::Katakana)),
        ];
        let remap = |c, to| Remap {
            from: Chord {
                key: Key::Char(c),
                mods: ctrl,
            },
            to: Chord {
                key: to,
                mods: plain,
            },
        };
        Self {
            reading: [
                &choosing[..],
                &deciding,
                &forms,
                &editing,
                &to_abc,
                &converting,
                &[begin],
            ]
            .concat(),
            candidates: [
                &choosing[..],
                &[key(Key::Down, plain, Next), key(Key::Up, plain, Previous)],
                &deciding,
                &forms,
                &(0..PAGE_LEN as u8)
                    .map(|n| key(Key::Char(char::from(b'1' + n)), plain, Pick(n)))
                    .collect::<Vec<_>>(),
                // 1 to 9 pick a candidate, so 0 is next to them for "none of these".
                &[
                    key(Key::Char('0'), plain, Register),
                    key(Key::Delete, shift, Forget),
                ],
                &to_abc,
                &converting,
                &[begin],
            ]
            .concat(),
            // In the text to register, ABC mode types letters into it and kana
            // mode comes back to kana.
            registration: [
                &deciding[..],
                &editing,
                &to_abc,
                &[off],
                &to_kana,
                &[on],
                &[begin],
            ]
            .concat(),
            // With nothing to undo, Shift+Backspace passes on and deletes a
            // character in most applications, where other keys bound by
            // other IMEs reload a page (Ctrl+Shift+R) or delete a word
            // (Ctrl+Backspace).
            kana: [
                &to_abc[..],
                &[off],
                &[begin],
                &[key(Key::Backspace, shift, UndoCommit)],
            ]
            .concat(),
            abc: [&to_kana[..], &[on]].concat(),
            application: vec![
                remap('h', Key::Backspace),
                remap('d', Key::Delete),
                remap('b', Key::Left),
                remap('f', Key::Right),
                remap('a', Key::Home),
                remap('e', Key::End),
                remap('n', Key::Down),
                remap('p', Key::Up),
                remap('m', Key::Enter),
            ],
        }
    }
}

/// The strings shown in the preedit to tell its scenes apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marks {
    /// Before a reading.
    pub reading: String,
    /// Before the selected candidate, and before a reading being registered.
    pub candidate: String,
    /// Where the okurigana starts.
    pub okurigana: String,
    /// Between a reading being registered and the text to register.
    pub registration: String,
    /// Where the cursor is inside a reading or the text to register, when it
    /// is not at the end.
    pub cursor: String,
    /// After the preedit while a key bound to be held is not yet known held
    /// or pressed alone, followed by the character waiting on it. Empty, it
    /// is shown as a zero-width space: an application that sees no preedit
    /// takes the kept key for its own.
    pub hold: String,
}

impl Default for Marks {
    fn default() -> Self {
        Self {
            reading: "›".to_owned(),
            candidate: "»".to_owned(),
            okurigana: "*".to_owned(),
            registration: " « ".to_owned(),
            cursor: "|".to_owned(),
            hold: String::new(),
        }
    }
}
