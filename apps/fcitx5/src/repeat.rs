//! Fcitx5's own mark of a key repeat.

use kanaemi_core::{KeyEvent, KeyKind};

/// Set in a key's state when Fcitx5 knows the press repeats one held down.
const REPEAT: u32 = 1 << 31;

/// The press read as a repeat when Fcitx5 marked it one. Over XIM a held key
/// repeats as a release and a press, which alone read as a new press.
pub(crate) fn marked(event: KeyEvent, state: u32) -> KeyEvent {
    match event.kind {
        KeyKind::Press if state & REPEAT != 0 => KeyEvent {
            kind: KeyKind::Repeat,
            ..event
        },
        _ => event,
    }
}

#[cfg(test)]
mod tests {
    use kanaemi_core::{Key, Modifiers};

    use super::*;

    fn event(kind: KeyKind) -> KeyEvent {
        KeyEvent {
            key: Key::Delete,
            mods: Modifiers::default(),
            kind,
            time_ms: 0,
        }
    }

    #[test]
    fn a_press_fcitx5_marks_a_repeat_is_one() {
        assert_eq!(marked(event(KeyKind::Press), REPEAT).kind, KeyKind::Repeat);
        assert_eq!(marked(event(KeyKind::Press), 0).kind, KeyKind::Press);
        assert_eq!(
            marked(event(KeyKind::Release), REPEAT).kind,
            KeyKind::Release
        );
    }
}
