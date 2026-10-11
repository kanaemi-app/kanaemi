//! What the add-on tells Fcitx5 for each signal, in the shape Fcitx5 takes
//! it, decided apart from Fcitx5 so it builds and tests on every platform.

use kanaemi_linux::keys;
use kanaemi_linux::reply::{self, Item, Signal};

/// Between a candidate and what follows it in the panel's text.
const SEPARATOR: char = '　';

/// The input context the signals go to, as Fcitx5 is told of them.
pub(crate) trait Host {
    fn commit(&mut self, text: &str);
    /// The preedit, with the cursor in bytes from its start, as Fcitx5
    /// counts it.
    fn preedit(&mut self, text: &str, cursor: usize);
    /// The page, the highlighted one, and the text beside them.
    fn candidates(&mut self, items: &[Candidate<'_>], selected: usize, aside: Option<&str>);
    fn hide_candidates(&mut self);
    fn forward(&mut self, keysym: u32, state: u32);
    /// Deletes the characters just before the caret from the text around
    /// it, as Unicode counts them.
    fn delete_before(&mut self, chars: usize);
    /// The mode's name to show by the caret for a moment.
    fn indicator(&mut self, label: &str);
}

/// How text before the caret is erased in a context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Erasing {
    /// By Backspaces sent to the application.
    ByKeys,
    /// From the text around the caret, where a Backspace sent would carry
    /// the modifiers held down.
    AroundTheCaret,
}

/// A candidate as the panel lists it: the candidate, and its dictionary or
/// the first candidate of a reading to complete with as the comment Fcitx5
/// draws apart from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Candidate<'a> {
    pub text: &'a str,
    pub comment: &'a str,
}

/// Tells `host` each of `signals`, in order.
pub(crate) fn tell(host: &mut impl Host, signals: &[Signal], erasing: Erasing) {
    for signal in signals {
        match signal {
            Signal::Erase(text) => match erasing {
                Erasing::ByKeys => {
                    for (keysym, state) in keys::erasing(text) {
                        host.forward(keysym, state);
                    }
                }
                Erasing::AroundTheCaret => host.delete_before(text.chars().count()),
            },
            Signal::Commit(text) => host.commit(text),
            Signal::Preedit(text, cursor) => host.preedit(text, byte_offset(text, *cursor)),
            Signal::Candidates(items, selected, aside) => {
                let items: Vec<_> = items.iter().map(candidate).collect();
                host.candidates(&items, *selected, aside.as_deref());
            }
            Signal::HideCandidates => host.hide_candidates(),
            Signal::Forward(keysym, state) => host.forward(*keysym, *state),
            Signal::Indicator(mode) => host.indicator(reply::mode_label(*mode)),
        }
    }
}

fn byte_offset(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(index, _)| index)
}

fn candidate(item: &Item) -> Candidate<'_> {
    match item.muted_from {
        Some(from) => {
            let (text, rest) = item.text.split_at(byte_offset(&item.text, from as usize));
            Candidate {
                text,
                comment: rest.trim_start_matches(SEPARATOR),
            }
        }
        None => Candidate {
            text: &item.text,
            comment: "",
        },
    }
}

#[cfg(test)]
mod tests {
    use kanaemi_core::Mode;

    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum Told {
        Commit(String),
        Preedit(String, usize),
        Candidates(Vec<(String, String)>, usize, Option<String>),
        HideCandidates,
        Forward(u32, u32),
        DeleteBefore(usize),
        Indicator(String),
    }

    #[derive(Default)]
    struct Recorder(Vec<Told>);

    impl Host for Recorder {
        fn commit(&mut self, text: &str) {
            self.0.push(Told::Commit(text.to_owned()));
        }
        fn preedit(&mut self, text: &str, cursor: usize) {
            self.0.push(Told::Preedit(text.to_owned(), cursor));
        }
        fn candidates(&mut self, items: &[Candidate<'_>], selected: usize, aside: Option<&str>) {
            let items = items
                .iter()
                .map(|c| (c.text.to_owned(), c.comment.to_owned()))
                .collect();
            self.0
                .push(Told::Candidates(items, selected, aside.map(str::to_owned)));
        }
        fn hide_candidates(&mut self) {
            self.0.push(Told::HideCandidates);
        }
        fn forward(&mut self, keysym: u32, state: u32) {
            self.0.push(Told::Forward(keysym, state));
        }
        fn delete_before(&mut self, chars: usize) {
            self.0.push(Told::DeleteBefore(chars));
        }
        fn indicator(&mut self, label: &str) {
            self.0.push(Told::Indicator(label.to_owned()));
        }
    }

    fn told(signals: &[Signal]) -> Vec<Told> {
        told_in(signals, Erasing::ByKeys)
    }

    fn told_in(signals: &[Signal], erasing: Erasing) -> Vec<Told> {
        let mut recorder = Recorder::default();
        tell(&mut recorder, signals, erasing);
        recorder.0
    }

    #[test]
    fn text_is_erased_by_backspaces_or_from_the_text_around_the_caret() {
        let erase = [Signal::Erase("記者𥸮".to_owned())];
        let backspace = || Told::Forward(0xff08, 0);
        assert_eq!(told(&erase), [backspace(), backspace(), backspace()]);
        assert_eq!(
            told_in(&erase, Erasing::AroundTheCaret),
            [Told::DeleteBefore(3)]
        );
    }

    #[test]
    fn the_preedit_cursor_counts_bytes() {
        assert_eq!(
            told(&[Signal::Preedit("›かな".to_owned(), 2)]),
            [Told::Preedit("›かな".to_owned(), 6)]
        );
        assert_eq!(
            told(&[Signal::Preedit("かな".to_owned(), 2)]),
            [Told::Preedit("かな".to_owned(), 6)],
            "at the end"
        );
    }

    #[test]
    fn what_follows_a_candidate_is_its_comment() {
        let items = vec![
            Item {
                text: "漢字　ユーザー辞書".to_owned(),
                muted_from: Some(2),
            },
            Item {
                text: "感じ".to_owned(),
                muted_from: None,
            },
        ];
        assert_eq!(
            told(&[Signal::Candidates(items, 1, Some("1 / 2".to_owned()))]),
            [Told::Candidates(
                vec![
                    ("漢字".to_owned(), "ユーザー辞書".to_owned()),
                    ("感じ".to_owned(), String::new()),
                ],
                1,
                Some("1 / 2".to_owned())
            )]
        );
    }

    #[test]
    fn the_signals_are_told_in_order() {
        assert_eq!(
            told(&[
                Signal::Forward(0xff08, 0),
                Signal::Commit("漢字".to_owned()),
                Signal::HideCandidates,
                Signal::Indicator(Mode::Abc),
            ]),
            [
                Told::Forward(0xff08, 0),
                Told::Commit("漢字".to_owned()),
                Told::HideCandidates,
                Told::Indicator("ABC".to_owned()),
            ]
        );
    }
}
