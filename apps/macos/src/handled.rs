//! Keys the IME handles without changing the text. Some applications, many
//! terminals among them, hand a key to the input method and pass it on as
//! well unless it inserted or marked text, as they cannot tell it was
//! handled. A key handled so is marked through with text that shows
//! nothing, taken back once the application is done with the key.

use kanaemi_core::Output;

/// What is marked through a key handled without changing the text.
pub const UNSEEN: &str = "\u{200B}";

/// Whether the key press handled into `output` changes nothing an
/// application could tell it by: it was handled, or sent as another key,
/// and leaves no text committed or marked, nor text marked before it,
/// which the application takes for the key's doing.
pub fn changes_nothing(output: &Output, marked_before: bool) -> bool {
    (output.consumed || output.send.is_some())
        && output.commit.is_none()
        && output.preedit.is_empty()
        && !marked_before
}

#[cfg(test)]
mod tests {
    use kanaemi_core::{Chord, Key, Mode, Modifiers};

    use super::*;

    fn passed_on() -> Output {
        Output {
            consumed: false,
            erase: None,
            commit: None,
            preedit: String::new(),
            cursor: 0,
            candidates: None,
            mode: Mode::Kana,
            indicator: None,
            send: None,
            effects: Vec::new(),
        }
    }

    fn handled() -> Output {
        Output {
            consumed: true,
            ..passed_on()
        }
    }

    #[test]
    fn a_key_that_only_erases_or_sends_another_changes_nothing() {
        let erasing = Output {
            erase: Some("かんじ".to_owned()),
            ..handled()
        };
        assert!(changes_nothing(&erasing, false));
        let sending = Output {
            send: Some(Chord {
                key: Key::Up,
                mods: Modifiers::default(),
            }),
            ..passed_on()
        };
        assert!(changes_nothing(&sending, false));
    }

    #[test]
    fn a_key_that_commits_or_marks_text_changes_it() {
        let committing = Output {
            commit: Some("か".to_owned()),
            ..handled()
        };
        assert!(!changes_nothing(&committing, false));
        let marking = Output {
            preedit: "›か".to_owned(),
            ..handled()
        };
        assert!(!changes_nothing(&marking, false));
        assert!(
            !changes_nothing(&handled(), true),
            "taking marked text away changes it"
        );
    }

    #[test]
    fn a_key_passed_on_is_left_to_the_application() {
        assert!(!changes_nothing(&passed_on(), false));
    }
}
