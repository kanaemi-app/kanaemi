//! Numbers in readings, and the ways a numeric item writes them.
//!
//! A numeric item holds its placeholders as [`OPEN`] and [`CLOSE`] around a
//! notation's name, so a literal `{` or `}` in a word stays itself.

pub(crate) const OPEN: char = '\u{FDD0}';
pub(crate) const CLOSE: char = '\u{FDD1}';

/// How a numeric item writes the number put in a placeholder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Notation {
    Digits,
    WideDigits,
    KanjiDigits,
    Kanji,
    Daiji,
    Grouped,
}

impl Notation {
    /// The notation a placeholder names; the empty name is plain digits.
    pub(crate) fn named(name: &str) -> Option<Self> {
        Some(match name {
            "" => Self::Digits,
            "wide-num" => Self::WideDigits,
            "kanji-num" => Self::KanjiDigits,
            "kanji" => Self::Kanji,
            "daiji" => Self::Daiji,
            "grouped-num" => Self::Grouped,
            _ => return None,
        })
    }

    /// `digits` are ASCII or full-width ones. `None` when the notation cannot
    /// write the number.
    pub(crate) fn write(self, digits: &str) -> Option<String> {
        let digits: Vec<u8> = digits.chars().map(digit).collect::<Option<_>>()?;
        let significant = match digits.iter().position(|&d| d != 0) {
            Some(at) => &digits[at..],
            None => &[0][..],
        };
        let each = |of: &dyn Fn(u8) -> char| digits.iter().map(|&d| of(d)).collect();
        match self {
            Self::Digits => Some(each(&|d| char::from(b'0' + d))),
            Self::WideDigits => Some(each(&|d| WIDE_DIGITS[usize::from(d)])),
            Self::KanjiDigits => Some(each(&|d| KANJI_DIGITS[usize::from(d)])),
            Self::Kanji => counted(significant, false),
            Self::Daiji => counted(significant, true),
            Self::Grouped => Some(grouped(significant)),
        }
    }
}

/// The numbers of a reading, and the reading a numeric item has for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Numbers {
    /// The reading with a placeholder in place of each number.
    pub(crate) reading: String,
    /// The digits of each number as typed, first to last.
    pub(crate) values: Vec<String>,
}

impl Numbers {
    /// `None` for a reading without digits.
    pub(crate) fn find(reading: &str) -> Option<Self> {
        let mut found = Self {
            reading: String::new(),
            values: Vec::new(),
        };
        let mut in_number = false;
        for c in reading.chars() {
            match (digit(c).is_some(), in_number) {
                (true, true) => found.values.last_mut()?.push(c),
                (true, false) => {
                    found.reading.extend([OPEN, CLOSE]);
                    found.values.push(c.to_string());
                }
                (false, _) => found.reading.push(c),
            }
            in_number = digit(c).is_some();
        }
        (!found.values.is_empty()).then_some(found)
    }
}

/// `surface` with each placeholder replaced by the number of its place, as its
/// notation writes it. `None` when a placeholder has no number or its notation
/// cannot write it.
pub(crate) fn fill(surface: &str, values: &[String]) -> Option<String> {
    let mut out = String::new();
    let mut values = values.iter();
    let mut rest = surface;
    while let Some(open) = rest.find(OPEN) {
        out.push_str(&rest[..open]);
        let inside = &rest[open + OPEN.len_utf8()..];
        let close = inside.find(CLOSE)?;
        let notation = Notation::named(&inside[..close])?;
        out.push_str(&notation.write(values.next()?)?);
        rest = &inside[close + CLOSE.len_utf8()..];
    }
    out.push_str(rest);
    Some(out)
}

const WIDE_DIGITS: [char; 10] = ['０', '１', '２', '３', '４', '５', '６', '７', '８', '９'];
const KANJI_DIGITS: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
const SMALL_UNITS: [&str; 4] = ["", "十", "百", "千"];
const LARGE_UNITS: [&str; 5] = ["", "万", "億", "兆", "京"];

fn digit(c: char) -> Option<u8> {
    match c {
        '0'..='9' => Some(c as u8 - b'0'),
        '０'..='９' => Some((u32::from(c) - u32::from('０')) as u8),
        _ => None,
    }
}

/// The number in kanji counted by its units, or `None` past the largest unit.
/// `digits` has no leading zero, unless it is 0 itself.
fn counted(digits: &[u8], daiji: bool) -> Option<String> {
    if digits.len() > SMALL_UNITS.len() * LARGE_UNITS.len() {
        return None;
    }
    if digits == [0] {
        return Some(KANJI_DIGITS[0].to_string());
    }
    let mut out = String::new();
    // Ones first, so each group of four lines up with its unit.
    let groups: Vec<&[u8]> = digits.rchunks(SMALL_UNITS.len()).collect();
    for (group, large) in groups.iter().zip(LARGE_UNITS).rev() {
        if group.iter().all(|&d| d == 0) {
            continue;
        }
        for (&d, small) in group.iter().rev().zip(SMALL_UNITS).rev() {
            if d == 0 {
                continue;
            }
            if d != 1 || small.is_empty() || daiji {
                out.push(KANJI_DIGITS[usize::from(d)]);
            }
            out.push_str(small);
        }
        out.push_str(large);
    }
    if daiji {
        out = out
            .chars()
            .map(|c| match c {
                '一' => '壱',
                '二' => '弐',
                '三' => '参',
                '十' => '拾',
                c => c,
            })
            .collect();
    }
    Some(out)
}

fn grouped(digits: &[u8]) -> String {
    let mut out = String::new();
    for (i, &d) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(char::from(b'0' + d));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(name: &str, digits: &str) -> Option<String> {
        Notation::named(name).unwrap().write(digits)
    }

    #[test]
    fn each_name_writes_twelve_and_two_thousand_twenty_six_its_way() {
        let cases = [
            ("", "12", "2026"),
            ("wide-num", "１２", "２０２６"),
            ("kanji-num", "一二", "二〇二六"),
            ("kanji", "十二", "二千二十六"),
            ("daiji", "壱拾弐", "弐千弐拾六"),
            ("grouped-num", "12", "2,026"),
        ];
        for (name, twelve, year) in cases {
            assert_eq!(write(name, "12").as_deref(), Some(twelve), "{name}");
            assert_eq!(write(name, "2026").as_deref(), Some(year), "{name}");
        }
    }

    #[test]
    fn an_unknown_name_is_no_notation() {
        assert_eq!(Notation::named("roman"), None);
        assert_eq!(Notation::named("Kanji"), None);
    }

    #[test]
    fn kanji_counts_in_groups_of_ten_thousand() {
        let cases = [
            ("10000", "一万"),
            ("1000000", "百万"),
            ("100010", "十万十"),
            ("1111", "千百十一"),
            ("10000000", "千万"),
            ("100000000", "一億"),
            ("1000000000000", "一兆"),
            ("10000000000000000", "一京"),
            (
                "99999999999999999999",
                "九千九百九十九京九千九百九十九兆九千九百九十九億九千九百九十九万九千九百九十九",
            ),
        ];
        for (digits, kanji) in cases {
            assert_eq!(write("kanji", digits).as_deref(), Some(kanji), "{digits}");
        }
    }

    #[test]
    fn daiji_writes_every_one_and_changes_only_its_own_characters() {
        let cases = [
            ("1111", "壱千壱百壱拾壱"),
            ("10", "壱拾"),
            ("10000", "壱万"),
            ("345", "参百四拾五"),
        ];
        for (digits, daiji) in cases {
            assert_eq!(write("daiji", digits).as_deref(), Some(daiji), "{digits}");
        }
    }

    #[test]
    fn ten_thousand_kei_and_above_has_no_kanji_or_daiji() {
        assert_eq!(write("kanji", "100000000000000000000"), None);
        assert_eq!(write("daiji", "100000000000000000000"), None);
        assert_eq!(
            write("", "100000000000000000000").as_deref(),
            Some("100000000000000000000")
        );
        assert_eq!(
            write("grouped-num", "100000000000000000000").as_deref(),
            Some("100,000,000,000,000,000,000")
        );
    }

    #[test]
    fn leading_zeros_stay_in_digits_and_go_from_counted_numbers() {
        let cases = [
            ("", "007"),
            ("wide-num", "００７"),
            ("kanji-num", "〇〇七"),
            ("kanji", "七"),
            ("daiji", "七"),
            ("grouped-num", "7"),
        ];
        for (name, written) in cases {
            assert_eq!(write(name, "007").as_deref(), Some(written), "{name}");
        }
    }

    #[test]
    fn zero_is_written_as_zero() {
        let cases = [("kanji", "〇"), ("daiji", "〇"), ("grouped-num", "0")];
        for (name, written) in cases {
            assert_eq!(write(name, "0").as_deref(), Some(written), "{name}");
            assert_eq!(write(name, "000").as_deref(), Some(written), "{name}");
        }
    }

    fn placeholder(name: &str) -> String {
        format!("{OPEN}{name}{CLOSE}")
    }

    #[test]
    fn each_run_of_digits_in_a_reading_is_one_number() {
        let found = Numbers::find("1がつ２０にち").unwrap();
        assert_eq!(
            found.reading,
            format!("{}がつ{}にち", placeholder(""), placeholder(""))
        );
        assert_eq!(found.values, ["1", "２０"]);
    }

    #[test]
    fn ascii_and_full_width_digits_mix_in_one_number() {
        let found = Numbers::find("1２こ").unwrap();
        assert_eq!(found.reading, format!("{}こ", placeholder("")));
        assert_eq!(found.values, ["1２"]);
    }

    #[test]
    fn a_point_or_a_comma_splits_numbers() {
        assert_eq!(Numbers::find("1.5").unwrap().values, ["1", "5"]);
        assert_eq!(Numbers::find("1,000").unwrap().values, ["1", "000"]);
    }

    #[test]
    fn a_reading_without_digits_has_no_numbers() {
        assert!(Numbers::find("こ").is_none());
    }

    #[test]
    fn the_nth_placeholder_takes_the_nth_number_in_its_notation() {
        let surface = format!("{}月{}日", placeholder("kanji"), placeholder("wide-num"));
        let values = ["12".to_owned(), "1".to_owned()];
        assert_eq!(fill(&surface, &values).as_deref(), Some("十二月１日"));
    }

    #[test]
    fn a_surface_left_with_no_number_or_an_unwritable_one_fills_nothing() {
        let values = ["1".to_owned()];
        let two = format!("{0}と{0}", placeholder(""));
        assert_eq!(fill(&two, &values), None);
        let huge = ["100000000000000000000".to_owned()];
        assert_eq!(fill(&placeholder("kanji"), &huge), None);
    }

    #[test]
    fn a_surface_without_placeholders_stays_as_written() {
        assert_eq!(fill("{個}", &["1".to_owned()]).as_deref(), Some("{個}"));
    }

    #[test]
    fn full_width_digits_are_read_as_their_numbers() {
        assert_eq!(write("", "１２").as_deref(), Some("12"));
        assert_eq!(write("kanji", "２０２６").as_deref(), Some("二千二十六"));
        assert_eq!(write("wide-num", "1２").as_deref(), Some("１２"));
    }
}
