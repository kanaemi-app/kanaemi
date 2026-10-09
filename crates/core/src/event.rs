use crate::Mode;

/// A key, as the core tells keys apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A key that types a character, with Shift already applied (`A`, not `a`).
    Char(char),
    Space,
    /// Tab; with Shift, Shift+Tab, however the host reports it.
    Tab,
    Enter,
    Esc,
    Backspace,
    Delete,
    ShiftLeft,
    ShiftRight,
    CtrlLeft,
    CtrlRight,
    /// Command on macOS, the key in its place on other keyboards.
    CmdLeft,
    CmdRight,
    /// Option on macOS, Alt elsewhere.
    AltLeft,
    AltRight,
    /// The JIS keyboard's 英数 key.
    Eisu,
    /// The JIS keyboard's かな key.
    Kana,
    /// The JIS keyboard's 変換 key, on Windows.
    Henkan,
    /// The JIS keyboard's 無変換 key, on Windows.
    Muhenkan,
    /// A function key, numbered from 1.
    F(u8),
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    /// A modifier key going down, on its own.
    Modifier,
    /// Any other key: it ends a modifier tap and passes on. While something
    /// is being typed, with Cmd, Ctrl or Option it is ignored unless
    /// [`crate::Config::pass_while_composing`] lets it through.
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub cmd: bool,
    /// Option on macOS, Alt elsewhere.
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    Press,
    Release,
    /// The OS pressing a key again while it is held. It is a press, but it
    /// does not answer a question that wants the key pressed again. A host
    /// that cannot tell a repeat sends a press.
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub mods: Modifiers,
    pub kind: KeyKind,
    /// Milliseconds on any clock that only moves forward.
    pub time_ms: u64,
}

/// Everything the host tells the core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Key(KeyEvent),
    FocusIn {
        password: bool,
    },
    FocusOut,
    /// The application or the OS asks for the preedit to be committed.
    Flush,
    /// A candidate on the shown page was picked, by its position from 0.
    Select(usize),
    /// Another program puts the field in `Mode`, as the `abc` and `kana`
    /// actions would; already in it, nothing changes. The program shows the
    /// mode itself, so the indicator stays off.
    SetMode(Mode),
    /// Whether the host took [`crate::Output::erase`] out of the field. Until
    /// it tells, what was undone stays committed.
    Erased(bool),
    /// The caret may have moved without a key the core saw, as on a click:
    /// what is before it is no longer known. What is being typed stays.
    CaretMoved,
}
