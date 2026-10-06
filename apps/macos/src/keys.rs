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
    /// Each key down, by key code, with the time it went down: the modifiers
    /// held as it is let go may make it read as another character.
    typed: Vec<(u16, Key, u64)>,
    /// Presses an event tap saw that have not reached the IME yet, by key
    /// code and time.
    tapped_downs: Vec<(u16, u64)>,
    /// Releases an event tap saw, in the order let go, not yet passed on.
    tapped_ups: Vec<(u16, u64)>,
    /// Presses that reached the IME before the tap told of them, by key code
    /// and time, kept for [`TAP_GRACE_MS`]: the tap's word on them comes late.
    untapped_downs: Vec<(u16, u64)>,
    /// The time of the last key the IME had. Keys reach it in the order
    /// they were typed, so a press the tap saw before this never comes.
    ime_seen_ms: u64,
    /// The time of the key the IME is about to pass on, which the keys let
    /// go before it go ahead of.
    pressing_ms: u64,
}

/// How long the tap may tell of a press after the IME had it, and a key may
/// stay down with no release from the tap before the keyboard is asked.
pub const TAP_GRACE_MS: u64 = 300;

/// Keys let go this close together were lifted as one.
pub const SIMULTANEOUS_MS: u64 = 20;

/// How long a release waits on a press the tap saw while the IME has no key
/// after it to say the press will never come.
pub const TAP_GIVE_UP_MS: u64 = 1000;

/// The flag a modifier key sets while it is down, by key code.
fn modifier_flag(code: u16) -> Option<usize> {
    match code {
        57 => Some(CAPS_LOCK),
        63 => Some(FUNCTION),
        _ => SIDED
            .iter()
            .find(|(sided, ..)| *sided == code)
            .map(|&(.., flag)| flag),
    }
}

/// Some applications, such as Slack, deliver each `flagsChanged` twice a few
/// milliseconds apart. No finger presses and releases a key this fast, so a
/// repeat within this time is the same event.
const REPEAT_MS: u64 = 15;

impl Keys {
    /// Whether a key is down whose release has not been seen.
    pub fn watching(&self) -> bool {
        !self.typed.is_empty()
    }

    /// Releases the keys `down` says are no longer down, as of `time_ms`.
    /// Input Method Kit never passes a key-up on, so a key let go is found
    /// by asking the keyboard; the modifiers it was let go with are unknown
    /// and left out, as the core reads only the key of a release.
    pub fn lifted(&mut self, down: impl Fn(u16) -> bool, time_ms: u64) -> Vec<KeyEvent> {
        let mut lifted = Vec::new();
        self.typed.retain(|(code, key, _)| {
            let still = down(*code);
            if !still {
                lifted.push(KeyEvent {
                    key: *key,
                    mods: Modifiers::default(),
                    kind: KeyKind::Release,
                    time_ms,
                });
            }
            still
        });
        lifted
    }

    /// A key pressed or let go, as an event tap saw it. The tap sees the
    /// keyboard ahead of Input Method Kit, so what it saw is held until the
    /// IME has the presses that came first.
    pub fn tapped(&mut self, code: u16, down: bool, time_ms: u64) {
        if down {
            // The IME had it first: it is no press to wait for.
            if let Some(index) = same_event(&self.untapped_downs, code, time_ms) {
                self.untapped_downs.remove(index);
                return;
            }
            self.tapped_downs.push((code, time_ms));
        } else if !self
            .untapped_downs
            .iter()
            .any(|&(seen, time)| seen == code && time > time_ms)
        {
            // Dropped when the IME already had the key pressed again: that
            // press let this one go, and the release would take the new one.
            self.tapped_ups.push((code, time_ms));
        }
    }

    /// A key the IME is about to pass on: the keys let go before it go first,
    /// with no wait for others lifted with them.
    pub fn pressing(&mut self, raw: RawEvent) {
        let pressed = match raw.kind {
            RawKind::KeyDown => true,
            // A modifier let go presses nothing.
            RawKind::FlagsChanged => {
                modifier_flag(raw.key_code).is_some_and(|flag| raw.flags & flag != 0)
            }
            RawKind::KeyUp | RawKind::Other => false,
        };
        if pressed && raw.user_data != POSTED_MARK {
            self.pressing_ms = self.pressing_ms.max(raw.time_ms);
        }
    }

    /// Whether the tap saw something not yet settled.
    pub fn waiting(&self) -> bool {
        !self.tapped_downs.is_empty() || !self.tapped_ups.is_empty()
    }

    /// Releases the keys the tap saw let go, at the times they were let go
    /// and in that order, as far as the IME has every press before them. A
    /// press the IME went past, or one waited on past [`TAP_GIVE_UP_MS`], is
    /// for a key this IME never has, such as one an application took first,
    /// and is dropped. A busy main thread delays the IME's presses as much as
    /// the tap's releases, so they are waited for by order, not by the clock.
    pub fn tapped_releases(&mut self, now_ms: u64) -> Vec<KeyEvent> {
        let seen = self.ime_seen_ms;
        self.tapped_downs
            .retain(|&(_, time)| time >= seen && time + TAP_GIVE_UP_MS >= now_ms);
        let mut released = Vec::new();
        while let Some(&(_, first)) = self.tapped_ups.first() {
            // Keys lifted as one reach the tap one at a time: the rest of
            // them are waited for, as far as a press after the first; a key
            // pressed since bounds them, and its press must not go first.
            let pressed_since = self
                .tapped_downs
                .iter()
                .map(|&(_, time)| time)
                .chain((self.pressing_ms >= first).then_some(self.pressing_ms))
                // A press in the same millisecond came after: the tap told
                // of the release first.
                .filter(|&time| time >= first)
                .min();
            let end = pressed_since.map_or(first + SIMULTANEOUS_MS, |pressed| {
                pressed
                    .saturating_sub(1)
                    .max(first)
                    .min(first + SIMULTANEOUS_MS)
            });
            let closed = pressed_since.is_some()
                || first + SIMULTANEOUS_MS < now_ms
                || self.tapped_ups.iter().any(|&(_, time)| time > end);
            let together = self
                .tapped_ups
                .iter()
                .take_while(|&&(_, time)| time <= end)
                .count();
            let ups = &self.tapped_ups[..together];
            let last = ups[together - 1].1;
            // A press in the same millisecond is after them, unless it is of
            // a key among them.
            let earlier = self.tapped_downs.iter().any(|&(code, down)| {
                down < last || (down == last && ups.iter().any(|&(up, _)| up == code))
            });
            if !closed || earlier {
                break;
            }
            // A key not down here was pressed before the focus came, or in
            // another application: nothing waits on it.
            let mut group: Vec<(u64, Key, u64)> = Vec::new();
            for (code, time) in self.tapped_ups.drain(..together) {
                if let Some(index) = self.typed.iter().position(|(typed, ..)| *typed == code) {
                    let (_, key, at) = self.typed.remove(index);
                    group.push((at, key, time));
                }
            }
            // As a chord comes apart, the key pressed last first: a letter
            // lifted with the key held under it was typed held.
            group.sort_by_key(|&(at, ..)| std::cmp::Reverse(at));
            released.extend(group.into_iter().map(|(_, key, time)| KeyEvent {
                key,
                mods: Modifiers::default(),
                kind: KeyKind::Release,
                time_ms: time,
            }));
        }
        released
    }

    /// Releases the keys down past [`TAP_GRACE_MS`] that `down` says are up
    /// and whose release the tap has not told of, as of `now_ms`: the tap
    /// lost them, as when macOS turned it off for a while.
    pub fn lost(&mut self, down: impl Fn(u16) -> bool, now_ms: u64) -> Vec<KeyEvent> {
        let mut lost = Vec::new();
        let told = &self.tapped_ups;
        self.typed.retain(|&(code, key, at)| {
            let gone = at + TAP_GRACE_MS < now_ms
                && !told.iter().any(|&(up, _)| up == code)
                && !down(code);
            if gone {
                lost.push(KeyEvent {
                    key,
                    mods: Modifiers::default(),
                    kind: KeyKind::Release,
                    time_ms: now_ms,
                });
            }
            !gone
        });
        lost
    }

    /// The release of a key pressed anew while its last press was never seen
    /// let go: let go and pressed again between two looks at the keyboard,
    /// or across a focus change. Without it the new press reads as a repeat
    /// to a key held.
    pub fn pressed_again(&mut self, raw: RawEvent) -> Option<KeyEvent> {
        if raw.kind != RawKind::KeyDown || raw.repeat || raw.user_data == POSTED_MARK {
            return None;
        }
        let index = self
            .typed
            .iter()
            .position(|(code, ..)| *code == raw.key_code)?;
        Some(KeyEvent {
            // Kept in the order pressed, which `lifted` lets go in.
            key: self.typed.remove(index).1,
            mods: Modifiers::default(),
            kind: KeyKind::Release,
            time_ms: raw.time_ms,
        })
    }

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
                // Passed on here, the release the tap saw is not to be again.
                if let Some(index) = same_event(&self.tapped_ups, raw.key_code, raw.time_ms) {
                    self.tapped_ups.remove(index);
                }
                let key = match self
                    .typed
                    .iter()
                    .position(|(code, ..)| *code == raw.key_code)
                {
                    // Kept in the order pressed, which the rest are let go in.
                    Some(index) => self.typed.remove(index).1,
                    None => key(raw),
                };
                Some(event(key, KeyKind::Release))
            }
            RawKind::KeyDown => {
                self.ime_seen_ms = self.ime_seen_ms.max(raw.time_ms);
                // The same event as the tap saw, on the same clock.
                match same_event(&self.tapped_downs, raw.key_code, raw.time_ms) {
                    Some(index) => {
                        self.tapped_downs.remove(index);
                    }
                    None => {
                        self.untapped_downs
                            .retain(|&(_, time)| time + TAP_GRACE_MS >= raw.time_ms);
                        self.untapped_downs.push((raw.key_code, raw.time_ms));
                    }
                }
                // A key repeating stays the key first pressed, whatever
                // modifier went down since. A new press replaces what a
                // release lost across a focus change left behind.
                let first = self
                    .typed
                    .iter()
                    .position(|(code, ..)| *code == raw.key_code);
                let key = match first {
                    Some(index) if raw.repeat => self.typed[index].1,
                    _ => {
                        let key = key(raw);
                        self.typed.retain(|(code, ..)| *code != raw.key_code);
                        // A release the tap saw before this press was of the
                        // last one, which this press let go.
                        self.tapped_ups
                            .retain(|&(code, time)| code != raw.key_code || time > raw.time_ms);
                        self.typed.push((raw.key_code, key, raw.time_ms));
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

/// Where in `seen` the tap's and Input Method Kit's word on one event is:
/// the same key at the same time, on the same clock.
fn same_event(seen: &[(u16, u64)], code: u16, time_ms: u64) -> Option<usize> {
    seen.iter()
        .position(|&(seen, time)| seen == code && time.abs_diff(time_ms) <= 1)
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
    fn a_key_found_up_is_let_go_once() {
        let mut keys = Keys::default();
        keys.translate(down(0, SHIFT, "A"));
        keys.translate(down(49, 0, " "));
        assert!(keys.watching());
        assert_eq!(keys.lifted(|_| true, 20), []);
        let lifted = keys.lifted(|code| code == 49, 30);
        assert_eq!(
            lifted,
            [KeyEvent {
                key: Key::Char('A'),
                mods: Modifiers::default(),
                kind: KeyKind::Release,
                time_ms: 30,
            }]
        );
        assert!(keys.watching(), "space is still down");
        let lifted = keys.lifted(|_| false, 40);
        assert_eq!(
            lifted.iter().map(|e| (e.key, e.kind)).collect::<Vec<_>>(),
            [(Key::Space, KeyKind::Release)]
        );
        assert!(!keys.watching());
        assert_eq!(keys.lifted(|_| false, 50), []);
    }

    fn at(raw: RawEvent<'_>, time_ms: u64) -> RawEvent<'_> {
        RawEvent { time_ms, ..raw }
    }

    /// The releases passed on at `now_ms`, once keys let go with them have
    /// had time to be told of.
    fn released(keys: &mut Keys, now_ms: u64) -> Vec<(Key, u64)> {
        keys.tapped_releases(now_ms + SIMULTANEOUS_MS + 1)
            .into_iter()
            .map(|e| {
                assert_eq!(e.kind, KeyKind::Release);
                (e.key, e.time_ms)
            })
            .collect()
    }

    /// The tap and Input Method Kit both see the press.
    fn press(keys: &mut Keys, raw: RawEvent<'_>) {
        keys.tapped(raw.key_code, true, raw.time_ms);
        keys.translate(raw);
    }

    #[test]
    fn keys_let_go_go_in_the_order_the_tap_saw_them() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        press(&mut keys, at(down(40, 0, "k"), 50));
        keys.tapped(40, false, 80);
        keys.tapped(49, false, 85);
        assert_eq!(
            released(&mut keys, 90),
            [(Key::Char('k'), 80), (Key::Space, 85)]
        );
        assert_eq!(released(&mut keys, 95), []);
    }

    #[test]
    fn keys_let_go_together_go_the_last_pressed_first() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        press(&mut keys, at(down(11, 0, "b"), 218));
        // Lifted as one: the tap may tell of either first.
        keys.tapped(49, false, 370);
        keys.tapped(11, false, 370 + SIMULTANEOUS_MS);
        assert_eq!(
            released(&mut keys, 400),
            [(Key::Char('b'), 370 + SIMULTANEOUS_MS), (Key::Space, 370)]
        );
    }

    #[test]
    fn a_key_let_go_goes_before_a_key_pressed_after_it() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        keys.tapped(49, false, 400);
        assert_eq!(
            keys.tapped_releases(410),
            [],
            "another may be lifted with it"
        );
        keys.pressing(at(down(11, 0, "b"), 410));
        let released: Vec<_> = keys
            .tapped_releases(410)
            .into_iter()
            .map(|e| (e.key, e.time_ms))
            .collect();
        assert_eq!(released, [(Key::Space, 400)]);
    }

    #[test]
    fn keys_let_go_on_either_side_of_a_press_are_not_lifted_as_one() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        keys.tapped(49, false, 400);
        keys.tapped(0, true, 410);
        keys.tapped(0, false, 415);
        keys.pressing(at(down(0, 0, "a"), 410));
        let released: Vec<_> = keys
            .tapped_releases(412)
            .into_iter()
            .map(|e| (e.key, e.time_ms))
            .collect();
        assert_eq!(released, [(Key::Space, 400)], "before a goes in");
    }

    #[test]
    fn the_release_of_the_key_being_pressed_waits_for_it() {
        let mut keys = Keys::default();
        // The tap's clock reads a millisecond off the IME's.
        keys.tapped(49, true, 409);
        keys.tapped(49, false, 415);
        let space = at(down(49, 0, " "), 410);
        keys.pressing(space);
        // The main thread got to it late.
        assert_eq!(keys.tapped_releases(500), []);
        keys.translate(space);
        let released: Vec<_> = keys
            .tapped_releases(500)
            .into_iter()
            .map(|e| (e.key, e.time_ms))
            .collect();
        assert_eq!(released, [(Key::Space, 415)]);
    }

    #[test]
    fn a_modifier_pressed_after_a_key_let_go_bounds_it_too() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        keys.tapped(49, false, 400);
        keys.pressing(at(flags_changed(56, SHIFT | DEVICE_LEFT_SHIFT), 410));
        let released: Vec<_> = keys
            .tapped_releases(412)
            .into_iter()
            .map(|e| (e.key, e.time_ms))
            .collect();
        assert_eq!(released, [(Key::Space, 400)]);
    }

    #[test]
    fn a_modifier_let_go_bounds_nothing() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        press(&mut keys, at(down(11, 0, "b"), 100));
        keys.tapped(49, false, 195);
        keys.pressing(at(flags_changed(56, 0), 200));
        keys.tapped(11, false, 210);
        assert_eq!(
            released(&mut keys, 210),
            [(Key::Char('b'), 210), (Key::Space, 195)]
        );
    }

    #[test]
    fn a_key_pressed_in_the_millisecond_another_is_let_go_bounds_it() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        keys.tapped(49, false, 400);
        keys.tapped(0, true, 400);
        keys.tapped(0, false, 410);
        keys.pressing(at(down(0, 0, "a"), 400));
        let released: Vec<_> = keys
            .tapped_releases(401)
            .into_iter()
            .map(|e| (e.key, e.time_ms))
            .collect();
        assert_eq!(released, [(Key::Space, 400)]);
    }

    #[test]
    fn keys_let_go_apart_go_in_the_order_let_go() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        press(&mut keys, at(down(11, 0, "b"), 218));
        keys.tapped(49, false, 370);
        keys.tapped(11, false, 370 + SIMULTANEOUS_MS + 1);
        assert_eq!(
            released(&mut keys, 450),
            [
                (Key::Space, 370),
                (Key::Char('b'), 370 + SIMULTANEOUS_MS + 1)
            ]
        );
    }

    #[test]
    fn a_key_let_go_waits_for_its_press() {
        let mut keys = Keys::default();
        keys.tapped(40, true, 10);
        keys.tapped(40, false, 20);
        assert_eq!(released(&mut keys, 25), []);
        keys.translate(at(down(40, 0, "k"), 10));
        assert_eq!(released(&mut keys, 26), [(Key::Char('k'), 20)]);
    }

    #[test]
    fn a_key_let_go_waits_for_the_keys_pressed_before_it() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        keys.tapped(38, true, 30);
        keys.tapped(49, false, 40);
        assert_eq!(released(&mut keys, 45), [], "j was pressed first");
        keys.translate(at(down(38, 0, "j"), 30));
        assert_eq!(released(&mut keys, 46), [(Key::Space, 40)]);
    }

    #[test]
    fn a_press_the_ime_had_before_the_tap_holds_nothing_up() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        keys.translate(at(down(40, 0, "k"), 30));
        keys.tapped(40, true, 30);
        keys.tapped(49, false, 40);
        assert_eq!(released(&mut keys, 45), [(Key::Space, 40)]);
    }

    #[test]
    fn a_release_the_ime_had_is_not_passed_on_again() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(40, 0, "k"), 0));
        press(&mut keys, at(down(49, 0, " "), 10));
        keys.tapped(40, false, 20);
        keys.translate(RawEvent {
            kind: RawKind::KeyUp,
            ..at(down(40, 0, "k"), 20)
        });
        keys.tapped(49, false, 30);
        assert_eq!(released(&mut keys, 35), [(Key::Space, 30)]);
    }

    #[test]
    fn a_release_the_tap_tells_of_after_the_next_press_keeps_that_press() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(40, 0, "k"), 0));
        let again = at(down(40, 0, "k"), 30);
        assert!(keys.pressed_again(again).is_some());
        keys.translate(again);
        keys.tapped(40, false, 20);
        keys.tapped(40, true, 30);
        assert_eq!(released(&mut keys, 35), []);
        keys.tapped(40, false, 50);
        assert_eq!(released(&mut keys, 55), [(Key::Char('k'), 50)]);
    }

    #[test]
    fn a_release_held_back_is_dropped_by_the_next_press_of_its_key() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(40, 0, "k"), 0));
        // A press the IME has yet to see holds the release back.
        keys.tapped(38, true, 10);
        keys.tapped(40, false, 20);
        keys.tapped(40, true, 30);
        let again = at(down(40, 0, "k"), 30);
        assert!(keys.pressed_again(again).is_some());
        keys.translate(again);
        keys.translate(at(down(38, 0, "j"), 10));
        assert_eq!(released(&mut keys, 35), []);
    }

    #[test]
    fn a_key_let_go_that_was_never_pressed_here_holds_nothing_up() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        // Pressed before the focus came here.
        keys.tapped(7, false, 5);
        keys.tapped(49, false, 10);
        assert_eq!(released(&mut keys, 11), [(Key::Space, 10)]);
        assert!(!keys.waiting());
    }

    #[test]
    fn a_key_the_tap_lost_is_let_go_once_the_keyboard_says_so() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        let lost = |keys: &mut Keys, down: bool, now: u64| {
            keys.lost(|_| down, now)
                .into_iter()
                .map(|e| (e.key, e.kind, e.time_ms))
                .collect::<Vec<_>>()
        };
        assert_eq!(lost(&mut keys, false, 100), [], "the tap may yet tell");
        assert_eq!(lost(&mut keys, true, 400), [], "still down");
        assert_eq!(
            lost(&mut keys, false, 400),
            [(Key::Space, KeyKind::Release, 400)]
        );
        assert!(!keys.watching());
    }

    #[test]
    fn a_key_let_go_the_tap_told_of_is_not_lost() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        // Its release waits on a press the IME has not had yet.
        keys.tapped(40, true, 380);
        keys.tapped(49, false, 390);
        assert_eq!(keys.lost(|_| false, 400), []);
    }

    #[test]
    fn keys_let_go_directly_keep_the_others_in_the_order_pressed() {
        let mut keys = Keys::default();
        for (code, c, time) in [(49, " ", 0), (40, "k", 10), (38, "j", 20)] {
            press(&mut keys, at(down(code, 0, c), time));
        }
        keys.translate(at(up(49, 0, " "), 30));
        let lifted: Vec<Key> = keys
            .lifted(|_| false, 40)
            .into_iter()
            .map(|e| e.key)
            .collect();
        assert_eq!(lifted, [Key::Char('k'), Key::Char('j')]);
    }

    #[test]
    fn a_press_that_never_comes_holds_nothing_up_for_long() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        // Typed into another application, or posted by another program.
        keys.tapped(0, true, 5);
        keys.tapped(7, false, 6);
        keys.tapped(49, false, 10);
        assert_eq!(released(&mut keys, 20), []);
        assert_eq!(
            released(&mut keys, 5 + TAP_GIVE_UP_MS + 1),
            [(Key::Space, 10)]
        );
        assert!(!keys.waiting());
    }

    #[test]
    fn a_press_the_ime_went_past_is_not_waited_for() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        // An application took it before the IME, as a menu shortcut.
        keys.tapped(8, true, 5);
        press(&mut keys, at(down(40, 0, "k"), 10));
        keys.tapped(49, false, 20);
        assert_eq!(released(&mut keys, 21), [(Key::Space, 20)]);
    }

    #[test]
    fn a_press_late_to_the_ime_is_waited_for() {
        let mut keys = Keys::default();
        press(&mut keys, at(down(49, 0, " "), 0));
        keys.tapped(40, true, 5);
        keys.tapped(40, false, 20);
        keys.tapped(49, false, 30);
        // The main thread was busy past the grace.
        assert_eq!(released(&mut keys, 400), []);
        keys.translate(at(down(40, 0, "k"), 5));
        assert_eq!(
            released(&mut keys, 401),
            [(Key::Char('k'), 20), (Key::Space, 30)]
        );
    }

    #[test]
    fn a_key_pressed_again_before_it_was_found_up_is_let_go_first() {
        let mut keys = Keys::default();
        keys.translate(down(49, 0, " "));
        let e = keys.pressed_again(down(49, 0, " ")).unwrap();
        assert_eq!((e.key, e.kind), (Key::Space, KeyKind::Release));
        keys.translate(down(49, 0, " "));
        let repeat = RawEvent {
            repeat: true,
            ..down(49, 0, " ")
        };
        assert_eq!(keys.pressed_again(repeat), None, "still held");
        assert_eq!(keys.pressed_again(down(0, 0, "a")), None, "never down");
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
