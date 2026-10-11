//! Windows virtual-key codes turned into the core's keys, and back for the
//! keys the core sends to the application.

use kanaemi_core::{Chord, Key, KeyEvent, KeyKind, Modifiers};

pub const VK_BACK: u16 = 0x08;
pub const VK_TAB: u16 = 0x09;
pub const VK_RETURN: u16 = 0x0d;
pub const VK_SHIFT: u16 = 0x10;
pub const VK_CONTROL: u16 = 0x11;
pub const VK_MENU: u16 = 0x12;
pub const VK_CAPITAL: u16 = 0x14;
pub const VK_ESCAPE: u16 = 0x1b;
pub const VK_CONVERT: u16 = 0x1c;
pub const VK_NONCONVERT: u16 = 0x1d;
pub const VK_SPACE: u16 = 0x20;
pub const VK_PRIOR: u16 = 0x21;
pub const VK_NEXT: u16 = 0x22;
pub const VK_END: u16 = 0x23;
pub const VK_HOME: u16 = 0x24;
pub const VK_LEFT: u16 = 0x25;
pub const VK_UP: u16 = 0x26;
pub const VK_RIGHT: u16 = 0x27;
pub const VK_DOWN: u16 = 0x28;
pub const VK_DELETE: u16 = 0x2e;
pub const VK_LWIN: u16 = 0x5b;
pub const VK_RWIN: u16 = 0x5c;
pub const VK_F1: u16 = 0x70;
pub const VK_F12: u16 = 0x7b;
pub const VK_LSHIFT: u16 = 0xa0;
pub const VK_RSHIFT: u16 = 0xa1;
pub const VK_LCONTROL: u16 = 0xa2;
pub const VK_RCONTROL: u16 = 0xa3;
pub const VK_LMENU: u16 = 0xa4;
pub const VK_RMENU: u16 = 0xa5;
/// The JIS keyboard's 英数 key.
pub const VK_DBE_ALPHANUMERIC: u16 = 0xf0;
/// The JIS keyboard's かな key with Shift.
pub const VK_DBE_KATAKANA: u16 = 0xf1;
/// The JIS keyboard's かな key.
pub const VK_DBE_HIRAGANA: u16 = 0xf2;
/// The JIS keyboard's 半角/全角 key, as an IME sees it.
pub const VK_KANJI: u16 = 0x19;
/// The JIS keyboard's 半角/全角 key, as the keyboard gives it: the two codes
/// take turns.
pub const VK_OEM_AUTO: u16 = 0xf3;
pub const VK_OEM_ENLW: u16 = 0xf4;

/// The right Shift's scan code: both Shift keys come as `VK_SHIFT`.
pub const SCAN_RSHIFT: u16 = 0x36;

/// A key as TSF reports it.
#[derive(Clone, Copy, Debug)]
pub struct RawKey {
    pub vk: u16,
    pub scan: u16,
    /// Set for the right-hand Ctrl and Alt, among others.
    pub extended: bool,
    pub down: bool,
    /// Set for a press Windows repeats while the key is held.
    pub repeat: bool,
    pub mods: Modifiers,
    /// What the key types with Shift alone applied, if it types anything.
    pub character: Option<char>,
    pub time_ms: u64,
}

/// Remembers the keys down that are not modifiers, because the modifiers
/// held as a key is let go may make it read as another character.
#[derive(Debug, Default)]
pub struct Keys {
    /// Each key down, by virtual-key code, as its press read.
    typed: Vec<(u16, Key)>,
}

impl Keys {
    /// The core's key for one TSF reports; `None` when it is not a press or
    /// release the core needs. A held modifier's repeated press is left out,
    /// as the core counts each press as a new one.
    pub fn translate(&mut self, raw: &RawKey) -> Option<KeyEvent> {
        let event = |key, mods, kind| KeyEvent {
            key,
            mods,
            kind,
            time_ms: raw.time_ms,
        };
        if let Some(key) = sided(raw) {
            return match (raw.down, raw.repeat) {
                (true, true) => None,
                (true, false) => Some(event(key, raw.mods, KeyKind::Press)),
                (false, _) => Some(event(key, raw.mods, KeyKind::Release)),
            };
        }
        if raw.vk == VK_CAPITAL {
            // Each press is one, as on platforms that tell only of its lock
            // turning; no binding waits on its release.
            return (raw.down && !raw.repeat)
                .then(|| event(Key::CapsLock, raw.mods, KeyKind::Press));
        }
        if !raw.down {
            let key = match self.typed.iter().position(|(vk, _)| *vk == raw.vk) {
                Some(index) => self.typed.swap_remove(index).1,
                None => key(raw),
            };
            return Some(event(key, raw.mods, KeyKind::Release));
        }
        // A key repeating stays the key first pressed, whatever modifier
        // went down since. A new press replaces what a release lost to
        // another application left behind.
        let first = self.typed.iter().position(|(vk, _)| *vk == raw.vk);
        let key = match first {
            Some(index) if raw.repeat => self.typed[index].1,
            _ => {
                let key = key(raw);
                self.typed.retain(|(vk, _)| *vk != raw.vk);
                self.typed.push((raw.vk, key));
                key
            }
        };
        // A symbol typed with Shift is that symbol (`:`), as a binding writes
        // it; only a letter keeps its Shift (`shift+a`).
        let mods = match key {
            Key::Char(c) if !c.is_ascii_alphabetic() => Modifiers {
                shift: false,
                ..raw.mods
            },
            _ => raw.mods,
        };
        let kind = if raw.repeat {
            KeyKind::Repeat
        } else {
            KeyKind::Press
        };
        Some(event(key, mods, kind))
    }
}

fn key(raw: &RawKey) -> Key {
    named(raw.vk)
        .or_else(|| raw.character.filter(|c| !c.is_control()).map(Key::Char))
        .unwrap_or(Key::Other)
}

fn sided(raw: &RawKey) -> Option<Key> {
    Some(match raw.vk {
        VK_SHIFT if raw.scan == SCAN_RSHIFT => Key::ShiftRight,
        VK_SHIFT | VK_LSHIFT => Key::ShiftLeft,
        VK_RSHIFT => Key::ShiftRight,
        VK_CONTROL if raw.extended => Key::CtrlRight,
        VK_CONTROL | VK_LCONTROL => Key::CtrlLeft,
        VK_RCONTROL => Key::CtrlRight,
        VK_MENU if raw.extended => Key::AltRight,
        VK_MENU | VK_LMENU => Key::AltLeft,
        VK_RMENU => Key::AltRight,
        VK_LWIN => Key::CmdLeft,
        VK_RWIN => Key::CmdRight,
        _ => return None,
    })
}

/// The keys the core knows by name rather than by what they type.
const NAMED: [(u16, Key); 22] = [
    (VK_BACK, Key::Backspace),
    (VK_TAB, Key::Tab),
    (VK_RETURN, Key::Enter),
    (VK_ESCAPE, Key::Esc),
    (VK_SPACE, Key::Space),
    (VK_DELETE, Key::Delete),
    (VK_LEFT, Key::Left),
    (VK_RIGHT, Key::Right),
    (VK_UP, Key::Up),
    (VK_DOWN, Key::Down),
    (VK_HOME, Key::Home),
    (VK_END, Key::End),
    (VK_PRIOR, Key::PageUp),
    (VK_NEXT, Key::PageDown),
    (VK_CONVERT, Key::Henkan),
    (VK_NONCONVERT, Key::Muhenkan),
    (VK_DBE_ALPHANUMERIC, Key::Eisu),
    (VK_DBE_HIRAGANA, Key::Kana),
    (VK_DBE_KATAKANA, Key::Kana),
    (VK_KANJI, Key::ZenkakuHankaku),
    (VK_OEM_AUTO, Key::ZenkakuHankaku),
    (VK_OEM_ENLW, Key::ZenkakuHankaku),
];

fn named(vk: u16) -> Option<Key> {
    if (VK_F1..=VK_F12).contains(&vk) {
        return Some(Key::F((vk - VK_F1 + 1) as u8));
    }
    NAMED
        .iter()
        .find(|(code, _)| *code == vk)
        .map(|(_, key)| *key)
}

/// Whether the key is one Windows tells apart by the extended flag: the
/// right-hand Ctrl and Alt, the Windows keys, and the editing keys that
/// share their scan codes with the number pad.
pub fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        VK_RCONTROL
            | VK_RMENU
            | VK_LWIN
            | VK_RWIN
            | VK_LEFT
            | VK_RIGHT
            | VK_UP
            | VK_DOWN
            | VK_HOME
            | VK_END
            | VK_PRIOR
            | VK_NEXT
            | VK_DELETE
    )
}

/// The virtual-key code to send for `chord`'s key, for the keys the core
/// can send.
pub fn key_to_send(chord: Chord) -> Option<u16> {
    NAMED
        .iter()
        .find(|(_, key)| *key == chord.key)
        .map(|(code, _)| *code)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One report read on its own, with no key down before it.
    fn translate(raw: &RawKey) -> Option<KeyEvent> {
        Keys::default().translate(raw)
    }

    fn raw(vk: u16, down: bool) -> RawKey {
        RawKey {
            vk,
            scan: 0,
            extended: false,
            down,
            repeat: false,
            mods: Modifiers::default(),
            character: None,
            time_ms: 0,
        }
    }

    fn typed(vk: u16, c: char, shift: bool) -> RawKey {
        RawKey {
            character: Some(c),
            mods: Modifiers {
                shift,
                ..Default::default()
            },
            ..raw(vk, true)
        }
    }

    #[test]
    fn the_two_shift_keys_are_told_apart_by_their_scan_codes() {
        let right = RawKey {
            scan: SCAN_RSHIFT,
            ..raw(VK_SHIFT, true)
        };
        let left = RawKey {
            scan: 0x2a,
            ..raw(VK_SHIFT, true)
        };
        assert_eq!(translate(&right).unwrap().key, Key::ShiftRight);
        assert_eq!(translate(&left).unwrap().key, Key::ShiftLeft);
    }

    #[test]
    fn a_held_modifier_presses_once_and_releases_once() {
        let down = raw(VK_SHIFT, true);
        let repeat = RawKey {
            repeat: true,
            ..down
        };
        assert_eq!(translate(&down).unwrap().kind, KeyKind::Press);
        assert_eq!(translate(&repeat), None, "the repeat");
        assert_eq!(
            translate(&raw(VK_SHIFT, false)).unwrap().kind,
            KeyKind::Release
        );
    }

    #[test]
    fn a_modifier_let_go_in_another_application_presses_again() {
        let down = raw(VK_SHIFT, true);
        assert_eq!(translate(&down).unwrap().kind, KeyKind::Press);
        // Its release went to another application.
        assert_eq!(translate(&down).unwrap().kind, KeyKind::Press);
    }

    #[test]
    fn the_right_ctrl_and_alt_are_extended_keys() {
        let extended = |vk| RawKey {
            extended: true,
            ..raw(vk, true)
        };
        assert_eq!(
            translate(&extended(VK_CONTROL)).unwrap().key,
            Key::CtrlRight
        );
        assert_eq!(translate(&extended(VK_MENU)).unwrap().key, Key::AltRight);
        assert_eq!(translate(&raw(VK_LWIN, true)).unwrap().key, Key::CmdLeft);
    }

    #[test]
    fn a_key_that_types_is_its_character() {
        let a = translate(&typed(0x41, 'a', false)).unwrap();
        assert_eq!((a.key, a.mods.shift), (Key::Char('a'), false));
        let upper = translate(&typed(0x41, 'A', true)).unwrap();
        assert_eq!((upper.key, upper.mods.shift), (Key::Char('A'), true));
        let colon = translate(&typed(0xba, ':', true)).unwrap();
        assert_eq!((colon.key, colon.mods.shift), (Key::Char(':'), false));
    }

    #[test]
    fn named_keys_are_named() {
        assert_eq!(translate(&raw(VK_SPACE, true)).unwrap().key, Key::Space);
        assert_eq!(translate(&raw(0x74, true)).unwrap().key, Key::F(5));
        assert_eq!(translate(&raw(VK_CONVERT, true)).unwrap().key, Key::Henkan);
        assert_eq!(translate(&raw(0x2d, true)).unwrap().key, Key::Other);
        assert_eq!(translate(&raw(VK_PRIOR, true)).unwrap().key, Key::PageUp);
        assert_eq!(translate(&raw(VK_NEXT, true)).unwrap().key, Key::PageDown);
    }

    #[test]
    fn a_named_key_let_go_is_released() {
        let e = translate(&raw(VK_SPACE, false)).unwrap();
        assert_eq!((e.key, e.kind), (Key::Space, KeyKind::Release));
    }

    fn let_go(raw: RawKey) -> RawKey {
        RawKey { down: false, ..raw }
    }

    #[test]
    fn a_key_let_go_is_the_key_its_press_was() {
        let mut keys = Keys::default();
        keys.translate(&typed(0x41, 'A', true));
        let e = keys.translate(&let_go(typed(0x41, 'a', false))).unwrap();
        assert_eq!((e.key, e.kind), (Key::Char('A'), KeyKind::Release));
        let e = keys.translate(&let_go(typed(0x41, 'a', false))).unwrap();
        assert_eq!(
            e.key,
            Key::Char('a'),
            "a release with no press reads as it is"
        );
    }

    #[test]
    fn a_key_repeating_is_the_key_first_pressed() {
        let mut keys = Keys::default();
        keys.translate(&typed(0x41, 'a', false));
        let repeat = RawKey {
            repeat: true,
            ..typed(0x41, 'A', true)
        };
        assert_eq!(keys.translate(&repeat).unwrap().key, Key::Char('a'));
        let e = keys.translate(&let_go(typed(0x41, 'A', true))).unwrap();
        assert_eq!(e.key, Key::Char('a'));
    }

    #[test]
    fn a_new_press_is_read_afresh_though_its_release_went_elsewhere() {
        let mut keys = Keys::default();
        keys.translate(&typed(0x41, 'A', true));
        let e = keys.translate(&typed(0x41, 'a', false)).unwrap();
        assert_eq!(e.key, Key::Char('a'));
    }

    #[test]
    fn caps_lock_let_go_is_left_out() {
        assert_eq!(translate(&raw(VK_CAPITAL, false)), None);
    }

    #[test]
    fn tab_is_tab_with_or_without_shift() {
        let tab = translate(&raw(VK_TAB, true)).unwrap();
        assert_eq!((tab.key, tab.mods.shift), (Key::Tab, false));
        let back_tab = translate(&RawKey {
            mods: Modifiers {
                shift: true,
                ..Default::default()
            },
            ..raw(VK_TAB, true)
        })
        .unwrap();
        assert_eq!((back_tab.key, back_tab.mods.shift), (Key::Tab, true));
    }

    #[test]
    fn the_jis_keys_are_the_cores_jis_keys() {
        let key = |vk| translate(&raw(vk, true)).unwrap().key;
        assert_eq!(key(VK_NONCONVERT), Key::Muhenkan);
        assert_eq!(key(VK_DBE_ALPHANUMERIC), Key::Eisu);
        assert_eq!(key(VK_DBE_HIRAGANA), Key::Kana);
    }

    #[test]
    fn a_key_held_down_presses_again_unless_it_is_a_modifier() {
        let held = RawKey {
            repeat: true,
            ..typed(0x41, 'a', false)
        };
        let e = translate(&held).unwrap();
        assert_eq!((e.key, e.kind), (Key::Char('a'), KeyKind::Repeat));
    }

    #[test]
    fn caps_lock_is_the_cores_caps_lock_with_the_modifiers_held() {
        let e = translate(&RawKey {
            mods: Modifiers {
                shift: true,
                ..Default::default()
            },
            ..raw(VK_CAPITAL, true)
        })
        .unwrap();
        assert_eq!(
            (e.key, e.kind, e.mods.shift),
            (Key::CapsLock, KeyKind::Press, true)
        );
        let held = RawKey {
            repeat: true,
            ..raw(VK_CAPITAL, true)
        };
        assert_eq!(translate(&held), None);
    }

    #[test]
    fn zenkaku_hankaku_is_one_key_whichever_code_windows_gives_it() {
        for vk in [VK_KANJI, VK_OEM_AUTO, VK_OEM_ENLW] {
            assert_eq!(
                translate(&raw(vk, true)).unwrap().key,
                Key::ZenkakuHankaku,
                "{vk:#x}"
            );
        }
    }

    #[test]
    fn kana_with_shift_is_the_kana_key_with_shift() {
        let e = translate(&RawKey {
            mods: Modifiers {
                shift: true,
                ..Default::default()
            },
            ..raw(VK_DBE_KATAKANA, true)
        })
        .unwrap();
        assert_eq!((e.key, e.mods.shift), (Key::Kana, true));
    }

    #[test]
    fn the_right_hand_modifiers_and_the_arrows_are_sent_as_extended_keys() {
        assert!(is_extended(VK_RCONTROL) && is_extended(VK_LEFT) && is_extended(VK_DELETE));
        assert!(!is_extended(VK_LCONTROL) && !is_extended(VK_BACK) && !is_extended(VK_RETURN));
    }

    #[test]
    fn the_keys_the_core_sends_have_codes() {
        let send = |key| {
            key_to_send(Chord {
                key,
                mods: Modifiers::default(),
            })
        };
        assert_eq!(send(Key::Backspace), Some(VK_BACK));
        assert_eq!(send(Key::Left), Some(VK_LEFT));
        assert_eq!(send(Key::Char('a')), None);
    }
}
