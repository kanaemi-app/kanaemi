use kanaemi_core::{Candidate, Converter, Effect};

use crate::numeric::{Numbers, fill, typed_placeholders};
use crate::{
    CONTEXT_CHARS, CandidateFacts, ConjugationTable, Dictionary, HISTORY_LEN, ItemLine, LineSink,
    MAX_SUFFIX_KANA, RankingInput, RankingModel, Selections, TextDictionary, UserCustom,
    WriteError, nfc, okuri_row,
};

/// On a frequency scale (100 per factor of e) this ranks a built form after
/// listed words about 150 times rarer: a dictionary lists the forms it sees
/// often, so a smaller value lets rare built forms win, and a larger one
/// changes almost nothing.
const CONJUGATED_COST: u32 = 500;

/// One place in the ordered dictionary list.
pub enum Slot {
    UserCustom,
    Dictionary(Box<dyn Dictionary>),
}

/// What the engine knows of the field with the focus. It ends when the focus
/// moves.
#[derive(Default)]
struct FieldSession {
    /// Oldest first, at most [`HISTORY_LEN`].
    history: Vec<(String, String)>,
    /// The end of the text committed to the field.
    context: String,
}

impl FieldSession {
    fn commit(&mut self, reading: String, surface: String) {
        self.history.push((reading, surface));
        if self.history.len() > HISTORY_LEN {
            self.history.remove(0);
        }
    }

    fn type_text(&mut self, text: &str) {
        self.context.push_str(text);
        let excess = self.context.chars().count().saturating_sub(CONTEXT_CHARS);
        if let Some((at, _)) = self.context.char_indices().nth(excess) {
            self.context.drain(..at);
        }
    }

    /// The surface last committed for `reading`.
    fn last(&self, reading: &str) -> Option<&str> {
        self.history
            .iter()
            .rev()
            .find(|(r, _)| r == reading)
            .map(|(_, s)| s.as_str())
    }

    fn input<'a>(&'a self, reading: &'a str) -> RankingInput<'a> {
        RankingInput {
            reading,
            history: &self.history,
            context: &self.context,
        }
    }
}

pub struct Engine {
    slots: Vec<Slot>,
    user: UserCustom,
    field: FieldSession,
    model: Option<RankingModel>,
    selections: Selections,
    /// Whether `selections` changed since it was last handed over.
    selections_changed: bool,
}

/// A candidate as it is gathered: what ranking knows of it.
struct Found {
    facts: CandidateFacts,
    /// The reading and surface of the numeric item it was filled from.
    numeric: Option<(String, String)>,
    /// Split where the user marked the okurigana: made from an okurigana
    /// word, or from a conjugating stem reaching the mark.
    okuri: bool,
}

impl Found {
    /// The (reading, surface) the history, the picks and hide lines know it
    /// by: a numeric item's own, so what is learned with one number holds
    /// for every number.
    fn recorded<'a>(&'a self, reading: &'a str) -> (&'a str, &'a str) {
        match &self.numeric {
            Some((reading, surface)) => (reading, surface),
            None => (reading, &self.facts.surface),
        }
    }
}

impl Engine {
    /// The user custom dictionary goes first when `slots` does not place it.
    pub fn new(
        slots: impl IntoIterator<Item = Slot>,
        user: TextDictionary,
        sink: impl LineSink + 'static,
    ) -> Self {
        let mut slots: Vec<Slot> = slots.into_iter().collect();
        if !slots.iter().any(|s| matches!(s, Slot::UserCustom)) {
            slots.insert(0, Slot::UserCustom);
        }
        Self {
            slots,
            user: UserCustom::new(user, Box::new(sink)),
            field: FieldSession::default(),
            model: None,
            selections: Selections::default(),
            selections_changed: false,
        }
    }

    /// Ranks with `model` from now on; without one, by the rules alone.
    pub fn set_model(&mut self, model: Option<RankingModel>) {
        self.model = model;
    }

    /// Puts the record of picks read from where it is kept in place.
    pub fn replace_selections(&mut self, selections: Selections) {
        self.selections = selections;
        self.selections_changed = false;
    }

    /// The record of picks to keep, when it changed since the last call.
    pub fn take_selections(&mut self) -> Option<Selections> {
        std::mem::take(&mut self.selections_changed).then(|| self.selections.clone())
    }

    /// Writes that failed since the last call. Their effect was kept in memory.
    pub fn take_write_errors(&mut self) -> Vec<WriteError> {
        self.user.take_errors()
    }

    /// Puts the user custom dictionary read again in place, with the lines not
    /// yet written on top.
    pub fn replace_user(&mut self, user: TextDictionary) {
        self.user.replace(user);
    }

    /// Carries what the field taught and the lines not yet written over from
    /// the engine this one replaces. The model stays this engine's own.
    pub fn take_over(&mut self, previous: Engine) {
        self.field = previous.field;
        self.user.absorb(previous.user);
        self.selections = previous.selections;
        self.selections_changed = previous.selections_changed;
    }

    /// Learns what the core reports happened, in its order.
    pub fn learn(&mut self, effect: &Effect) {
        match effect {
            Effect::Committed {
                reading,
                okurigana,
                surface,
            } => {
                let (reading, surface) =
                    self.recorded(nfc(reading), okurigana.is_some(), nfc(surface));
                self.selections.record(&reading, &surface);
                self.selections_changed = true;
                self.field.commit(reading, surface);
            }
            Effect::Registered {
                reading,
                okurigana,
                surface,
            } => self.register(&nfc(reading), okurigana.as_deref().map(nfc), &nfc(surface)),
            Effect::Forgotten {
                reading,
                okurigana,
                surface,
            } => {
                let (reading, surface) =
                    self.recorded(nfc(reading), okurigana.is_some(), nfc(surface));
                self.user
                    .write(TextDictionary::hide_line(&reading, &surface));
            }
            Effect::Typed(text) => self.field.type_text(&nfc(text)),
            Effect::FocusMoved => self.field = FieldSession::default(),
        }
    }

    /// The pair a candidate the core reports is recorded as, found again as
    /// it was converted. Converting with okurigana fills no numeric item.
    fn recorded(&self, reading: String, okurigana: bool, surface: String) -> (String, String) {
        if okurigana || Numbers::find(&reading).is_none() {
            return (reading, surface);
        }
        self.found(&reading, None)
            .into_iter()
            .find(|f| f.facts.surface == surface)
            .and_then(|f| f.numeric)
            .unwrap_or((reading, surface))
    }

    fn register(&mut self, reading: &str, okurigana: Option<String>, surface: &str) {
        if let Some((numbers, surface)) =
            numeric_registration(reading, okurigana.as_deref(), surface)
        {
            let line = ItemLine {
                reading: &numbers.reading,
                surface: &surface,
                ..ItemLine::default()
            };
            self.user.write(line.to_string());
            return;
        }
        // The text format marks okurigana of one kana with an okurigana row
        // only; any other (きゃ, ー) is kept as a plain word rather than lost.
        let markable = |kana: &str| {
            let mut chars = kana.chars();
            matches!((chars.next(), chars.next()), (Some(c), None) if okuri_row(c).is_some())
        };
        let (reading, okurigana) = match okurigana {
            Some(kana) if markable(&kana) => (reading.to_owned(), Some(kana)),
            Some(kana) => (format!("{reading}{kana}"), None),
            None => (reading.to_owned(), None),
        };
        let line = ItemLine {
            reading: &reading,
            okurigana: okurigana.as_deref(),
            surface,
            ..ItemLine::default()
        };
        self.user.write(line.to_string());
    }

    fn found(&self, reading: &str, okurigana: Option<&str>) -> Vec<Found> {
        let mut all: Vec<Found> = Vec::new();
        let user = self.user.dictionary();
        let numbers = match okurigana {
            Some(_) => None,
            None => Numbers::find(reading),
        };
        for (priority, slot) in self.slots.iter().enumerate() {
            let dictionary: &dyn Dictionary = match slot {
                Slot::UserCustom => user,
                Slot::Dictionary(d) => d.as_ref(),
            };
            let hidden = |reading: &str, surface: &str| user.is_hidden(reading, surface);
            for mut f in found_in(dictionary, reading, okurigana, numbers.as_ref(), &hidden) {
                match all.iter_mut().find(|a| a.facts.surface == f.facts.surface) {
                    Some(a) => a.okuri |= f.okuri,
                    None => {
                        f.facts.dictionary = priority;
                        all.push(f);
                    }
                }
            }
        }
        all
    }

    /// Sorts by the model's score; ties, and every candidate without a model,
    /// go by the rules.
    fn rank(&self, reading: &str, mut all: Vec<Found>) -> Vec<Found> {
        fn key(f: &Found) -> (usize, u32, &[u8]) {
            (f.facts.dictionary, f.facts.cost, f.facts.surface.as_bytes())
        }
        let tie = |a: &Found, b: &Found| key(a).cmp(&key(b));
        if let Some(model) = &self.model {
            let input = self.field.input(reading);
            let mut scored: Vec<(f32, Found)> = all
                .into_iter()
                .map(|f| (model.score_candidate(&input, &f.facts), f))
                .collect();
            scored.sort_by(|(sa, a), (sb, b)| sb.total_cmp(sa).then_with(|| tie(a, b)));
            return scored.into_iter().map(|(_, f)| f).collect();
        }
        let group = |f: &Found| {
            let (reading, surface) = f.recorded(reading);
            if self.field.last(reading) == Some(surface) {
                0
            } else if self.field.history.iter().any(|(_, s)| s == surface) {
                1
            } else {
                2
            }
        };
        all.sort_by(|a, b| group(a).cmp(&group(b)).then_with(|| tie(a, b)));
        all
    }
}

impl Engine {
    /// Brings candidates split at the okurigana mark to the front: the mark
    /// says how the reading splits, which a word of the whole reading (立ちゃ
    /// for たち*ゃ) may not. Once the reading has a commit in
    /// this field, the field's history decides instead.
    fn okuri_first(&self, reading: &str, ranked: Vec<Found>) -> Vec<Found> {
        if self.field.last(reading).is_some() {
            return ranked;
        }
        let (mut okuri, rest): (Vec<_>, Vec<_>) = ranked.into_iter().partition(|f| f.okuri);
        okuri.extend(rest);
        okuri
    }

    /// Brings candidates picked again and again to the front, heaviest first.
    /// Once the reading, or a numeric item's reading for it, has a commit in
    /// this field, the field's history decides instead: a choice just made
    /// there is not overturned.
    fn favor(&self, reading: &str, ranked: Vec<Found>) -> Vec<Found> {
        let committed = |reading: &str| self.field.last(reading).is_some();
        if committed(reading) || ranked.iter().any(|f| committed(f.recorded(reading).0)) {
            return ranked;
        }
        let mut weighed: Vec<(f64, Found)> = ranked
            .into_iter()
            .map(|f| {
                let (recorded_reading, surface) = f.recorded(reading);
                let weight = self.selections.weight(recorded_reading, surface);
                (
                    if weight >= Selections::FAVORITE {
                        weight
                    } else {
                        0.0
                    },
                    f,
                )
            })
            .collect();
        // Stable, so the rest keeps the order it was ranked in.
        weighed.sort_by(|a, b| b.0.total_cmp(&a.0));
        weighed.into_iter().map(|(_, f)| f).collect()
    }
}

impl Converter for Engine {
    fn convert(&self, reading: &str, okurigana: Option<&str>) -> Vec<Candidate> {
        let reading = &nfc(reading);
        let okurigana = okurigana.map(nfc);
        let ranked = self.rank(reading, self.found(reading, okurigana.as_deref()));
        let ranked = self.okuri_first(reading, ranked);
        self.favor(reading, ranked)
            .into_iter()
            .map(|f| Candidate {
                surface: f.facts.surface,
            })
            .collect()
    }

    fn registered_text(&self, reading: &str, okurigana: Option<&str>, surface: &str) -> String {
        numeric_registration(&nfc(reading), okurigana, surface)
            .and_then(|(numbers, surface)| fill(&surface, &numbers.values))
            .unwrap_or_else(|| surface.to_owned())
    }
}

/// The reading's numbers and the numeric item's surface, when a word
/// registered for `reading` has placeholders and so is written as one.
fn numeric_registration(
    reading: &str,
    okurigana: Option<&str>,
    surface: &str,
) -> Option<(Numbers, String)> {
    if okurigana.is_some() {
        return None;
    }
    Some((Numbers::find(reading)?, typed_placeholders(surface)?))
}

/// The candidates one dictionary gives for a reading, and for its `numbers`
/// put in numeric items, merged by surface at the cheapest cost. An item
/// whose recorded pair is `hidden` is left out before merging, so it takes no
/// other item of its surface with it.
fn found_in(
    dictionary: &dyn Dictionary,
    reading: &str,
    okurigana: Option<&str>,
    numbers: Option<&Numbers>,
    hidden: &dyn Fn(&str, &str) -> bool,
) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    let mut add = |surface: String,
                   cost: u32,
                   built: bool,
                   numeric: Option<(String, String)>,
                   okuri: bool| {
        let (recorded_reading, recorded_surface) = match &numeric {
            Some((reading, surface)) => (reading.as_str(), surface.as_str()),
            None => (reading, surface.as_str()),
        };
        if hidden(recorded_reading, recorded_surface) {
            return;
        }
        match found.iter_mut().find(|f| f.facts.surface == surface) {
            Some(f) => {
                f.okuri |= okuri;
                if cost < f.facts.cost {
                    f.facts.cost = cost;
                    f.facts.built = built;
                    f.numeric = numeric;
                }
            }
            None => found.push(Found {
                facts: CandidateFacts {
                    surface,
                    dictionary: 0,
                    cost,
                    built,
                },
                numeric,
                okuri,
            }),
        }
    };
    // One lookup for each place the reading may split: the whole reading's
    // words come first, then each split's conjugating stems.
    let splits: Vec<(&str, Vec<_>)> = stem_splits(reading, okurigana)
        .into_iter()
        .map(|(stem, rest)| (rest, dictionary.lookup(stem)))
        .collect();
    let whole = match splits.last() {
        Some(("", entries)) => entries.clone(),
        _ => dictionary.lookup(reading),
    };
    for e in whole.into_iter().filter(|e| e.conjugation.is_none()) {
        add(e.surface, e.cost, false, None, false);
    }
    let table = ConjugationTable::builtin();
    for (rest, entries) in splits {
        for e in entries {
            let Some(conjugation) = &e.conjugation else {
                continue;
            };
            let allowed = match okurigana {
                Some(_) => table.prefixes(conjugation),
                None => table.suffixes(conjugation),
            };
            if allowed.is_some_and(|set| set.contains(rest)) {
                add(
                    format!("{}{rest}", e.surface),
                    e.cost.saturating_add(CONJUGATED_COST),
                    true,
                    None,
                    // With okurigana, the stem reaches the mark.
                    okurigana.is_some(),
                );
            }
        }
    }
    if let Some(okurigana) = okurigana
        && let Some(stem) = reading.strip_suffix(okurigana)
    {
        for o in okurigana
            .chars()
            .next()
            .and_then(okuri_row)
            .map(|row| dictionary.okuri(stem, row))
            .unwrap_or_default()
        {
            add(
                format!("{}{okurigana}", o.surface),
                o.cost,
                false,
                None,
                true,
            );
        }
    }
    if let Some(numbers) = numbers {
        let items = dictionary.lookup(&numbers.reading);
        for e in items.into_iter().filter(|e| e.conjugation.is_none()) {
            if let Some(surface) = fill(&e.surface, &numbers.values) {
                let numeric = (numbers.reading.clone(), e.surface);
                add(surface, e.cost, false, Some(numeric), false);
            }
        }
    }
    if let Some(okurigana) = okurigana {
        found.retain(|f| f.facts.surface.ends_with(okurigana));
    }
    found
}

/// Where a reading may split into a conjugating stem and what follows it. With
/// okurigana the stem reaches at least to where the user marked it: an ichidan
/// stem takes the okurigana's kana too (食べ for TaBe).
fn stem_splits<'a>(reading: &'a str, okurigana: Option<&str>) -> Vec<(&'a str, &'a str)> {
    let shortest = match okurigana {
        Some(okurigana) => match reading.strip_suffix(okurigana) {
            Some(stem) => stem.len(),
            None => return Vec::new(),
        },
        None => 0,
    };
    reading
        .char_indices()
        .map(|(i, _)| i)
        .chain([reading.len()])
        .filter(|&i| i > 0 && i >= shortest)
        .filter(|&i| reading[i..].chars().count() <= MAX_SUFFIX_KANA)
        .map(|at| (&reading[..at], &reading[at..]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Discard;

    #[test]
    fn each_candidate_carries_what_the_model_ranks_it_by() {
        let dictionary = TextDictionary::parse("か\t書\t五段-カ行\t100\nかく\t核\t\t300\n").0;
        let engine = Engine::new(
            [Slot::UserCustom, Slot::Dictionary(Box::new(dictionary))],
            TextDictionary::parse_user_custom("").0,
            Discard,
        );
        let found = engine.found("かく", None);
        let book = &found
            .iter()
            .find(|f| f.facts.surface == "書く")
            .unwrap()
            .facts;
        assert_eq!((book.dictionary, book.cost, book.built), (1, 600, true));
        let core = &found
            .iter()
            .find(|f| f.facts.surface == "核")
            .unwrap()
            .facts;
        assert_eq!((core.dictionary, core.cost, core.built), (1, 300, false));
    }
}
