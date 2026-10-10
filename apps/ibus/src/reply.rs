//! What the engine tells IBus after an event, decided apart from D-Bus so it
//! builds and tests on every platform.

use kanaemi_core::{Candidate, CandidateView, Chord, Key, Mode, Modifiers, Output};

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
    /// The page, the highlighted one, and what the auxiliary text tells of
    /// it, if anything.
    Candidates(Vec<Item>, usize, Option<String>),
    HideCandidates,
    Forward(u32, u32),
    /// The mode to show by the caret for a moment, after it changed.
    Indicator(Mode),
}

/// A candidate as the panel lists it. Only shown: a candidate clicked
/// comes back by its position, and what is committed is the candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub text: String,
    /// Where, in characters, the part after the candidate starts, drawn
    /// greyed where the panel draws attributes.
    pub muted_from: Option<u32>,
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
        Some(view) => Signal::Candidates(items(&view.items), view.selected, aside(view)),
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

/// Between a candidate and what follows it in the panel's text.
const SEPARATOR: char = '　';

/// The panel's text for each candidate: the candidate, then its dictionary
/// or, for a reading to complete with, its first candidate, as the panel
/// has no column for them. IBus hands a clicked candidate back by its
/// position, so candidates that read alike need not be told apart.
fn items(items: &[Candidate]) -> Vec<Item> {
    items
        .iter()
        .map(|c| match c.source.as_ref().or(c.preview.as_ref()) {
            Some(beside) => Item {
                text: format!("{}{SEPARATOR}{beside}", c.surface),
                muted_from: Some(c.surface.chars().count() as u32),
            },
            None => Item {
                text: c.surface.clone(),
                muted_from: None,
            },
        })
        .collect()
}

/// What the auxiliary text tells beside the panel: the page shown of how
/// many, when there are more than one, then a highlighted reading's other
/// candidates, or the highlighted candidate with its dictionary.
fn aside(view: &CandidateView) -> Option<String> {
    let page = (view.pages > 1).then(|| format!("{} / {}", view.page + 1, view.pages));
    let about = if view.more.is_empty() {
        view.items.get(view.selected).and_then(|highlighted| {
            let source = highlighted.source.as_ref()?;
            Some(format!("{}{SEPARATOR}{source}", highlighted.surface))
        })
    } else {
        Some(view.more.join(&SEPARATOR.to_string()))
    };
    match (page, about) {
        (Some(page), Some(about)) => Some(format!("{page}{SEPARATOR}{about}")),
        (page, about) => page.or(about),
    }
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

/// The program an input context's client (`FocusInId`) is, when it says:
/// GTK's input modules append it (`gtk4-im:ghostty`), and XIM or GNOME
/// Shell's own entries tell none.
pub fn program(client: &str) -> Option<&str> {
    match client.split_once(':') {
        Some(("gtk-im" | "gtk3-im" | "gtk4-im", program)) => Some(program),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use kanaemi_core::{Chord, Key, Modifiers};

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
            source: None,
            preview: None,
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

    fn item(text: &str, muted_from: Option<u32>) -> Item {
        Item {
            text: text.to_owned(),
            muted_from,
        }
    }

    fn shown(view: CandidateView) -> Signal {
        let output = Output {
            candidates: Some(view),
            ..output()
        };
        reply(&output, false).signals[1].clone()
    }

    #[test]
    fn a_candidate_shows_its_surface() {
        assert_eq!(
            shown(CandidateView {
                items: vec![candidate("漢字"), candidate("感じ")],
                selected: 1,
                page: 0,
                pages: 1,
                more: Vec::new(),
            }),
            Signal::Candidates(vec![item("漢字", None), item("感じ", None)], 1, None)
        );
    }

    #[test]
    fn a_candidate_is_followed_by_its_dictionary_greyed_and_the_aux_text_tells_it() {
        assert_eq!(
            shown(CandidateView {
                items: vec![
                    Candidate {
                        source: Some("ユーザー辞書".to_owned()),
                        ..candidate("漢字")
                    },
                    candidate("カンジ"),
                ],
                selected: 0,
                page: 0,
                pages: 1,
                more: Vec::new(),
            }),
            Signal::Candidates(
                vec![item("漢字　ユーザー辞書", Some(2)), item("カンジ", None)],
                0,
                Some("漢字　ユーザー辞書".to_owned())
            )
        );
    }

    #[test]
    fn a_reading_to_complete_with_shows_its_first_candidate_after_it_and_the_others_aside() {
        assert_eq!(
            shown(CandidateView {
                items: vec![
                    Candidate {
                        preview: Some("漢字".to_owned()),
                        ..candidate("かんじ")
                    },
                    candidate("かお"),
                ],
                selected: 0,
                page: 0,
                pages: 1,
                more: vec!["感じ".to_owned(), "幹事".to_owned()],
            }),
            Signal::Candidates(
                vec![item("かんじ　漢字", Some(3)), item("かお", None)],
                0,
                Some("感じ　幹事".to_owned())
            )
        );
    }

    #[test]
    fn the_aux_text_tells_the_page_shown_of_how_many_first_when_there_are_more() {
        let view = |source: Option<&str>| CandidateView {
            items: vec![Candidate {
                source: source.map(str::to_owned),
                ..candidate("工")
            }],
            selected: 0,
            page: 1,
            pages: 2,
            more: Vec::new(),
        };
        assert_eq!(
            shown(view(Some("ユーザー辞書"))),
            Signal::Candidates(
                vec![item("工　ユーザー辞書", Some(1))],
                0,
                Some("2 / 2　工　ユーザー辞書".to_owned())
            )
        );
        assert_eq!(
            shown(view(None)),
            Signal::Candidates(vec![item("工", None)], 0, Some("2 / 2".to_owned()))
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

    #[test]
    fn a_gtk_client_names_its_program_and_others_name_none() {
        assert_eq!(program("gtk4-im:ghostty"), Some("ghostty"));
        assert_eq!(
            program("gtk3-im:gnome-terminal-server"),
            Some("gnome-terminal-server")
        );
        assert_eq!(program("gtk-im:firefox"), Some("firefox"));
        assert_eq!(program("xim"), None);
        assert_eq!(program("gnome-shell"), None);
        assert_eq!(program("fake"), None);
    }
}
