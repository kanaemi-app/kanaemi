//! The dictionaries Kanaemi ships with: words whose values the built-in
//! functions work out each time, so they need none of the analysis the
//! distributed dictionaries are made with.

/// How the dictionary list names a built-in dictionary, before its name.
pub const BUILTIN_PREFIX: &str = "builtin:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltinDictionary {
    pub name: &'static str,
    /// A text dictionary.
    pub text: &'static str,
}

/// In the order they follow the other dictionaries when none are listed.
pub const BUILTIN_DICTIONARIES: &[BuiltinDictionary] = &[
    BuiltinDictionary {
        name: "date",
        text: include_str!("../assets/dictionaries/date.tsv"),
    },
    BuiltinDictionary {
        name: "time",
        text: include_str!("../assets/dictionaries/time.tsv"),
    },
    BuiltinDictionary {
        name: "year",
        text: include_str!("../assets/dictionaries/year.tsv"),
    },
    BuiltinDictionary {
        name: "random",
        text: include_str!("../assets/dictionaries/random.tsv"),
    },
];

pub fn builtin_dictionary(name: impl AsRef<str>) -> Option<&'static BuiltinDictionary> {
    BUILTIN_DICTIONARIES
        .iter()
        .find(|dictionary| dictionary.name == name.as_ref())
}
