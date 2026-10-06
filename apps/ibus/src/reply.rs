//! What the engine tells IBus after an event, decided apart from D-Bus so it
//! builds and tests on every platform.

use kanaemi_core::{Candidate, Chord, Key, Mode, Modifiers, Output};

use crate::keys;

/// The input purposes of a password and a PIN, as IBus numbers them.
const SECRET_PURPOSES: [u32; 2] = [8, 9];
/// The hint of a field whose text is not to be remembered, such as one in a
/// browser's private window.
const PRIVATE_HINT: u32 = 1 << 11;

/// One thing to tell IBus, in the order the list gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signal {
    Commit(String),
    /// The preedit, with the cursor in characters from its start.
    Preedit(String, usize),
    Candidates(Vec<String>, usize),
    HideCandidates,
    Forward(u32, u32),
    /// The mode to show by the caret for a moment, after it changed.
    Indicator(Mode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub consumed: bool,
    pub signals: Vec<Signal>,
}

impl Reply {
    pub const NOTHING: Self = Self {
        consumed: false,
        signals: Vec::new(),
    };
}

/// What to tell IBus to show `output`. The mode is shown by the caret only
/// where the panel shows it well (`indicator`).
pub fn reply(output: &Output, indicator: bool) -> Reply {
    let mut signals = Vec::new();
    // Forwarded keys reach the application in order, before what follows.
    if let Some(text) = &output.erase {
        let backspace = Chord {
            key: Key::Backspace,
            mods: Modifiers::default(),
        };
        if let Some((keyval, state)) = keys::key_to_send(backspace) {
            let presses = kanaemi_runtime::backspaces(text);
            signals.extend((0..presses).map(|_| Signal::Forward(keyval, state)));
        }
    }
    if let Some(text) = &output.commit {
        signals.push(Signal::Commit(text.clone()));
    }
    signals.push(Signal::Preedit(output.preedit.clone(), output.cursor));
    signals.push(match &output.candidates {
        Some(view) => Signal::Candidates(labels(&view.items), view.selected),
        None => Signal::HideCandidates,
    });
    if let Some(mode) = output.indicator
        && indicator
    {
        signals.push(Signal::Indicator(mode));
    }
    let mut consumed = output.consumed;
    if let Some(chord) = output.send {
        match keys::key_to_send(chord) {
            Some((keyval, state)) => {
                signals.push(Signal::Forward(keyval, state));
                consumed = true;
            }
            None => tracing::warn!(?chord, "no keysym to send"),
        }
    }
    Reply { consumed, signals }
}

/// The panel's text for each candidate. IBus hands a clicked candidate back
/// by its position, so candidates that read alike need not be told apart.
fn labels(items: &[Candidate]) -> Vec<String> {
    items.iter().map(|c| c.surface.clone()).collect()
}

/// How the mode is named by the caret.
pub fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Kana => "かな",
        Mode::Abc => "ABC",
    }
}

/// Whether the panel of `desktop` (`XDG_CURRENT_DESKTOP`) shows the mode by
/// the caret well: GNOME Shell does. IBus's own panel, on other desktops,
/// keeps an empty frame of candidates up after the text goes, so there the
/// mode is not shown at all.
pub fn shows_indicator(desktop: &str) -> bool {
    desktop.split(':').any(|d| d == "GNOME")
}

/// What a field's content type says: whether it takes a secret, and
/// whether it asks that what is typed there not be recorded.
pub fn content_type(purpose: u32, hints: u32) -> (bool, bool) {
    (
        SECRET_PURPOSES.contains(&purpose),
        hints & PRIVATE_HINT != 0,
    )
}

#[cfg(test)]
mod tests {
    use kanaemi_core::{CandidateView, Chord, Key, Modifiers};

    use super::*;

    fn output() -> Output {
        Output {
            consumed: true,
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

    fn candidate(surface: &str) -> Candidate {
        Candidate {
            surface: surface.to_owned(),
        }
    }

    #[test]
    fn the_commit_comes_before_the_preedit_and_the_candidates() {
        let output = Output {
            commit: Some("漢字".to_owned()),
            preedit: "›かな".to_owned(),
            cursor: 3,
            ..output()
        };
        assert_eq!(
            reply(&output, false).signals,
            [
                Signal::Commit("漢字".to_owned()),
                Signal::Preedit("›かな".to_owned(), 3),
                Signal::HideCandidates,
            ]
        );
    }

    #[test]
    fn text_to_erase_goes_first_as_backspaces() {
        let output = Output {
            erase: Some("記者𥸮".to_owned()),
            ..output()
        };
        let backspace = Signal::Forward(0xff08, 0);
        assert_eq!(
            reply(&output, false).signals[..4],
            [
                backspace.clone(),
                backspace.clone(),
                backspace,
                Signal::Preedit(String::new(), 0)
            ]
        );
    }

    #[test]
    fn a_candidate_shows_its_surface() {
        let output = Output {
            candidates: Some(CandidateView {
                items: vec![candidate("漢字"), candidate("感じ")],
                selected: 1,
            }),
            ..output()
        };
        assert_eq!(
            reply(&output, false).signals[1],
            Signal::Candidates(vec!["漢字".to_owned(), "感じ".to_owned()], 1)
        );
    }

    #[test]
    fn the_mode_shows_by_the_caret_only_where_the_panel_shows_it_well() {
        let output = Output {
            indicator: Some(Mode::Kana),
            ..output()
        };
        assert!(
            reply(&output, true)
                .signals
                .contains(&Signal::Indicator(Mode::Kana))
        );
        assert!(
            !reply(&output, false)
                .signals
                .iter()
                .any(|s| matches!(s, Signal::Indicator(_)))
        );
    }

    #[test]
    fn a_key_to_send_is_forwarded_and_the_pressed_one_kept() {
        let output = Output {
            consumed: false,
            send: Some(Chord {
                key: Key::Backspace,
                mods: Modifiers::default(),
            }),
            ..output()
        };
        let reply = reply(&output, false);
        assert!(reply.consumed);
        assert_eq!(reply.signals.last(), Some(&Signal::Forward(0xff08, 0)));
    }

    #[test]
    fn a_key_with_no_keysym_to_send_passes_on() {
        let output = Output {
            consumed: false,
            send: Some(Chord {
                key: Key::Char('a'),
                mods: Modifiers::default(),
            }),
            ..output()
        };
        assert!(!reply(&output, false).consumed);
    }

    #[test]
    fn gnome_shows_the_mode_by_the_caret_and_other_panels_do_not() {
        assert!(shows_indicator("GNOME"));
        assert!(shows_indicator("ubuntu:GNOME"));
        assert!(!shows_indicator("KDE"));
        assert!(!shows_indicator(""));
    }

    #[test]
    fn a_password_or_pin_is_a_secret_and_the_private_hint_asks_for_no_record() {
        assert_eq!(content_type(8, 0), (true, false));
        assert_eq!(content_type(9, 0), (true, false));
        assert_eq!(content_type(0, 1 << 11), (false, true));
        assert_eq!(content_type(0, 0), (false, false));
    }
}
