use std::fs;
use std::io;
use std::path::Path;

use kanaemi_core::RomajiTable;

use crate::ROMAJI_DIR;
use crate::folder::stays_inside;

/// A romaji table file shipped with Kanaemi, by the name the settings use.
pub(crate) struct BundledRomajiTable {
    pub(crate) name: &'static str,
    pub(crate) text: &'static str,
}

pub(crate) const BUNDLED_ROMAJI_TABLES: &[BundledRomajiTable] = &[
    BundledRomajiTable {
        name: "full-width",
        text: include_str!("../assets/romaji/full-width.tsv"),
    },
    BundledRomajiTable {
        name: "hepburn",
        text: include_str!("../assets/romaji/hepburn.tsv"),
    },
    BundledRomajiTable {
        name: "kunrei",
        text: include_str!("../assets/romaji/kunrei.tsv"),
    },
    BundledRomajiTable {
        name: "input-aids",
        text: include_str!("../assets/romaji/input-aids.tsv"),
    },
    BundledRomajiTable {
        name: "z-symbols",
        text: include_str!("../assets/romaji/z-symbols.tsv"),
    },
    BundledRomajiTable {
        name: "azik",
        text: include_str!("../assets/romaji/azik.tsv"),
    },
];

/// The bundled tables stacked when the settings choose none, in order.
/// Full-width ASCII comes first, so the Japanese punctuation stacked above it wins.
pub(crate) const DEFAULT_ROMAJI_TABLES: &[&str] =
    &["full-width", "hepburn", "kunrei", "input-aids", "z-symbols"];

/// The names of the bundled romaji tables, in the order to list them.
pub fn bundled_romaji_tables() -> impl Iterator<Item = &'static str> {
    BUNDLED_ROMAJI_TABLES.iter().map(|t| t.name)
}

/// The names of the romaji tables stacked when the settings choose none, in
/// order.
pub fn default_romaji_tables() -> &'static [&'static str] {
    DEFAULT_ROMAJI_TABLES
}

/// The text of the romaji table `name`: the bundled table of that name, or
/// else the file of that name in `dir`'s romaji folder, which a
/// name may not leave.
pub fn read_romaji_table(dir: impl AsRef<Path>, name: impl AsRef<str>) -> io::Result<String> {
    let name = name.as_ref();
    match BUNDLED_ROMAJI_TABLES.iter().find(|t| t.name == name) {
        Some(bundled) => Ok(bundled.text.to_owned()),
        None if !stays_inside(name) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} is outside the romaji folder"),
        )),
        None => fs::read_to_string(dir.as_ref().join(ROMAJI_DIR).join(name)),
    }
}

/// The romaji table stacked from the bundled tables in the default order.
pub fn default_romaji_table() -> RomajiTable {
    let mut table = RomajiTable::empty();
    let bundled = DEFAULT_ROMAJI_TABLES
        .iter()
        .filter_map(|name| BUNDLED_ROMAJI_TABLES.iter().find(|t| t.name == *name));
    for t in bundled {
        table.apply(t.text);
    }
    table
}

#[cfg(test)]
mod tests {
    use kanaemi_core::{
        Action, Candidate, Config, Converter, Core, Event, Key, KeyEvent, KeyKind, Modifiers,
    };

    use super::*;

    struct NoWords;

    impl Converter for NoWords {
        fn convert(&self, _reading: &str, _okurigana: Option<&str>) -> Vec<Candidate> {
            Vec::new()
        }
    }

    /// The bundled tables stacked in order; every line of them must be valid.
    fn bundled(names: &[&str]) -> RomajiTable {
        let mut table = RomajiTable::empty();
        for name in names {
            let bundled = BUNDLED_ROMAJI_TABLES
                .iter()
                .find(|t| t.name == *name)
                .unwrap_or_else(|| panic!("no bundled table {name}"));
            assert_eq!(table.apply(bundled.text), Vec::<usize>::new(), "{name}");
        }
        table
    }

    /// The default tables with AZIK stacked on top.
    fn with_azik() -> RomajiTable {
        let mut names = DEFAULT_ROMAJI_TABLES.to_vec();
        names.push("azik");
        bundled(&names)
    }

    /// What typing `input` in kana mode commits. No key begins a reading,
    /// so `;` reaches the table as AZIK users set it up.
    fn typed_with(romaji: RomajiTable, input: &str) -> String {
        let mut config = Config {
            romaji,
            ..Config::default()
        };
        config.bindings.kana.retain(|b| b.to != Action::Begin);
        let mut core = Core::new(NoWords, config);
        core.handle(Event::FocusIn { password: false });
        // A right Shift tap, the default way into kana mode.
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        core.handle(Event::Key(KeyEvent {
            key: Key::ShiftRight,
            mods: shift,
            kind: KeyKind::Press,
            time_ms: 0,
        }));
        core.handle(Event::Key(KeyEvent {
            key: Key::ShiftRight,
            mods: Modifiers::default(),
            kind: KeyKind::Release,
            time_ms: 10,
        }));
        let mut typed = String::new();
        for c in input.chars() {
            let out = core.handle(Event::Key(KeyEvent {
                key: Key::Char(c),
                mods: Modifiers::default(),
                kind: KeyKind::Press,
                time_ms: 0,
            }));
            typed.push_str(out.commit.as_deref().unwrap_or(""));
        }
        typed
    }

    #[test]
    fn the_default_stacks_the_standard_tables_in_order() {
        assert_eq!(
            DEFAULT_ROMAJI_TABLES,
            ["full-width", "hepburn", "kunrei", "input-aids", "z-symbols"]
        );
        assert_eq!(bundled(DEFAULT_ROMAJI_TABLES), default_romaji_table());
    }

    #[test]
    fn every_bundled_table_is_valid() {
        let names: Vec<&str> = BUNDLED_ROMAJI_TABLES.iter().map(|t| t.name).collect();
        bundled(&names);
    }

    #[test]
    fn the_hepburn_table_alone_does_not_take_kunrei_spellings() {
        assert_eq!(typed_with(bundled(&["hepburn"]), "shi"), "し");
        assert_eq!(typed_with(bundled(&["hepburn"]), "si"), "い");
    }

    #[test]
    fn the_kunrei_table_alone_does_not_take_hepburn_spellings() {
        assert_eq!(typed_with(bundled(&["kunrei"]), "si"), "し");
        assert_eq!(typed_with(bundled(&["kunrei"]), "shi"), "ひ");
    }

    #[test]
    fn the_input_aid_table_types_what_neither_system_can() {
        let t = bundled(&["hepburn", "input-aids"]);
        assert_eq!(typed_with(t, "diduwoxa,"), "ぢづをぁ、");
    }

    #[test]
    fn the_full_width_table_types_every_ascii_symbol_full_width() {
        assert_eq!(typed_with(bundled(&["full-width"]), "1@,\\"), "１＠，＼");
    }

    #[test]
    fn the_z_symbol_table_types_arrows_and_brackets() {
        let t = bundled(&["z-symbols"]);
        assert_eq!(typed_with(t.clone(), "zhzjzkzl"), "←↓↑→");
        assert_eq!(typed_with(t, "z-z.z,z/z[z]"), "〜…‥・『』");
    }

    #[test]
    fn the_input_aids_stacked_over_the_full_width_table_win() {
        let t = bundled(&["full-width", "input-aids"]);
        assert_eq!(typed_with(t, ",1"), "、１");
    }

    #[test]
    fn azik_types_extensions_and_still_takes_plain_romaji() {
        assert_eq!(typed_with(with_azik(), "kzsq;te:"), "かんさいってー");
        assert_eq!(typed_with(with_azik(), "kgpxa"), "きょうしゃ");
        assert_eq!(typed_with(with_azik(), "shimn"), "しもの");
    }

    #[test]
    fn the_azik_table_says_to_unbind_semicolon_from_begin() {
        let azik = BUNDLED_ROMAJI_TABLES
            .iter()
            .find(|t| t.name == "azik")
            .expect("the AZIK table");
        let header: Vec<&str> = azik
            .text
            .lines()
            .take_while(|l| l.starts_with('#'))
            .collect();
        assert!(
            header
                .iter()
                .any(|l| l.contains(';') && l.contains("@begin")),
            "{header:?}"
        );
    }

    #[test]
    fn azik_types_small_kana_with_l_and_takes_z_symbols_it_does_not_redefine() {
        assert_eq!(typed_with(with_azik(), "laz."), "ぁ…");
        assert_eq!(typed_with(with_azik(), "zl"), "ぞん");
    }
}
