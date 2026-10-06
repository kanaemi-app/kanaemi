use crate::{Candidate, Chord};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The input mode the user switches between. A reading and its candidates
/// are typed in kana mode; the text to register can be typed in either.
pub enum Mode {
    Abc,
    Kana,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateView {
    pub items: Vec<Candidate>,
    pub selected: usize,
}

/// What the host learns from an event, in the order it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// A candidate was committed for `reading` (okurigana included), as
    /// converted with `okurigana`, the chunk the core passed to
    /// [`crate::Converter::convert`].
    Committed {
        reading: String,
        okurigana: Option<String>,
        surface: String,
    },
    /// A word was registered. `reading` stops before any okurigana.
    /// `okurigana_head` is the okurigana's first chunk, typed where it was
    /// marked; the okurigana goes on past it only by romaji that chunk left
    /// over (`っ` of `った`).
    Registered {
        reading: String,
        okurigana: Option<String>,
        okurigana_head: Option<String>,
        surface: String,
    },
    /// `surface`, converted from `reading` with `okurigana` as for
    /// [`Effect::Committed`], is not to be offered for `reading` again.
    Forgotten {
        reading: String,
        okurigana: Option<String>,
        surface: String,
    },
    /// Text went into the field, converted or not.
    Typed(String),
    /// The focus moved to another field: what was learned of the last one ends.
    FocusMoved,
}

/// The whole state to show after an event, so the host never has to remember the last one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    /// Whether the key was handled here; `false` means the host passes it to the application.
    pub consumed: bool,
    /// Text to commit before passing the key on.
    pub commit: Option<String>,
    pub preedit: String,
    /// In characters from the start of the preedit.
    pub cursor: usize,
    pub candidates: Option<CandidateView>,
    pub mode: Mode,
    /// The mode to show near the cursor, set only by the event that changed it.
    pub indicator: Option<Mode>,
    /// A key the host sends to the application in place of the one pressed.
    pub send: Option<Chord>,
    pub effects: Vec<Effect>,
}
