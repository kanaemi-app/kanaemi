//! Placeholders in a surface, which a function fills each time the item is
//! converted.
//!
//! An item holds a placeholder as [`OPEN`] and [`CLOSE`] around what was
//! written between its braces, so a literal `{` or `}` in a word stays itself.

use crate::numeric::Notation;

pub(crate) const OPEN: char = '\u{FDD0}';
pub(crate) const CLOSE: char = '\u{FDD1}';

/// Functions the user adds. One of the name of a built-in function goes in its
/// place.
pub trait Functions {
    fn has(&self, name: &str) -> bool;

    /// The text the function fills a placeholder with; `None` when it fails
    /// or gives none.
    fn call(&self, call: &Call) -> Option<String>;
}

/// One call of a function, to fill one placeholder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Call<'a> {
    pub name: &'a str,
    pub source: &'a str,
    pub argument: Option<&'a str>,
}

/// What a placeholder hands its function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Position {
    /// The reading's next number, counting only placeholders that name none.
    Next,
    /// The reading's number at this index, from 0.
    At(usize),
    /// The whole reading.
    Reading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placeholder<'a> {
    pub(crate) position: Position,
    pub(crate) name: &'a str,
    pub(crate) argument: Option<&'a str>,
}

impl<'a> Placeholder<'a> {
    /// `inside` is what is between the braces, unescaped. `None` when it is
    /// not written as a placeholder may be.
    pub(crate) fn parse(inside: &'a str) -> Option<Self> {
        let head = inside.find(' ').map_or(inside, |space| &inside[..space]);
        let (position, rest) = match head.find(':') {
            Some(colon) => (Position::parse(&inside[..colon])?, &inside[colon + 1..]),
            None => (Position::Next, inside),
        };
        let (name, argument) = match rest.split_once(' ') {
            Some((name, argument)) => (name, Some(argument)),
            None => (rest, None),
        };
        if name.contains([':', '{', '}', '\\', OPEN, CLOSE]) {
            return None;
        }
        Some(Self {
            position,
            name,
            argument,
        })
    }
}

impl Position {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "" => Some(Self::Next),
            "-" => Some(Self::Reading),
            _ if text.bytes().all(|b| b.is_ascii_digit()) => text.parse().ok().map(Self::At),
            _ => None,
        }
    }
}

/// Each placeholder of `surface`, in order; `None` when one is not written as
/// a placeholder may be.
pub(crate) fn placeholders(surface: &str) -> Option<Vec<Placeholder<'_>>> {
    let mut found = Vec::new();
    let mut rest = surface;
    while let Some(open) = rest.find(OPEN) {
        let inside = &rest[open + OPEN.len_utf8()..];
        let close = inside.find(CLOSE)?;
        found.push(Placeholder::parse(&inside[..close])?);
        rest = &inside[close + CLOSE.len_utf8()..];
    }
    Some(found)
}

/// Whether every placeholder of `surface` has what it takes from a reading
/// with `numbers` numbers.
pub(crate) fn fits(surface: &str, numbers: usize) -> bool {
    let Some(placeholders) = placeholders(surface) else {
        return false;
    };
    let mut next = 0;
    placeholders.iter().all(|p| match p.position {
        Position::Next => {
            next += 1;
            next <= numbers
        }
        Position::At(at) => at < numbers,
        Position::Reading => true,
    })
}

/// `surface` with each placeholder filled by its function, given what it
/// takes from `reading` and its `numbers`, as typed. `None` when a
/// placeholder has nothing to take, or its function is missing or gives no
/// text.
pub(crate) fn fill(
    surface: &str,
    numbers: &[String],
    reading: &str,
    functions: Option<&dyn Functions>,
) -> Option<String> {
    let mut out = String::new();
    let mut next = numbers.iter();
    let mut rest = surface;
    while let Some(open) = rest.find(OPEN) {
        out.push_str(&rest[..open]);
        let inside = &rest[open + OPEN.len_utf8()..];
        let close = inside.find(CLOSE)?;
        let placeholder = Placeholder::parse(&inside[..close])?;
        let source = match placeholder.position {
            Position::Next => next.next()?,
            Position::At(at) => numbers.get(at)?,
            Position::Reading => reading,
        };
        let call = Call {
            name: placeholder.name,
            source,
            argument: placeholder.argument,
        };
        out.push_str(&match functions {
            Some(functions) if functions.has(call.name) => functions.call(&call)?,
            _ => builtin(call.name, call.source)?,
        });
        rest = &inside[close + CLOSE.len_utf8()..];
    }
    out.push_str(rest);
    Some(out)
}

/// The functions the IME has: the empty name gives its source back, and the
/// others write a number. None of them takes an argument.
fn builtin(name: &str, source: &str) -> Option<String> {
    if name.is_empty() {
        return Some(source.to_owned());
    }
    Notation::named(name)?.write(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked(inside: &str) -> String {
        format!("{OPEN}{inside}{CLOSE}")
    }

    #[test]
    fn a_position_and_a_function_are_split_at_the_colon() {
        let parsed = |inside| Placeholder::parse(inside).unwrap();
        assert_eq!(
            parsed("1:kanji"),
            Placeholder {
                position: Position::At(1),
                name: "kanji",
                argument: None
            }
        );
        assert_eq!(parsed(":kanji").position, Position::Next);
        assert_eq!(parsed("-:date").position, Position::Reading);
        assert_eq!(parsed("1:").name, "");
    }

    #[test]
    fn without_a_colon_the_whole_is_the_function() {
        let one = Placeholder::parse("1").unwrap();
        assert_eq!((one.position, one.name), (Position::Next, "1"));
        assert_eq!(Placeholder::parse("").unwrap().name, "");
    }

    #[test]
    fn the_argument_is_everything_past_the_first_space() {
        let date = Placeholder::parse("-:date %H:%M %S").unwrap();
        assert_eq!((date.name, date.argument), ("date", Some("%H:%M %S")));
        let plain = Placeholder::parse("date %Y:%m").unwrap();
        assert_eq!(
            (plain.position, plain.name, plain.argument),
            (Position::Next, "date", Some("%Y:%m"))
        );
        assert_eq!(Placeholder::parse("date ").unwrap().argument, Some(""));
    }

    #[test]
    fn a_position_must_be_digits_a_dash_or_nothing() {
        assert_eq!(Placeholder::parse("x:kanji"), None);
        assert_eq!(Placeholder::parse("1-:kanji"), None);
        assert_eq!(Placeholder::parse("a:b:c"), None);
    }

    #[test]
    fn a_placeholder_fits_when_the_reading_has_its_numbers() {
        let surface = format!("{}{}", marked("kanji"), marked("1:"));
        assert!(fits(&surface, 2));
        assert!(!fits(&surface, 1));
        assert!(!fits(&marked("kanji"), 0));
        assert!(fits(&marked("-:date"), 0));
        assert!(fits("個", 0));
    }

    #[test]
    fn placeholders_without_a_position_take_the_numbers_in_turn() {
        let surface = format!("{}月{}日{}", marked("1:"), marked("kanji"), marked(""));
        let numbers = ["12".to_owned(), "1".to_owned()];
        assert_eq!(
            fill(&surface, &numbers, "", None).as_deref(),
            Some("1月十二日1")
        );
    }

    #[test]
    fn the_empty_function_gives_the_number_as_typed() {
        let numbers = ["１２".to_owned()];
        assert_eq!(
            fill(&marked(""), &numbers, "", None).as_deref(),
            Some("１２")
        );
    }

    #[test]
    fn a_dash_hands_the_whole_reading() {
        assert_eq!(
            fill(&format!("「{}」", marked("-:")), &[], "きょう", None).as_deref(),
            Some("「きょう」")
        );
    }

    #[test]
    fn nothing_is_filled_without_a_number_or_a_function() {
        assert_eq!(fill(&marked("kanji"), &[], "", None), None);
        assert_eq!(fill(&marked("3:"), &["1".to_owned()], "", None), None);
        assert_eq!(fill(&marked("-:missing"), &[], "よみ", None), None);
        assert_eq!(fill(&marked("-:kanji"), &[], "よみ", None), None);
    }

    struct Shout;

    impl Functions for Shout {
        fn has(&self, name: &str) -> bool {
            matches!(name, "shout" | "kanji")
        }

        fn call(&self, call: &Call) -> Option<String> {
            match call.name {
                "shout" => Some(format!("{}{}", call.source, call.argument.unwrap_or("!"))),
                _ => None,
            }
        }
    }

    #[test]
    fn a_user_function_takes_its_source_and_argument() {
        let shout = |surface: &str| fill(surface, &["1".to_owned()], "よみ", Some(&Shout));
        assert_eq!(shout(&marked("-:shout")).as_deref(), Some("よみ!"));
        assert_eq!(shout(&marked("shout ?!")).as_deref(), Some("1?!"));
    }

    #[test]
    fn a_user_function_goes_in_place_of_the_builtin_of_its_name() {
        let numbers = ["1".to_owned()];
        assert_eq!(fill(&marked("kanji"), &numbers, "", Some(&Shout)), None);
        assert_eq!(
            fill(&marked("wide-num"), &numbers, "", Some(&Shout)).as_deref(),
            Some("１")
        );
    }

    #[test]
    fn a_surface_without_placeholders_stays_as_written() {
        assert_eq!(fill("{個}", &[], "", None).as_deref(), Some("{個}"));
    }
}
