//! Okurigana by row: dictionaries file an okurigana word under the letter of
//! the row its okurigana starts in (書く under か and k), since the kana
//! itself changes as the word conjugates.

/// The letter that files okurigana starting with `kana`, or `None` for a kana
/// no row has.
pub(crate) fn okuri_row(kana: char) -> Option<char> {
    ROWS.iter()
        .find(|(_, kanas)| kanas.contains(kana))
        .map(|(row, _)| *row)
}

/// The key a stem and an okurigana row are filed under.
pub(crate) fn okuri_key(stem: &str, row: char) -> String {
    format!("{stem}{row}")
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
