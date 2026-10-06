//! How often the user picked each candidate, across fields and sessions: a
//! candidate picked again and again comes first, while a single pick changes
//! nothing. Only a weight per (reading, surface) is kept, not when anything
//! was picked, so the record does not tell what was typed when.

use std::collections::HashMap;

use crate::text_dictionary::{
    escape_literal_reading, escape_surface, unescape_literal_reading, unescape_surface,
};

/// After this many picks of anything, an old pick counts half.
const HALF_LIFE: f64 = 1000.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selections {
    /// Each pair's weight as of the pick at its tick.
    pairs: HashMap<(String, String), (f64, u64)>,
    /// How many picks were recorded since the record was read.
    tick: u64,
}

impl Selections {
    /// The weight from which a candidate comes first: three recent picks.
    pub(crate) const FAVORITE: f64 = 2.5;
    /// How many pairs the record keeps, the heaviest.
    const MAX_PAIRS: usize = 5000;

    pub(crate) fn record(&mut self, reading: &str, surface: &str) {
        self.tick += 1;
        let key = (reading.to_owned(), surface.to_owned());
        let weight = self.weight_of(self.pairs.get(&key));
        self.pairs.insert(key, (weight + 1.0, self.tick));
    }

    /// Takes back a pick recorded, as never made. The picks since it faded
    /// it a little; a pick taken back at once takes back all of it.
    pub(crate) fn withdraw(&mut self, reading: &str, surface: &str) {
        let key = (reading.to_owned(), surface.to_owned());
        let weight = self.weight_of(self.pairs.get(&key)) - 1.0;
        if weight > 0.0 {
            self.pairs.insert(key, (weight, self.tick));
        } else {
            self.pairs.remove(&key);
        }
    }

    /// The pair's weight now, after the picks since it faded it.
    pub(crate) fn weight(&self, reading: &str, surface: &str) -> f64 {
        self.weight_of(self.pairs.get(&(reading.to_owned(), surface.to_owned())))
    }

    fn weight_of(&self, pair: Option<&(f64, u64)>) -> f64 {
        pair.map_or(0.0, |&(weight, at)| {
            weight * 0.5f64.powf((self.tick - at) as f64 / HALF_LIFE)
        })
    }

    /// One line per pair: reading, surface and weight as of now, heaviest
    /// first, escaped as a text dictionary escapes a word's reading and
    /// surface, so a reading starting with `#` is not taken for a comment.
    pub fn parse(text: impl AsRef<str>) -> Self {
        let pairs = text
            .as_ref()
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let reading = unescape_literal_reading(fields.next()?).ok()?;
                let surface = unescape_surface(fields.next()?).ok()?;
                let weight: f64 = fields.next()?.parse().ok()?;
                (fields.next().is_none() && weight.is_finite() && weight > 0.0)
                    .then_some(((reading, surface), (weight, 0)))
            })
            .collect();
        Self { pairs, tick: 0 }
    }

    pub fn to_text(&self) -> String {
        let mut now: Vec<(&(String, String), f64)> = self
            .pairs
            .iter()
            .map(|(key, pair)| (key, self.weight_of(Some(pair))))
            .collect();
        now.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let mut out = String::from("# Kanaemi's record of how often each candidate was picked\n");
        for ((reading, surface), weight) in now.into_iter().take(Self::MAX_PAIRS) {
            out.push_str(&format!(
                "{}\t{}\t{weight:.3}\n",
                escape_literal_reading(reading),
                escape_surface(surface)
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placeholder::{CLOSE, OPEN};

    #[test]
    fn the_record_reads_back_without_when_anything_was_picked() {
        let mut picked = Selections::default();
        picked.record("きしゃ", "貴社");
        picked.record("きしゃ", "貴社");
        picked.record("き\tしゃ", "x\\y");
        let text = picked.to_text();
        assert!(!text.contains("\n\n"));
        let read = Selections::parse(&text);
        assert_eq!(read.to_text(), text);
        assert!((read.weight("きしゃ", "貴社") - picked.weight("きしゃ", "貴社")).abs() < 0.01);
        assert!(read.weight("き\tしゃ", "x\\y") > 0.9);
    }

    #[test]
    fn a_reading_that_starts_like_a_comment_or_a_hide_line_reads_back() {
        let mut picked = Selections::default();
        picked.record("#きしゃ", "貴社");
        picked.record("!きしゃ", "#記者");
        let text = picked.to_text();
        assert!(text.contains("\n\\#きしゃ\t貴社\t"), "{text}");
        assert!(text.contains("\n\\!きしゃ\t#記者\t"), "{text}");
        let read = Selections::parse(&text);
        assert!(read.weight("#きしゃ", "貴社") > 0.9);
        assert!(read.weight("!きしゃ", "#記者") > 0.9);
        assert_eq!(read.to_text(), text);
    }

    #[test]
    fn the_record_reads_the_escapes_of_a_text_dictionary_reading() {
        let read = Selections::parse("\\!き\\*しゃ\t貴社\t2.5\n\\#き\\\\しゃ\t記者\t2.5\n");
        assert!((read.weight("!き*しゃ", "貴社") - 2.5).abs() < 0.01);
        assert!((read.weight("#き\\しゃ", "記者") - 2.5).abs() < 0.01);
    }

    #[test]
    fn a_numeric_pair_keeps_its_placeholders_apart_from_literal_braces() {
        let reading = format!("{OPEN}{CLOSE}こ{{");
        let surface = format!("{OPEN}kanji{CLOSE}個{{");
        let mut picked = Selections::default();
        picked.record(&reading, &surface);
        let text = picked.to_text();
        assert!(text.contains("\n{}こ\\{\t{kanji}個\\{\t"), "{text}");
        assert!(Selections::parse(&text).weight(&reading, &surface) > 0.9);
    }

    #[test]
    fn a_broken_line_of_the_record_is_skipped() {
        let read = Selections::parse("# comment\nきしゃ\t貴社\t2.5\nbroken\nきしゃ\t記者\tx\n");
        assert!((read.weight("きしゃ", "貴社") - 2.5).abs() < 0.01);
        assert_eq!(read.weight("きしゃ", "記者"), 0.0);
    }

    #[test]
    fn the_record_keeps_the_heaviest_pairs_within_its_limit() {
        let mut picked = Selections::default();
        for i in 0..(Selections::MAX_PAIRS + 10) {
            picked.record(&format!("よみ{i}"), "x");
        }
        picked.record("よみ0", "x");
        let read = Selections::parse(picked.to_text());
        assert_eq!(read.pairs.len(), Selections::MAX_PAIRS);
        assert!(read.weight("よみ0", "x") > 1.0);
    }
}
