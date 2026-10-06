//! Numbers in readings, which numeric items put in their placeholders.

use crate::placeholder::{CLOSE, OPEN};

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
            match (is_digit(c), in_number) {
                (true, true) => found.values.last_mut()?.push(c),
                (true, false) => {
                    found.reading.extend([OPEN, CLOSE]);
                    found.values.push(c.to_string());
                }
                (false, _) => found.reading.push(c),
            }
            in_number = is_digit(c);
        }
        (!found.values.is_empty()).then_some(found)
    }
}

fn is_digit(c: char) -> bool {
    matches!(c, '0'..='9' | '０'..='９')
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
