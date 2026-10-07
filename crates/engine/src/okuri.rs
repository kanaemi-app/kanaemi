//! Okurigana by kana or by row: a dictionary files an okurigana word under the
//! first kana of its okurigana (書く under か and く), or under only the
//! letter of that kana's row (書 under か and k) where the kana is not known,
//! as with SKK, which keeps only the okurigana's consonant.

/// What an okurigana word is filed under after its stem.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OkuriHead {
    /// The first kana of the okurigana, one with a row: found only by
    /// okurigana starting with that kana.
    Kana(char),
    /// The letter of a row: found by okurigana starting with any kana of it.
    Row(char),
}

impl OkuriHead {
    /// The head a text dictionary writes after the `*`: a kana with a row, or
    /// the letter of a row.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let mut chars = text.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            return None;
        };
        if okuri_row(c).is_some() {
            Some(Self::Kana(c))
        } else {
            is_okuri_row(c).then_some(Self::Row(c))
        }
    }

    /// The heads okurigana starting with `kana` is found under: the kana
    /// itself, then its row. None for a kana no row has.
    pub(crate) fn of(kana: char) -> impl Iterator<Item = Self> {
        okuri_row(kana)
            .map(|row| [Self::Kana(kana), Self::Row(row)])
            .into_iter()
            .flatten()
    }

    /// Whether `okurigana` starts as this head files it.
    pub(crate) fn starts(self, okurigana: &str) -> bool {
        let Some(first) = okurigana.chars().next() else {
            return false;
        };
        match self {
            Self::Kana(kana) => first == kana,
            Self::Row(row) => okuri_row(first) == Some(row),
        }
    }

    /// The character written after the stem, in a text dictionary and in the
    /// binary dictionary's index alike.
    pub(crate) fn as_char(self) -> char {
        match self {
            Self::Kana(c) | Self::Row(c) => c,
        }
    }
}

/// The letter that files okurigana starting with `kana`, or `None` for a kana
/// no row has.
pub(crate) fn okuri_row(kana: char) -> Option<char> {
    ROWS.iter()
        .find(|(_, kanas)| kanas.contains(kana))
        .map(|(row, _)| *row)
}

/// Whether `letter` names a row okurigana is filed under.
pub(crate) fn is_okuri_row(letter: char) -> bool {
    ROWS.iter().any(|&(row, _)| row == letter)
}

/// The key a stem and a head are filed under. A kana and a row's letter never
/// meet, so the two kinds of key stay apart.
pub(crate) fn okuri_key(stem: &str, head: OkuriHead) -> String {
    format!("{stem}{}", head.as_char())
}

/// Small kana have a row of their own, by the `x` that types them (ゃ of
/// `xya`): filed with the large ones, `;tachi;ya` would also find 達ゃ as
/// 達や. っ stays with た, as the doubled consonant types it.
const ROWS: [(char, &str); 23] = [
    ('a', "あ"),
    ('i', "い"),
    ('u', "う"),
    ('e', "え"),
    ('o', "お"),
    ('k', "かきくけこ"),
    ('g', "がぎぐげご"),
    ('s', "さしすせそ"),
    ('z', "ざずぜぞ"),
    ('j', "じ"),
    ('t', "たちつてとっ"),
    ('d', "だぢづでど"),
    ('n', "なにぬねのん"),
    ('h', "はひへほ"),
    ('f', "ふ"),
    ('b', "ばびぶべぼ"),
    ('p', "ぱぴぷぺぽ"),
    ('v', "ゔ"),
    ('m', "まみむめも"),
    ('y', "やゆよ"),
    ('r', "らりるれろ"),
    ('w', "わゐゑを"),
    ('x', "ぁぃぅぇぉゃゅょゎゕゖ"),
];
