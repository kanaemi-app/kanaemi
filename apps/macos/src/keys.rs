//! Turns AppKit key events into core key events. Kept free of AppKit types so
//! it builds and tests on any platform.

use kanaemi_core::{Chord, Key, KeyEvent, KeyKind, Modifiers};

// NSEventModifierFlags.
const SHIFT: usize = 1 << 17;
const CONTROL: usize = 1 << 18;
const OPTION: usize = 1 << 19;
const COMMAND: usize = 1 << 20;
const CAPS_LOCK: usize = 1 << 16;
const FUNCTION: usize = 1 << 23;
// Device-dependent modifier bits (IOKit NX_DEVICE*KEYMASK), which tell left
// from right.
const DEVICE_LEFT_CONTROL: usize = 0x01;
const DEVICE_LEFT_SHIFT: usize = 0x02;
const DEVICE_RIGHT_SHIFT: usize = 0x04;
const DEVICE_LEFT_COMMAND: usize = 0x08;
const DEVICE_RIGHT_COMMAND: usize = 0x10;
const DEVICE_LEFT_OPTION: usize = 0x20;
const DEVICE_RIGHT_OPTION: usize = 0x40;
const DEVICE_RIGHT_CONTROL: usize = 0x2000;

/// The modifier keys whose taps the core can bind: key code, key, the
/// device bit for that side, and the flag for either side.
const SIDED: [(u16, Key, usize, usize); 8] = [
    (56, Key::ShiftLeft, DEVICE_LEFT_SHIFT, SHIFT),
    (60, Key::ShiftRight, DEVICE_RIGHT_SHIFT, SHIFT),
    (59, Key::CtrlLeft, DEVICE_LEFT_CONTROL, CONTROL),
    (62, Key::CtrlRight, DEVICE_RIGHT_CONTROL, CONTROL),
    (55, Key::CmdLeft, DEVICE_LEFT_COMMAND, COMMAND),
    (54, Key::CmdRight, DEVICE_RIGHT_COMMAND, COMMAND),
    (58, Key::AltLeft, DEVICE_LEFT_OPTION, OPTION),
    (61, Key::AltRight, DEVICE_RIGHT_OPTION, OPTION),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawKind {
    KeyDown,
    KeyUp,
    FlagsChanged,
    Other,
}

/// What the IME reads from an `NSEvent`.
#[derive(Clone, Copy, Debug)]
pub struct RawEvent<'a> {
    pub kind: RawKind,
    pub key_code: u16,
    pub flags: usize,
    pub characters: Option<&'a str>,
    pub characters_ignoring_modifiers: Option<&'a str>,
    pub time_ms: u64,
    /// The event's `kCGEventSourceUserData`.
    pub user_data: i64,
    /// A key-down sent again while the key is held.
    pub repeat: bool,
}

/// Remembers which modifier keys are down, because a `flagsChanged` event
/// does not always say whether its key went down or up.
#[derive(Debug, Default)]
pub struct Keys {
    down: [bool; 8],
    /// The last `flagsChanged`: its key code, flags and time.
    last_flags: Option<(u16, usize, u64)>,
    /// Each key down, by key code: the modifiers held as it is let go may
    /// make it read as another character.
    typed: Vec<(u16, Key)>,
}

/// Some applications, such as Slack, deliver each `flagsChanged` twice a few
/// milliseconds apart. No finger presses and releases a key this fast, so a
/// repeat within this time is the same event.
const REPEAT_MS: u64 = 15;

impl Keys {
    /// `None` means the event is not a key press or release the core needs.
    pub fn translate(&mut self, raw: RawEvent) -> Option<KeyEvent> {
        // A key the IME posted is already what the core asked for; remapped
        // again, a cycle of remaps would never end.
        if raw.user_data == POSTED_MARK {
            return None;
        }
        let mods = Modifiers {
            shift: raw.flags & SHIFT != 0,
            ctrl: raw.flags & CONTROL != 0,
            cmd: raw.flags & COMMAND != 0,
            alt: raw.flags & OPTION != 0,
        };
        let event = |key, kind| KeyEvent {
            key,
            mods,
            kind,
            time_ms: raw.time_ms,
        };
        match raw.kind {
            RawKind::FlagsChanged => {
                let this = (raw.key_code, raw.flags, raw.time_ms);
                let repeated = self.last_flags.is_some_and(|(code, flags, time)| {
                    (code, flags) == (this.0, this.1) && this.2.saturating_sub(time) < REPEAT_MS
                });
                self.last_flags = Some(this);
                if repeated {
                    return None;
                }
                let Some(index) = SIDED.iter().position(|(code, ..)| *code == raw.key_code) else {
                    // Caps Lock or Fn going down ends a tap. It goes as a bare key, so
                    // the core does not commit as for a shortcut.
                    let modifier = match raw.key_code {
                        57 => CAPS_LOCK,
                        63 => FUNCTION,
                        _ => return None,
                    };
                    let down = raw.flags & modifier != 0;
                    return down.then(|| KeyEvent {
                        key: Key::Modifier,
                        mods: Modifiers::default(),
                        kind: KeyKind::Press,
                        time_ms: raw.time_ms,
                    });
                };
                let (_, key, device_bit, flag) = SIDED[index];
                let pressed =
                    self.pressed(index, raw.flags & device_bit != 0, raw.flags & flag != 0);
                let kind = if pressed {
                    KeyKind::Press
                } else {
                    KeyKind::Release
                };
                Some(event(key, kind))
            }
            RawKind::KeyUp => {
                let key = match self
                    .typed
                    .iter()
                    .position(|(code, _)| *code == raw.key_code)
                {
                    Some(index) => self.typed.swap_remove(index).1,
                    None => key(raw),
                };
                Some(event(key, KeyKind::Release))
            }
            RawKind::KeyDown => {
                // A key repeating stays the key first pressed, whatever
                // modifier went down since. A new press replaces what a
                // release lost across a focus change left behind.
                let first = self
                    .typed
                    .iter()
                    .position(|(code, _)| *code == raw.key_code);
                let key = match first {
                    Some(index) if raw.repeat => self.typed[index].1,
                    _ => {
                        let key = key(raw);
                        self.typed.retain(|(code, _)| *code != raw.key_code);
                        self.typed.push((raw.key_code, key));
                        key
                    }
                };
                // A symbol typed with Shift is that symbol (`:`), as a binding
                // writes it; only a letter keeps its Shift (`shift+a`).
                let mods = match key {
                    Key::Char(c) if !c.is_ascii_alphabetic() => Modifiers {
                        shift: false,
                        ..mods
                    },
                    _ => mods,
                };
                Some(KeyEvent {
                    key,
                    mods,
                    kind: KeyKind::Press,
                    time_ms: raw.time_ms,
                })
            }
            RawKind::Other => None,
        }
    }

    /// The device-dependent bit says so directly, but synthetic events lack
    /// it. Without it, no flag for either side means released; otherwise the
    /// key toggled from its last known state.
    fn pressed(&mut self, index: usize, device_bit: bool, either_side: bool) -> bool {
        let pressed = if device_bit {
            true
        } else if !either_side {
            false
        } else {
            !self.down[index]
        };
        self.down[index] = pressed;
        pressed
    }
}

fn key(raw: RawEvent) -> Key {
    match raw.key_code {
        102 => Key::Eisu,
        104 => Key::Kana,
        49 => Key::Space,
        36 | 76 => Key::Enter,
        53 => Key::Esc,
        51 => Key::Backspace,
        117 => Key::Delete,
        123 => Key::Left,
        124 => Key::Right,
        125 => Key::Down,
        126 => Key::Up,
        115 => Key::Home,
        119 => Key::End,
        122 => Key::F(1),
        120 => Key::F(2),
        99 => Key::F(3),
        118 => Key::F(4),
        96 => Key::F(5),
        97 => Key::F(6),
        98 => Key::F(7),
        100 => Key::F(8),
        101 => Key::F(9),
        109 => Key::F(10),
        103 => Key::F(11),
        111 => Key::F(12),
        _ => {
            // With Control held, `characters` is a control code, and with Option
            // another character (ƒ for F); the core matches the key's own
            // character (Ctrl+Z, Option+F).
            let characters = if raw.flags & (CONTROL | OPTION) != 0 {
                raw.characters_ignoring_modifiers
            } else {
                raw.characters
            };
            match characters.and_then(|s| s.chars().next()) {
                Some(c) if !c.is_control() && !is_function_key(c) => Key::Char(c),
                _ => Key::Other,
            }
        }
    }
}

/// AppKit reports arrows, Home, Page Up and the like as characters in this
/// private-use range (NSUpArrowFunctionKey …).
fn is_function_key(c: char) -> bool {
    ('\u{F700}'..='\u{F8FF}').contains(&c)
}

/// Marks the keys the IME posts itself (`kCGEventSourceUserData`), so it
/// knows them when they come back to it.
pub const POSTED_MARK: i64 = 0x6B61_6E61_656D_6900;

// CGEventFlags.
const CG_SHIFT: u64 = 0x2_0000;
const CG_CONTROL: u64 = 0x4_0000;
const CG_OPTION: u64 = 0x8_0000;
const CG_COMMAND: u64 = 0x10_0000;
const CG_NUMERIC_PAD: u64 = 0x20_0000;
const CG_FUNCTION: u64 = 0x80_0000;

/// The virtual key code and the exact flags of a key the IME sends to the
/// application. The flags are set in full so that the Ctrl still held for
/// Ctrl+H does not leak into the Backspace sent for it.
pub fn key_to_send(chord: Chord) -> Option<(u16, u64)> {
    let (code, mut flags) = match chord.key {
        Key::Backspace => (51, 0),
        Key::Delete => (117, CG_FUNCTION),
        Key::Enter => (36, 0),
        Key::Esc => (53, 0),
        Key::Space => (49, 0),
        Key::Left => (123, CG_FUNCTION | CG_NUMERIC_PAD),
        Key::Right => (124, CG_FUNCTION | CG_NUMERIC_PAD),
        Key::Down => (125, CG_FUNCTION | CG_NUMERIC_PAD),
        Key::Up => (126, CG_FUNCTION | CG_NUMERIC_PAD),
        Key::Home => (115, CG_FUNCTION),
        Key::End => (119, CG_FUNCTION),
        _ => return None,
    };
    for (held, flag) in [
        (chord.mods.shift, CG_SHIFT),
        (chord.mods.ctrl, CG_CONTROL),
        (chord.mods.alt, CG_OPTION),
        (chord.mods.cmd, CG_COMMAND),
    ] {
        if held {
            flags |= flag;
        }
    }
    Some((code, flags))
}

/// The UTF-16 offset of the `chars`-th character, as `NSRange` counts.
pub fn utf16_offset(text: &str, chars: usize) -> usize {
    text.chars().take(chars).map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(key_code: u16, flags: usize, characters: &str) -> RawEvent<'_> {
        RawEvent {
            kind: RawKind::KeyDown,
            key_code,
            flags,
            characters: Some(characters),
            characters_ignoring_modifiers: Some(characters),
            time_ms: 7,
            user_data: 0,
            repeat: false,
        }
    }

    fn flags_changed(key_code: u16, flags: usize) -> RawEvent<'static> {
        RawEvent {
            kind: RawKind::FlagsChanged,
            key_code,
            flags,
            characters: None,
            characters_ignoring_modifiers: None,
            time_ms: 7,
            user_data: 0,
            repeat: false,
        }
    }

    fn key_of(raw: RawEvent) -> Option<Key> {
        Keys::default().translate(raw).map(|e| e.key)
    }

    fn up(key_code: u16, flags: usize, characters: &str) -> RawEvent<'_> {
        RawEvent {
            kind: RawKind::KeyUp,
            ..down(key_code, flags, characters)
        }
    }

    #[test]
    fn a_key_let_go_is_the_key_its_press_was() {
        let mut keys = Keys::default();
        keys.translate(down(0, SHIFT, "A"));
        let e = keys.translate(up(0, 0, "a")).unwrap();
        assert_eq!((e.key, e.kind), (Key::Char('A'), KeyKind::Release));
        let e = keys.translate(up(0, 0, "a")).unwrap();
        assert_eq!(
            e.key,
            Key::Char('a'),
            "a release with no press reads as it is"
        );
    }

    #[test]
    fn a_key_repeating_is_the_key_first_pressed() {
        let mut keys = Keys::default();
        keys.translate(down(0, 0, "a"));
        let repeat = RawEvent {
            repeat: true,
            ..down(0, SHIFT, "A")
        };
        assert_eq!(keys.translate(repeat).unwrap().key, Key::Char('a'));
        let e = keys.translate(up(0, SHIFT, "A")).unwrap();
        assert_eq!(e.key, Key::Char('a'));
    }

    #[test]
    fn a_new_press_is_read_afresh_though_its_release_was_lost() {
        let mut keys = Keys::default();
        keys.translate(down(0, SHIFT, "A"));
        assert_eq!(keys.translate(down(0, 0, "a")).unwrap().key, Key::Char('a'));
    }

    #[test]
    fn a_named_key_let_go_is_released() {
        let e = Keys::default().translate(up(49, 0, " ")).unwrap();
        assert_eq!((e.key, e.kind), (Key::Space, KeyKind::Release));
    }

    #[test]
    fn a_letter_is_its_character() {
        let e = Keys::default().translate(down(0, 0, "a")).unwrap();
        assert_eq!(
            e,
            KeyEvent {
                key: Key::Char('a'),
                mods: Modifiers::default(),
                kind: KeyKind::Press,
                time_ms: 7
            }
        );
        let e = Keys::default().translate(down(0, SHIFT, "A")).unwrap();
        assert_eq!((e.key, e.mods.shift), (Key::Char('A'), true));
    }

    #[test]
    fn control_reads_the_plain_letter() {
        let raw = RawEvent {
            characters: Some("\u{1a}"),
            characters_ignoring_modifiers: Some("z"),
            ..down(6, CONTROL, "")
        };
        let e = Keys::default().translate(raw).unwrap();
        assert_eq!((e.key, e.mods.ctrl), (Key::Char('z'), true));
    }

    #[test]
    fn named_keys() {
        assert_eq!(key_of(down(49, 0, " ")), Some(Key::Space));
        assert_eq!(key_of(down(36, 0, "\r")), Some(Key::Enter));
        assert_eq!(key_of(down(76, 0, "\u{3}")), Some(Key::Enter));
        assert_eq!(key_of(down(53, 0, "\u{1b}")), Some(Key::Esc));
        assert_eq!(key_of(down(51, 0, "\u{7f}")), Some(Key::Backspace));
        assert_eq!(key_of(down(117, 0, "\u{f728}")), Some(Key::Delete));
        assert_eq!(key_of(down(97, 0, "\u{f709}")), Some(Key::F(6)));
        assert_eq!(key_of(down(103, 0, "\u{f70e}")), Some(Key::F(11)));
        assert_eq!(key_of(down(111, 0, "\u{f70f}")), Some(Key::F(12)));
        assert_eq!(key_of(down(98, 0, "\u{f70a}")), Some(Key::F(7)));
        assert_eq!(key_of(down(102, 0, "")), Some(Key::Eisu));
        assert_eq!(key_of(down(104, 0, "")), Some(Key::Kana));
    }

    #[test]
    fn cursor_keys() {
        assert_eq!(key_of(down(123, 0, "\u{f702}")), Some(Key::Left));
        assert_eq!(key_of(down(124, 0, "\u{f703}")), Some(Key::Right));
        assert_eq!(key_of(down(125, 0, "\u{f701}")), Some(Key::Down));
        assert_eq!(key_of(down(126, 0, "\u{f700}")), Some(Key::Up));
        assert_eq!(key_of(down(115, 0, "\u{f729}")), Some(Key::Home));
        assert_eq!(key_of(down(119, 0, "\u{f72b}")), Some(Key::End));
    }

    #[test]
    fn option_is_reported_with_the_key() {
        let e = Keys::default().translate(down(0, OPTION, "å")).unwrap();
        assert_eq!((e.key, e.mods.alt), (Key::Char('å'), true));
        let e = Keys::default().translate(down(49, OPTION, " ")).unwrap();
        assert_eq!((e.key, e.mods.alt), (Key::Space, true));
    }

    #[test]
    fn keys_the_core_does_not_handle_are_other() {
        assert_eq!(key_of(down(48, 0, "\t")), Some(Key::Other));
    }

    #[test]
    fn a_key_the_ime_posted_itself_passes_to_the_application() {
        // Remapped again, `left = "right"` with `right = "left"` would post
        // keys forever.
        let posted = RawEvent {
            user_data: POSTED_MARK,
            ..down(123, 0, "\u{f702}")
        };
        assert_eq!(key_of(posted), None);
        let typed = RawEvent {
            user_data: 1,
            ..down(123, 0, "\u{f702}")
        };
        assert_eq!(key_of(typed), Some(Key::Left), "another program's mark");
    }

    #[test]
    fn shift_presses_and_releases_by_the_device_bit() {
        let mut keys = Keys::default();
        let e = keys
            .translate(flags_changed(60, SHIFT | DEVICE_RIGHT_SHIFT))
            .unwrap();
        assert_eq!((e.key, e.kind), (Key::ShiftRight, KeyKind::Press));
        let e = keys.translate(flags_changed(60, 0)).unwrap();
        assert_eq!((e.key, e.kind), (Key::ShiftRight, KeyKind::Release));
        let e = keys
            .translate(flags_changed(56, SHIFT | DEVICE_LEFT_SHIFT))
            .unwrap();
        assert_eq!((e.key, e.kind), (Key::ShiftLeft, KeyKind::Press));
    }

    #[test]
    fn control_command_and_option_press_and_release_by_side() {
        let mut keys = Keys::default();
        for (code, key, device, flag) in [
            (59, Key::CtrlLeft, DEVICE_LEFT_CONTROL, CONTROL),
            (62, Key::CtrlRight, DEVICE_RIGHT_CONTROL, CONTROL),
            (55, Key::CmdLeft, DEVICE_LEFT_COMMAND, COMMAND),
            (54, Key::CmdRight, DEVICE_RIGHT_COMMAND, COMMAND),
            (58, Key::AltLeft, DEVICE_LEFT_OPTION, OPTION),
            (61, Key::AltRight, DEVICE_RIGHT_OPTION, OPTION),
        ] {
            let e = keys.translate(flags_changed(code, flag | device)).unwrap();
            assert_eq!((e.key, e.kind), (key, KeyKind::Press), "{key:?}");
            let e = keys.translate(flags_changed(code, 0)).unwrap();
            assert_eq!((e.key, e.kind), (key, KeyKind::Release), "{key:?}");
        }
    }

    #[test]
    fn option_and_shift_keys_come_as_the_keys_on_the_keyboard() {
        let mut keys = Keys::default();
        let raw = RawEvent {
            kind: RawKind::KeyDown,
            key_code: 3,
            flags: OPTION,
            characters: Some("\u{192}"),
            characters_ignoring_modifiers: Some("f"),
            time_ms: 0,
            user_data: 0,
            repeat: false,
        };
        let e = keys.translate(raw).unwrap();
        assert_eq!(
            (e.key, e.mods.alt),
            (Key::Char('f'), true),
            "Option+F, not \u{192}"
        );
        let raw = RawEvent {
            kind: RawKind::KeyDown,
            key_code: 41,
            flags: SHIFT,
            characters: Some(":"),
            characters_ignoring_modifiers: Some(";"),
            time_ms: 0,
            user_data: 0,
            repeat: false,
        };
        let e = keys.translate(raw).unwrap();
        assert_eq!(
            (e.key, e.mods.shift),
            (Key::Char(':'), false),
            "a shifted symbol is the symbol"
        );
    }

    #[test]
    fn caps_lock_or_fn_going_down_is_a_modifier_key() {
        let mut keys = Keys::default();
        for (code, flag) in [(57, CAPS_LOCK), (63, FUNCTION)] {
            let e = keys.translate(flags_changed(code, flag | SHIFT)).unwrap();
            assert_eq!((e.key, e.mods), (Key::Modifier, Modifiers::default()));
            assert_eq!(keys.translate(flags_changed(code, SHIFT)), None);
        }
    }

    #[test]
    fn without_the_device_bit_shift_toggles() {
        let mut keys = Keys::default();
        let e = keys.translate(flags_changed(56, SHIFT)).unwrap();
        assert_eq!(e.kind, KeyKind::Press);
        let e = keys.translate(at(flags_changed(56, SHIFT), 200)).unwrap();
        assert_eq!(e.kind, KeyKind::Release);
    }

    #[test]
    fn keys_to_send_carry_only_their_own_modifiers() {
        let plain = |key| Chord {
            key,
            mods: Modifiers::default(),
        };
        assert_eq!(key_to_send(plain(Key::Backspace)), Some((51, 0)));
        assert_eq!(
            key_to_send(plain(Key::Left)),
            Some((123, CG_FUNCTION | CG_NUMERIC_PAD))
        );
        let shifted = Chord {
            key: Key::Space,
            mods: Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        assert_eq!(key_to_send(shifted), Some((49, CG_SHIFT)));
        assert_eq!(key_to_send(plain(Key::Char('a'))), None);
    }

    #[test]
    fn the_cursor_counts_utf16_units() {
        assert_eq!(utf16_offset("›かんじ", 2), 2);
        assert_eq!(utf16_offset("𝄞あ", 1), 2);
        assert_eq!(utf16_offset("あ", 5), 1);
    }

    fn at(raw: RawEvent<'static>, time_ms: u64) -> RawEvent<'static> {
        RawEvent { time_ms, ..raw }
    }

    #[test]
    fn a_flags_change_delivered_twice_counts_once() {
        // Slack sends each flagsChanged twice, without the side's bit.
        let mut keys = Keys::default();
        let kinds: Vec<_> = [
            at(flags_changed(56, SHIFT), 100),
            at(flags_changed(56, SHIFT), 103),
            at(flags_changed(56, 0), 400),
            at(flags_changed(56, 0), 402),
        ]
        .into_iter()
        .map(|raw| keys.translate(raw).map(|e| e.kind))
        .collect();
        assert_eq!(
            kinds,
            [Some(KeyKind::Press), None, Some(KeyKind::Release), None]
        );
    }

    #[test]
    fn a_quick_tap_while_the_other_shift_is_held_still_releases() {
        let mut keys = Keys::default();
        keys.translate(at(flags_changed(60, SHIFT), 100));
        let press = keys.translate(at(flags_changed(56, SHIFT), 200)).unwrap();
        let release = keys.translate(at(flags_changed(56, SHIFT), 260)).unwrap();
        assert_eq!(
            (press.kind, release.kind),
            (KeyKind::Press, KeyKind::Release)
        );
    }
}
