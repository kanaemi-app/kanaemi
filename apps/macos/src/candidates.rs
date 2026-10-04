//! The strings the candidate panel shows. The panel hands a clicked
//! candidate back only as its string, so every string on a page is made
//! unique to tell which candidate was clicked.

use kanaemi_core::Candidate;

/// Added to a string already on the page until it is not; it shows nothing.
const DISTINCT: char = '\u{200B}';

/// The panel's string for each candidate, in order.
pub fn labels(items: &[Candidate]) -> Vec<String> {
    let mut shown: Vec<String> = Vec::with_capacity(items.len());
    for c in items {
        let mut label = c.surface.clone();
        while shown.contains(&label) {
            label.push(DISTINCT);
        }
        shown.push(label);
    }
    shown
}

/// The position of the clicked `label` among `labels`.
pub fn position(labels: &[String], label: &str) -> Option<usize> {
    labels.iter().position(|l| l == label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(surface: &str) -> Candidate {
        Candidate {
            surface: surface.to_owned(),
        }
    }

    #[test]
    fn a_candidate_shows_its_surface() {
        assert_eq!(
            labels(&[candidate("漢字"), candidate("感じ")]),
            ["漢字", "感じ"]
        );
    }

    #[test]
    fn candidates_that_would_show_alike_are_told_apart() {
        let shown = labels(&[
            candidate("漢字"),
            candidate("漢字\u{200B}"),
            candidate("漢字"),
        ]);
        for (index, label) in shown.iter().enumerate() {
            assert_eq!(position(&shown, label), Some(index), "{label:?}");
        }
    }

    #[test]
    fn a_string_not_shown_is_no_candidate() {
        assert_eq!(position(&labels(&[candidate("漢字")]), "感じ"), None);
    }
}
