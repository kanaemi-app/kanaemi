use std::mem;
use std::ops::Range;

use crate::RomajiTable;
use crate::edit::{CursorMove, Editable};

/// A reading being typed: kana before the okurigana, the okurigana once it is marked,
/// and romaji not yet turned into kana.
#[derive(Clone, Debug, Default)]
pub(crate) struct Word {
    /// The cursor moves only before the okurigana is marked.
    pub(crate) stem: Editable,
    pub(crate) okurigana: Option<String>,
    pub(crate) pending: String,
    /// The keys that made each kana of the stem and of the okurigana, kept
    /// beside them through every edit so the reading can be given back in
    /// letters.
    stem_keys: Vec<Keys>,
    okurigana_keys: Vec<Keys>,
    groups: u32,
    /// Keys that made no kana (the `s` of `test`, a lone `k` at a commit),
    /// kept for the letters and joined to the next kana's keys.
    loose: String,
}

/// The keys that made a run of kana, held by each kana of the run.
#[derive(Clone, Debug)]
struct Keys {
    group: u32,
    kana: usize,
    keys: String,
}

impl Word {
    pub(crate) fn is_empty(&self) -> bool {
        self.stem.is_empty() && self.okurigana.is_none() && self.pending.is_empty()
    }

    pub(crate) fn kana(&self) -> String {
        let mut kana = self.stem.as_str().to_owned();
        kana.push_str(self.okurigana.as_deref().unwrap_or(""));
        kana
    }

    /// The reading in the letters typed for it. Kana whose keys are no
    /// longer whole — one of a pair typed together was erased — are spelled
    /// from the table instead.
    pub(crate) fn letters(&self, table: &RomajiTable) -> String {
        // What is being typed sits at the cursor, or after the okurigana.
        let typing = format!("{}{}", self.loose, self.pending);
        if let Some(okurigana) = &self.okurigana {
            let mut letters = spelled(self.stem.as_str(), &self.stem_keys, table);
            letters.push_str(&spelled(okurigana, &self.okurigana_keys, table));
            letters.push_str(&typing);
            return letters;
        }
        if typing.is_empty() {
            return spelled(self.stem.as_str(), &self.stem_keys, table);
        }
        let at = self.stem.cursor();
        let (before, after) = self.stem.split();
        let mut letters = spelled(before, &self.stem_keys[..at], table);
        letters.push_str(&typing);
        letters.push_str(&spelled(after, &self.stem_keys[at..], table));
        letters
    }

    /// The okurigana to convert with, once its first kana is typed.
    pub(crate) fn okurigana(&self) -> Option<&str> {
        self.okurigana.as_deref().filter(|o| !o.is_empty())
    }

    /// Input the table knows is fed through it; any other character joins the
    /// kana as it is.
    pub(crate) fn feed(&mut self, c: char, table: &RomajiTable) {
        if !table.is_input_char(c) {
            let opening = self.okurigana.as_deref() == Some("");
            self.flush(table);
            // The okurigana already took its one kana from the romaji before.
            if opening && self.okurigana().is_some() {
                self.pending.push(c);
            } else {
                self.push(c.encode_utf8(&mut [0; 4]), c.to_string());
            }
            return;
        }
        let mut typed = self.pending.clone();
        typed.push(c);
        let made = table.feed_parts(&mut self.pending, c);
        // What is still pending is the end of what was typed.
        let used = typed.len() - self.pending.len();
        self.place(made, &typed, used);
    }

    pub(crate) fn flush(&mut self, table: &RomajiTable) {
        let typed = mem::take(&mut self.pending);
        let made = table.flush_parts(&mut typed.clone());
        self.place(made, &typed, typed.len());
    }

    /// Places kana made from the first `used` bytes of `typed`: an okurigana
    /// starts with one kana. What was made after that kana is given back as
    /// its keys, to be typed again after the word.
    fn place(&mut self, mut made: Vec<(usize, String)>, typed: &str, mut used: usize) {
        if self.okurigana.as_deref() == Some("") && !made.is_empty() {
            made.truncate(1);
            // っ is made from a letter still pending, too.
            let end = made[0].0.min(used);
            self.pending.insert_str(0, &typed[end..used]);
            used = end;
        }
        let kana: String = made.into_iter().map(|(_, kana)| kana).collect();
        self.push(&kana, typed[..used].to_owned());
    }

    fn push(&mut self, kana: &str, keys: String) {
        let count = kana.chars().count();
        if count == 0 {
            self.loose.push_str(&keys);
            return;
        }
        let keys = mem::take(&mut self.loose) + &keys;
        self.groups += 1;
        let made = (0..count).map(|_| Keys {
            group: self.groups,
            kana: count,
            keys: keys.clone(),
        });
        match &mut self.okurigana {
            Some(okurigana) => {
                okurigana.push_str(kana);
                self.okurigana_keys.extend(made);
            }
            None => {
                let at = self.stem.cursor();
                self.stem.insert(kana);
                self.stem_keys.splice(at..at, made);
            }
        }
    }

    /// Keys that made no kana belong where they were typed: before the
    /// cursor leaves, they join the keys of the kana before it, or at the
    /// start of the kana after it. With no kana to join, they stay loose.
    fn settle_loose(&mut self, table: &RomajiTable) {
        if self.loose.is_empty() || self.okurigana.is_some() {
            return;
        }
        let at = self.stem.cursor();
        let (index, after) = match at.checked_sub(1) {
            Some(before) => (before, true),
            None if self.stem_keys.is_empty() => return,
            None => (0, false),
        };
        let run = self.whole_run(index, table);
        let loose = mem::take(&mut self.loose);
        for keys in &mut self.stem_keys[run] {
            if after {
                keys.keys.push_str(&loose);
            } else {
                keys.keys.insert_str(0, &loose);
            }
        }
    }

    /// The run of stem kana typed together that holds the kana at `index`.
    /// A run no longer whole is given the keys the table spells it with, as
    /// [`spelled`] would, so keys added to it are not spelled over.
    fn whole_run(&mut self, index: usize, table: &RomajiTable) -> Range<usize> {
        let group = self.stem_keys[index].group;
        let same = |k: &Keys| k.group == group;
        let start = index
            - self.stem_keys[..index]
                .iter()
                .rev()
                .take_while(|k| same(k))
                .count();
        let end = index
            + self.stem_keys[index..]
                .iter()
                .take_while(|k| same(k))
                .count();
        if end - start != self.stem_keys[index].kana {
            let kana: String = self
                .stem
                .as_str()
                .chars()
                .skip(start)
                .take(end - start)
                .collect();
            let keys = table.spell(&kana);
            self.groups += 1;
            for k in &mut self.stem_keys[start..end] {
                k.group = self.groups;
                k.kana = end - start;
                k.keys = keys.clone();
            }
        }
        start..end
    }

    pub(crate) fn move_cursor(&mut self, to: CursorMove, table: &RomajiTable) {
        self.settle_loose(table);
        if self.okurigana.is_none() {
            self.stem.move_cursor(to);
        }
    }

    pub(crate) fn delete(&mut self, table: &RomajiTable) {
        self.settle_loose(table);
        let at = self.stem.cursor();
        if at < self.stem_keys.len() {
            self.stem.delete();
            self.stem_keys.remove(at);
        }
    }

    /// Marks where the okurigana starts, once the pending romaji is kana of
    /// the stem. Only at the end of a stem with kana, and only once.
    pub(crate) fn mark_okurigana(&mut self, table: &RomajiTable) {
        if self.okurigana.is_some() || !self.stem.at_end() {
            return;
        }
        let mut marked = self.clone();
        marked.flush(table);
        if !marked.stem.is_empty() {
            marked.okurigana = Some(String::new());
            *self = marked;
        }
    }

    /// Removes pending romaji first, then okurigana, then the okurigana mark, then the stem.
    pub(crate) fn backspace(&mut self) {
        // Unseen, the loose keys are not what Backspace is for.
        self.loose.clear();
        if self.pending.pop().is_some() {
            return;
        }
        match &mut self.okurigana {
            Some(okurigana) => {
                if okurigana.pop().is_none() {
                    self.okurigana = None;
                } else {
                    self.okurigana_keys.pop();
                }
            }
            None => {
                let at = self.stem.cursor();
                if self.stem.backspace() {
                    self.stem_keys.remove(at - 1);
                }
            }
        }
    }
}

/// `kana` in letters: each run of kana typed together by its keys, while the
/// run is whole, and otherwise by the table.
fn spelled(kana: &str, keys: &[Keys], table: &RomajiTable) -> String {
    let chars: Vec<char> = kana.chars().collect();
    let mut letters = String::new();
    let mut i = 0;
    while i < chars.len() {
        let group = keys[i].group;
        let run = keys[i..].iter().take_while(|k| k.group == group).count();
        if run == keys[i].kana {
            letters.push_str(&keys[i].keys);
        } else {
            let part: String = chars[i..i + run].iter().collect();
            letters.push_str(&table.spell(&part));
        }
        i += run;
    }
    letters
}
