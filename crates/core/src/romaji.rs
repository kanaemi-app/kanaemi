/// What the keys type in kana mode, built by applying romaji table
/// files one after another. Which files, and their text, are the host's to
/// give; this owns their format and how they stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RomajiTable {
    rules: Vec<(String, String)>,
}

enum Line {
    Add(String, String),
    Remove(String),
}

impl RomajiTable {
    pub fn empty() -> Self {
        Self { rules: Vec::new() }
    }

    /// Applies a romaji table file on top of this table. Invalid lines are
    /// skipped; their 1-based numbers are returned.
    pub fn apply(&mut self, text: impl AsRef<str>) -> Vec<usize> {
        let text = text.as_ref();
        let mut invalid = Vec::new();
        for (i, line) in text.trim_start_matches('\u{feff}').split('\n').enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match parse_line(line) {
                Some(Line::Add(input, output)) => self.insert(input, output),
                Some(Line::Remove(input)) => self.rules.retain(|(r, _)| *r != input),
                None => invalid.push(i + 1),
            }
        }
        invalid
    }

    fn insert(&mut self, input: String, output: String) {
        match self.rules.iter_mut().find(|(r, _)| *r == input) {
            Some(rule) => rule.1 = output,
            None => self.rules.push((input, output)),
        }
    }

    fn lookup(&self, input: &str) -> Option<&str> {
        self.rules
            .iter()
            .find(|(r, _)| r == input)
            .map(|(_, output)| output.as_str())
    }

    fn extends(&self, prefix: &str) -> bool {
        self.rules
            .iter()
            .any(|(r, _)| r.len() > prefix.len() && r.starts_with(prefix))
    }

    pub(crate) fn is_input_char(&self, c: char) -> bool {
        c.is_ascii_lowercase() || self.rules.iter().any(|(r, _)| r.contains(c))
    }

    /// Appends `c` to the input typed so far and returns the text it completes.
    pub(crate) fn feed(&self, pending: &mut String, c: char) -> String {
        self.feed_parts(pending, c)
            .into_iter()
            .map(|(_, kana)| kana)
            .collect()
    }

    /// As [`Self::feed`], each piece of text with how many keys of the input,
    /// counted from its start, it was made from. っ for a doubled letter is
    /// made from both letters, ん for an `n` from the `n` alone.
    pub(crate) fn feed_parts(&self, pending: &mut String, c: char) -> Vec<(usize, String)> {
        pending.push(c);
        self.resolve(pending, false)
    }

    /// Turns `pending` into text as far as it can. Unless `finishing`, input
    /// that may still grow into a rule, ん or っ waits for the next key.
    fn resolve(&self, pending: &mut String, finishing: bool) -> Vec<(usize, String)> {
        let mut made = Vec::new();
        let mut used = 0;
        while let Some(first) = pending.chars().next() {
            if !finishing && self.extends(pending) {
                break;
            }
            if let Some(k) = self.lookup(pending) {
                used += pending.len();
                made.push((used, k.to_owned()));
                pending.clear();
                break;
            }
            // A rule that was waiting for a longer one that did not come.
            if let Some((len, k)) = self.longest_prefix_rule(pending) {
                used += len;
                made.push((used, k.to_owned()));
                pending.drain(..len);
                continue;
            }
            let second = pending.chars().nth(1);
            let consonant = first.is_ascii_lowercase() && !"aiueo".contains(first);
            if consonant && second.is_none() && !finishing {
                break;
            }
            if first == 'n' {
                made.push((used + 1, "ん".to_owned()));
            } else if consonant && second == Some(first) {
                made.push((used + 2, "っ".to_owned()));
            } else if !first.is_ascii_lowercase() {
                made.push((used + first.len_utf8(), first.to_string()));
            }
            pending.remove(0);
            used += first.len_utf8();
        }
        made
    }

    /// Letters that type `kana`: at each place the rule making the most kana,
    /// and of those the one with the fewest keys. Kana no rule makes stays.
    pub(crate) fn spell(&self, kana: &str) -> String {
        let mut letters = String::new();
        let mut rest = kana;
        while let Some(c) = rest.chars().next() {
            let best = self
                .rules
                .iter()
                .filter(|(_, output)| rest.starts_with(output.as_str()))
                .max_by(|(ai, ao), (bi, bo)| ao.len().cmp(&bo.len()).then(bi.len().cmp(&ai.len())));
            match best {
                Some((input, output)) => {
                    letters.push_str(input);
                    rest = &rest[output.len()..];
                }
                None => {
                    letters.push(c);
                    rest = &rest[c.len_utf8()..];
                }
            }
        }
        letters
    }

    fn longest_prefix_rule(&self, input: &str) -> Option<(usize, &str)> {
        input
            .char_indices()
            .rev()
            .map(|(i, _)| i)
            .filter(|&i| i > 0)
            .find_map(|i| self.lookup(&input[..i]).map(|k| (i, k)))
    }

    /// Resolves unfinished romaji at a commit, as if no key could follow it.
    pub(crate) fn flush(&self, pending: &mut String) -> String {
        self.flush_parts(pending)
            .into_iter()
            .map(|(_, kana)| kana)
            .collect()
    }

    /// As [`Self::flush`], each piece of text with where its keys end, as
    /// [`Self::feed_parts`] gives them.
    pub(crate) fn flush_parts(&self, pending: &mut String) -> Vec<(usize, String)> {
        self.resolve(pending, true)
    }
}

fn parse_line(line: &str) -> Option<Line> {
    if let Some(input) = line.strip_prefix('!') {
        return Some(Line::Remove(parse_input(input)?));
    }
    let (input, output) = line.split_once('\t')?;
    let output = unescape(output, false)?;
    if output.is_empty() || output.contains('\t') {
        return None;
    }
    Some(Line::Add(parse_input(input)?, output))
}

/// Input is printable ASCII.
fn parse_input(field: &str) -> Option<String> {
    let input = unescape(field, true)?;
    let valid = !input.is_empty() && input.chars().all(|c| matches!(c, '!'..='~'));
    valid.then_some(input)
}

fn unescape(field: &str, is_input: bool) -> Option<String> {
    let mut out = String::new();
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            '\\' => out.push('\\'),
            c @ ('#' | '!') if is_input && out.is_empty() => out.push(c),
            _ => return None,
        }
    }
    Some(out)
}

/// Half-width katakana, with voiced kana split into the kana and its mark.
pub(crate) fn half_katakana(hiragana: &str) -> String {
    const FULL: &str = "アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワヲンァィゥェォャュョッー、。「」・";
    const HALF: &str = "ｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜｦﾝｧｨｩｪｫｬｭｮｯｰ､｡｢｣･";
    const VOICED: &str = "ガギグゲゴザジズゼゾダヂヅデドバビブベボ";
    const VOICED_BASE: &str = "カキクケコサシスセソタチツテトハヒフヘホ";
    const SEMI_VOICED: &str = "パピプペポ";
    let half = |c: char| {
        FULL.chars()
            .position(|f| f == c)
            .and_then(|i| HALF.chars().nth(i))
    };
    let mut out = String::new();
    for c in katakana(hiragana).chars() {
        if let Some(i) = VOICED.chars().position(|v| v == c) {
            let base = VOICED_BASE.chars().nth(i).and_then(half).unwrap_or(c);
            out.push(base);
            out.push('ﾞ');
        } else if let Some(i) = SEMI_VOICED.chars().position(|v| v == c) {
            out.push(HALF.chars().nth(25 + i).unwrap_or(c));
            out.push('ﾟ');
        } else if c == 'ヴ' {
            out.push_str("ｳﾞ");
        } else {
            out.push(half(c).unwrap_or(c));
        }
    }
    out
}

/// ASCII as full-width characters, the space as the ideographic space.
pub(crate) fn full_width(ascii: &str) -> String {
    ascii
        .chars()
        .map(|c| match c {
            ' ' => '\u{3000}',
            '!'..='~' => char::from_u32(c as u32 + 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .collect()
}

pub(crate) fn katakana(hiragana: &str) -> String {
    hiragana
        .chars()
        .map(|c| match c {
            'ぁ'..='ゖ' => char::from_u32(c as u32 + 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}
