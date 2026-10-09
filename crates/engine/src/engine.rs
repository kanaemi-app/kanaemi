use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use kanaemi_core::{Candidate, Converter, Effect};

use crate::numeric::Numbers;
use crate::placeholder::{Functions, OPEN, fill, fits};
use crate::{
    CONTEXT_CHARS, CandidateFacts, ConjugationTable, Dictionary, HISTORY_LEN, ItemLine, LineSink,
    MAX_SUFFIX_KANA, RankingInput, RankingModel, Selections, TextDictionary, UserCustom,
    WriteError, mark_placeholders, nfc, okuri_lookup, okuri_row,
};

/// On a frequency scale (100 per factor of e) this ranks a built form after
/// listed words about 150 times rarer: a dictionary lists the forms it sees
/// often, so a smaller value lets rare built forms win, and a larger one
/// changes almost nothing.
const CONJUGATED_COST: u32 = 500;

/// How many readings a reading is completed with at most.
const MAX_COMPLETIONS: usize = 100;

/// How many readings going on from the one typed are looked at in each
/// dictionary, in its order: a short reading starts tens of thousands in a
/// large dictionary, too many to look up while a key waits.
const COMPLETIONS_LOOKED_AT: usize = 5000;

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

    /// Drops the last commit of the pair, as never made.
    fn withdraw(&mut self, reading: &str, surface: &str) {
        if let Some(at) = self
            .history
            .iter()
            .rposition(|(r, s)| r == reading && s == surface)
        {
            self.history.remove(at);
        }
    }

    /// Takes `text` off the end of the context; a context that does not end
    /// with it is no longer known, and starts over.
    fn erase(&mut self, text: &str) {
        match self.context.strip_suffix(text) {
            Some(rest) => self.context.truncate(rest.len()),
            None => self.context.clear(),
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
    model: Option<Arc<RankingModel>>,
    selections: Selections,
    /// Whether `selections` changed since it was last handed over.
    selections_changed: bool,
    functions: Option<Rc<dyn Functions>>,
    /// Set while a key is only tried, so the user's functions, which may
    /// count their calls, run once for each key.
    without_functions: Cell<bool>,
    /// The last conversion without okurigana, so a candidate is recorded by
    /// the item it was filled from even when its function gives another
    /// text each time.
    last: RefCell<Option<(String, Vec<CandidateFacts>)>>,
    /// The last commit as the core reported it, and the pair it was recorded
    /// as, so undoing it withdraws that pair whatever was converted since.
    committed: Option<((String, String), (String, String))>,
}

/// A candidate as it is gathered: what ranking knows of it.
struct Found {
    facts: CandidateFacts,
    /// Split where the user marked the okurigana: made from an okurigana
    /// word, or from a conjugating stem reaching the mark.
    okuri: bool,
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
            functions: None,
            without_functions: Cell::new(false),
            last: RefCell::new(None),
            committed: None,
        }
    }

    /// Ranks with `model` from now on; without one, by the rules alone. The
    /// engines of one process may share it.
    pub fn set_model(&mut self, model: Option<Arc<RankingModel>>) {
        self.model = model;
    }

    /// Fills placeholders with `functions` from now on, before the built-in
    /// ones. The engines of one process may share them.
    pub fn set_functions(&mut self, functions: Option<Rc<dyn Functions>>) {
        self.functions = functions;
        self.last.replace(None);
    }

    /// Runs `run` with only the built-in functions filling placeholders, and
    /// keeps no conversion for recording, as for a key that is only tried.
    pub fn without_functions<T>(&self, run: impl FnOnce() -> T) -> T {
        let before = self.without_functions.replace(true);
        let result = run();
        self.without_functions.set(before);
        result
    }

    /// Keeps `facts` of a word registered for `reading` as converted, before
    /// what was converted for it, so its commit is recorded as registered
    /// without running the functions again.
    fn remember(&self, reading: &str, facts: CandidateFacts) {
        if self.without_functions.get() {
            return;
        }
        let mut last = self.last.borrow_mut();
        match last.as_mut() {
            Some((converted, all)) if converted == reading => all.insert(0, facts),
            _ => *last = Some((reading.to_owned(), vec![facts])),
        }
    }

    /// The user's functions, unless a key is only tried.
    fn user_functions(&self) -> Option<&dyn Functions> {
        self.functions
            .as_deref()
            .filter(|_| !self.without_functions.get())
    }

    /// Every candidate with what the ranking model knows of it, in no order:
    /// what training ranks.
    pub fn candidate_facts(
        &self,
        reading: impl AsRef<str>,
        okurigana: Option<&str>,
    ) -> Vec<CandidateFacts> {
        let okurigana = okurigana.map(nfc);
        self.found(&nfc(reading.as_ref()), okurigana.as_deref())
            .into_iter()
            .map(|f| f.facts)
            .collect()
    }

    /// Puts the record of picks read from where it is kept in place.
    pub fn replace_selections(&mut self, selections: Selections) {
        self.selections = selections;
        self.selections_changed = false;
    }

    /// Puts a record of picks written elsewhere since this engine's was read
    /// in place, with the picks not yet written from here made on it.
    pub fn merge_selections(&mut self, selections: Selections) {
        self.selections = self.selections.merged_into(selections);
        self.selections_changed = self.selections.has_unwritten();
    }

    /// Tells that `written`, a record handed over since this engine's was
    /// last put in place, was written, so its picks are not made again on a
    /// record merged later. Picks made since it was handed over still are.
    pub fn selections_written(&mut self, written: &Selections) {
        self.selections.mark_written(written);
    }

    /// The record of picks to keep, when it changed since the last call.
    pub fn take_selections(&mut self) -> Option<Selections> {
        std::mem::take(&mut self.selections_changed).then(|| self.selections.clone())
    }

    /// Writes that failed since the last call. Their effect was kept in memory.
    pub fn take_write_errors(&mut self) -> Vec<WriteError> {
        self.user.take_errors()
    }

    /// The user custom dictionary as it is in memory, for a host that cannot
    /// read it back from where its lines went.
    pub fn user_dictionary(&self) -> &TextDictionary {
        self.user.dictionary()
    }

    /// Puts the user custom dictionary read again in place, with the lines not
    /// yet written on top.
    pub fn replace_user(&mut self, user: TextDictionary) {
        self.user.replace(user);
        self.last.replace(None);
    }

    /// Carries what the field taught, the lines not yet written, and the
    /// last conversion and commit over from the engine this one replaces, so
    /// a candidate shown or committed before is still recorded, and its
    /// commit undone, by the item it was filled from. The model and the
    /// functions stay this engine's own.
    pub fn take_over(&mut self, previous: Engine) {
        self.field = previous.field;
        self.user.absorb(previous.user);
        self.selections = previous.selections;
        self.selections_changed = previous.selections_changed;
        self.last = previous.last;
        self.committed = previous.committed;
    }

    /// Learns what the core reports happened, in its order.
    pub fn learn(&mut self, effect: &Effect) {
        // A word is kept for its reading: a candidate of letters that made
        // no kana (`pdf`) has none, and a line without one cannot be read
        // back.
        if let Effect::Committed { reading, .. }
        | Effect::Registered { reading, .. }
        | Effect::Forgotten { reading, .. }
        | Effect::Withdrawn { reading, .. } = effect
            && reading.is_empty()
        {
            return;
        }
        match effect {
            Effect::Committed {
                reading,
                okurigana,
                surface,
            } => {
                let reported = (nfc(reading), nfc(surface));
                let (reading, surface) =
                    self.recorded(reported.0.clone(), okurigana.is_some(), reported.1.clone());
                self.committed = Some((reported, (reading.clone(), surface.clone())));
                self.selections.record(&reading, &surface);
                self.selections_changed = true;
                self.field.commit(reading, surface);
            }
            Effect::Registered {
                reading,
                okurigana,
                okurigana_head,
                surface,
            } => self.register(
                &nfc(reading),
                okurigana.as_deref().map(nfc),
                okurigana_head.as_deref().map(nfc),
                &nfc(surface),
            ),
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
            Effect::Withdrawn {
                reading,
                okurigana,
                surface,
            } => {
                let reported = (nfc(reading), nfc(surface));
                let (reading, surface) = match self.committed.take() {
                    Some((committed, recorded)) if committed == reported => recorded,
                    _ => self.recorded(reported.0, okurigana.is_some(), reported.1),
                };
                self.selections.withdraw(&reading, &surface);
                self.selections_changed = true;
                self.field.withdraw(&reading, &surface);
            }
            Effect::Typed(text) => self.field.type_text(&nfc(text)),
            Effect::Erased(text) => self.field.erase(&nfc(text)),
            Effect::FocusMoved => {
                self.field = FieldSession::default();
                // A line the sink could not take, as when the file was
                // locked elsewhere, is not left waiting for the next one.
                self.user.flush();
            }
        }
    }

    /// The pair a candidate the core reports is recorded as, found again as
    /// it was converted. Converting with okurigana fills no placeholder.
    fn recorded(&self, reading: String, okurigana: bool, surface: String) -> (String, String) {
        if okurigana {
            return (reading, surface);
        }
        // The candidate, and the item it was filled from if any.
        let template = |all: &[CandidateFacts]| {
            all.iter()
                .find(|f| f.surface == surface)
                .map(|f| f.template.clone())
        };
        let last = match self.last.borrow().as_ref() {
            Some((converted, all)) if *converted == reading => template(all),
            _ => None,
        };
        // Found again only when that conversion did not give it, such as a
        // word registered since: running the functions again may give
        // another item this text.
        last.or_else(|| {
            template(
                &self
                    .found(&reading, None)
                    .into_iter()
                    .map(|f| f.facts)
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .unwrap_or((reading, surface))
    }

    fn register(
        &mut self,
        reading: &str,
        okurigana: Option<String>,
        head: Option<String>,
        surface: &str,
    ) {
        if let Some(registration) = placeholder_registration(reading, okurigana.as_deref(), surface)
        {
            let line = ItemLine {
                reading: &registration.reading,
                surface: &registration.surface,
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
        // An okurigana grown past a markable first chunk is filed under that
        // chunk (か*っ 勝っ for 勝った), so its other forms going on from that
        // chunk (勝って) find the word too.
        let grown = |kana: &str, head: &str| {
            let rest = kana.strip_prefix(head)?;
            surface.strip_suffix(rest)
        };
        let (reading, okurigana, surface) = match (okurigana, head) {
            (Some(kana), _) if markable(&kana) => (reading.to_owned(), Some(kana), surface),
            (Some(kana), Some(head))
                if markable(&head)
                    && let Some(surface) = grown(&kana, &head) =>
            {
                (reading.to_owned(), Some(head), surface)
            }
            (Some(kana), _) => (format!("{reading}{kana}"), None, surface),
            (None, _) => (reading.to_owned(), None, surface),
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
        let functions = self.user_functions();
        for (priority, slot) in self.slots.iter().enumerate() {
            let dictionary: &dyn Dictionary = match slot {
                Slot::UserCustom => user,
                Slot::Dictionary(d) => d.as_ref(),
            };
            let hidden = |reading: &str, surface: &str| user.is_hidden(reading, surface);
            for mut f in found_in(
                dictionary,
                reading,
                okurigana,
                numbers.as_ref(),
                functions,
                &hidden,
            ) {
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
            let (reading, surface) = f.facts.recorded(reading);
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
    /// Once the reading, or the reading of an item a candidate filled, has a commit in
    /// this field, the field's history decides instead: a choice just made
    /// there is not overturned.
    fn favor(&self, reading: &str, ranked: Vec<Found>) -> Vec<Found> {
        let committed = |reading: &str| self.field.last(reading).is_some();
        if committed(reading)
            || ranked
                .iter()
                .any(|f| committed(f.facts.recorded(reading).0))
        {
            return ranked;
        }
        let mut weighed: Vec<(f64, Found)> = ranked
            .into_iter()
            .map(|f| {
                let (recorded_reading, surface) = f.facts.recorded(reading);
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

impl Engine {
    /// Readings longer than `prefix` that start with it, to complete it
    /// with: those committed in this field, latest first; those of pairs
    /// picked again and again, heaviest first; then each dictionary's in
    /// its priority, by the cheapest word that does not conjugate, then
    /// shortest first. A reading with a placeholder is never one.
    fn completions(&self, prefix: &str) -> Vec<String> {
        if prefix.is_empty() {
            return Vec::new();
        }
        let goes_on =
            |r: &str| r.len() > prefix.len() && r.starts_with(prefix) && !r.contains(OPEN);
        let mut readings: Vec<String> = Vec::new();
        let add = |readings: &mut Vec<String>, reading: &str| {
            if goes_on(reading) && !readings.iter().any(|r| r == reading) {
                readings.push(reading.to_owned());
            }
        };
        for (reading, _) in self.field.history.iter().rev() {
            add(&mut readings, reading);
        }
        for reading in self.selections.favorite_readings(prefix) {
            add(&mut readings, reading);
        }
        let user = self.user.dictionary();
        for slot in &self.slots {
            if readings.len() >= MAX_COMPLETIONS {
                break;
            }
            let dictionary: &dyn Dictionary = match slot {
                Slot::UserCustom => user,
                Slot::Dictionary(d) => d.as_ref(),
            };
            let mut found: Vec<(u32, usize, String)> = dictionary
                .readings_from(prefix, COMPLETIONS_LOOKED_AT)
                .into_iter()
                .filter(|r| goes_on(r) && !readings.contains(r))
                .filter_map(|reading| {
                    let cheapest = dictionary
                        .lookup(&reading)
                        .into_iter()
                        .filter(|e| {
                            e.conjugation.is_none() && !user.is_hidden(&reading, &e.surface)
                        })
                        .map(|e| e.cost)
                        .min()?;
                    Some((cheapest, reading.chars().count(), reading))
                })
                .collect();
            found.sort_unstable();
            for (_, _, reading) in found {
                add(&mut readings, &reading);
            }
        }
        readings.truncate(MAX_COMPLETIONS);
        readings
    }
}

impl Converter for Engine {
    fn complete(&self, reading: &str) -> Vec<String> {
        // Looked up as NFC, but each goes on from `reading` as given: the
        // core keeps the readings that start with it.
        let normal = nfc(reading);
        self.completions(&normal)
            .into_iter()
            .filter_map(|completed| {
                let added = completed.strip_prefix(normal.as_str())?;
                Some(format!("{reading}{added}"))
            })
            .collect()
    }

    fn convert(&self, reading: &str, okurigana: Option<&str>) -> Vec<Candidate> {
        let reading = &nfc(reading);
        let okurigana = okurigana.map(nfc);
        let found = self.found(reading, okurigana.as_deref());
        if okurigana.is_none() && !self.without_functions.get() {
            let facts = found.iter().map(|f| f.facts.clone()).collect();
            self.last.replace(Some((reading.clone(), facts)));
        }
        let ranked = self.rank(reading, found);
        let ranked = self.okuri_first(reading, ranked);
        self.favor(reading, ranked)
            .into_iter()
            .map(|f| Candidate {
                surface: f.facts.surface,
            })
            .collect()
    }

    fn registered_text(&self, reading: &str, okurigana: Option<&str>, surface: &str) -> String {
        let reading = nfc(reading);
        let Some(registration) = placeholder_registration(&reading, okurigana, surface) else {
            if okurigana.is_none() {
                self.remember(&reading, CandidateFacts::plain(nfc(surface)));
            }
            return surface.to_owned();
        };
        let Some(text) = fill(
            &registration.surface,
            &registration.numbers,
            &reading,
            self.user_functions(),
        )
        .map(|text| nfc(&text)) else {
            return surface.to_owned();
        };
        let facts = CandidateFacts {
            template: Some((registration.reading, registration.surface)),
            ..CandidateFacts::plain(text.clone())
        };
        self.remember(&reading, facts);
        text
    }
}

/// A word registered with placeholders, written as an item with them.
struct PlaceholderRegistration {
    /// With a placeholder in place of each number.
    reading: String,
    surface: String,
    /// As typed.
    numbers: Vec<String>,
}

/// `None` when `surface` has no placeholders, or would make an invalid line.
fn placeholder_registration(
    reading: &str,
    okurigana: Option<&str>,
    surface: &str,
) -> Option<PlaceholderRegistration> {
    if okurigana.is_some() {
        return None;
    }
    let surface = mark_placeholders(surface)?;
    let (reading, numbers) = match Numbers::find(reading) {
        Some(found) => (found.reading, found.values),
        None => (reading.to_owned(), Vec::new()),
    };
    fits(&surface, numbers.len()).then_some(PlaceholderRegistration {
        reading,
        surface,
        numbers,
    })
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
    functions: Option<&dyn Functions>,
    hidden: &dyn Fn(&str, &str) -> bool,
) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    let mut add = |surface: String,
                   cost: u32,
                   built: bool,
                   template: Option<(String, String)>,
                   okuri: bool| {
        let (recorded_reading, recorded_surface) = match &template {
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
                    f.facts.template = template;
                }
            }
            None => found.push(Found {
                facts: CandidateFacts {
                    surface,
                    dictionary: 0,
                    cost,
                    built,
                    template,
                },
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
        if !e.surface.contains(OPEN) {
            add(e.surface, e.cost, false, None, false);
        } else if okurigana.is_none()
            // A hidden item runs no function: one may count its calls.
            && !hidden(reading, &e.surface)
            && let Some(surface) = fill(&e.surface, &[], reading, functions)
        {
            let template = (reading.to_owned(), e.surface);
            add(nfc(&surface), e.cost, false, Some(template), false);
        }
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
            .map(|kana| okuri_lookup(dictionary, stem, kana))
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
            if hidden(&numbers.reading, &e.surface) {
                continue;
            }
            if let Some(surface) = fill(&e.surface, &numbers.values, reading, functions) {
                let template = (numbers.reading.clone(), e.surface);
                add(nfc(&surface), e.cost, false, Some(template), false);
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
