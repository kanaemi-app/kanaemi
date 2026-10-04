use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Most kana that may follow a stem.
pub const MAX_SUFFIX_KANA: usize = 4;

/// The ending of `conjugation`'s terminal form, which a dictionary form ends
/// with (五段-カ行 → く, 下一段-バ行 → る, サ行変格 → する). `None` for a type
/// the table does not know.
pub fn terminal_ending(conjugation: impl AsRef<str>) -> Option<&'static str> {
    ConjugationTable::builtin()
        .terminals
        .get(conjugation.as_ref())
        .map(String::as_str)
}

/// Whether `reading` may follow a stem of `conjugation`, whole or as the
/// start of what may follow (書い as well as 書いた).
pub fn may_follow_stem(conjugation: impl AsRef<str>, reading: impl AsRef<str>) -> bool {
    ConjugationTable::builtin()
        .prefixes(conjugation.as_ref())
        .is_some_and(|starts| starts.contains(reading.as_ref()))
}

struct Ending {
    form: String,
    text: String,
    ends_word: bool,
    voices_next: bool,
}

struct Suffix {
    text: String,
    voiced: String,
    after: Vec<String>,
    conjugation: Option<String>,
}

/// The readings each conjugation type allows after a stem, built once from
/// the bundled endings and the auxiliaries and particles that attach to them.
pub(crate) struct ConjugationTable {
    suffixes: HashMap<String, HashSet<String>>,
    prefixes: HashMap<String, HashSet<String>>,
    terminals: HashMap<String, String>,
}

static BUILTIN: LazyLock<ConjugationTable> = LazyLock::new(|| {
    ConjugationTable::build(
        include_str!("../assets/conjugation/endings.tsv"),
        include_str!("../assets/conjugation/suffixes.tsv"),
    )
});

impl ConjugationTable {
    pub(crate) fn builtin() -> &'static Self {
        &BUILTIN
    }

    pub(crate) fn suffixes(&self, conjugation: &str) -> Option<&HashSet<String>> {
        self.suffixes.get(conjugation)
    }

    /// Readings that start an allowed suffix: what may follow a stem while
    /// the word is still being typed, as at an okurigana conversion (書い).
    pub(crate) fn prefixes(&self, conjugation: &str) -> Option<&HashSet<String>> {
        self.prefixes.get(conjugation)
    }

    /// Panics on a malformed line: the data ships with Kanaemi and its tests
    /// load it, so a bad line never reaches a user.
    fn build(endings: &str, suffixes: &str) -> Self {
        let mut types: HashMap<String, Vec<Ending>> = HashMap::new();
        for fields in data_lines(endings) {
            let [conjugation, form, text, marks] = fields[..] else {
                panic!("conjugation ending: {fields:?}");
            };
            types
                .entry(conjugation.to_owned())
                .or_default()
                .push(Ending {
                    form: form.to_owned(),
                    text: text.to_owned(),
                    ends_word: marks.contains('終'),
                    voices_next: marks.contains('濁'),
                });
        }
        let attached: Vec<Suffix> = data_lines(suffixes)
            .map(|fields| {
                let [text, voiced, after, conjugation] = fields[..] else {
                    panic!("conjugation suffix: {fields:?}");
                };
                assert!(
                    conjugation.is_empty() || types.contains_key(conjugation),
                    "conjugation suffix with unknown type: {fields:?}"
                );
                Suffix {
                    text: text.to_owned(),
                    voiced: if voiced.is_empty() { text } else { voiced }.to_owned(),
                    after: after.split(',').map(str::to_owned).collect(),
                    conjugation: Some(conjugation)
                        .filter(|c| !c.is_empty())
                        .map(str::to_owned),
                }
            })
            .collect();
        let suffixes: HashMap<String, HashSet<String>> = types
            .keys()
            .map(|name| {
                let mut out = HashSet::new();
                expand(&types, &attached, name, String::new(), &mut out);
                (name.clone(), out)
            })
            .collect();
        let prefixes = suffixes
            .iter()
            .map(|(name, set): (&String, &HashSet<String>)| {
                let starts = set
                    .iter()
                    .flat_map(|s| {
                        s.char_indices()
                            .map(|(i, _)| s[..i].to_owned())
                            .chain([s.clone()])
                    })
                    .collect();
                (name.clone(), starts)
            })
            .collect();
        let terminals = types
            .iter()
            .filter_map(|(name, endings)| {
                endings
                    .iter()
                    .find(|e| e.form == "終止形" && e.ends_word)
                    .map(|e| (name.clone(), e.text.clone()))
            })
            .collect();
        Self {
            suffixes,
            prefixes,
            terminals,
        }
    }
}

fn expand(
    types: &HashMap<String, Vec<Ending>>,
    attached: &[Suffix],
    conjugation: &str,
    prefix: String,
    out: &mut HashSet<String>,
) {
    for ending in &types[conjugation] {
        let base = format!("{prefix}{}", ending.text);
        if base.chars().count() > MAX_SUFFIX_KANA {
            continue;
        }
        if ending.ends_word {
            out.insert(base.clone());
        }
        for suffix in attached.iter().filter(|s| s.after.contains(&ending.form)) {
            let text = if ending.voices_next {
                &suffix.voiced
            } else {
                &suffix.text
            };
            let joined = format!("{base}{text}");
            if joined.chars().count() > MAX_SUFFIX_KANA {
                continue;
            }
            match &suffix.conjugation {
                Some(next) => expand(types, attached, next, joined, out),
                None => {
                    out.insert(joined);
                }
            }
        }
    }
}

fn data_lines(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allows(conjugation: &str, suffix: &str) -> bool {
        ConjugationTable::builtin()
            .suffixes(conjugation)
            .unwrap_or_else(|| panic!("unknown conjugation {conjugation}"))
            .contains(suffix)
    }

    #[track_caller]
    fn assert_allows(conjugation: &str, suffixes: &[&str]) {
        for suffix in suffixes {
            assert!(allows(conjugation, suffix), "{conjugation} + {suffix}");
        }
    }

    #[track_caller]
    fn assert_rejects(conjugation: &str, suffixes: &[&str]) {
        for suffix in suffixes {
            assert!(!allows(conjugation, suffix), "{conjugation} + {suffix}");
        }
    }

    #[test]
    fn godan_ka() {
        assert_allows(
            "五段-カ行",
            &[
                "く",
                "き",
                "け",
                "かない",
                "いた",
                "いて",
                "いたら",
                "こう",
                "けば",
                "きます",
                "きたい",
                "いたり",
                "きながら",
            ],
        );
        assert_rejects("五段-カ行", &["", "か", "いだ", "かなかった", "った"]);
    }

    #[test]
    fn godan_ga_voices_ta_and_te() {
        assert_allows("五段-ガ行", &["いだ", "いで", "いだり", "がない"]);
        assert_rejects("五段-ガ行", &["いた", "いて"]);
    }

    #[test]
    fn godan_with_n_onbin_voices_ta() {
        for conjugation in ["五段-ナ行", "五段-バ行", "五段-マ行"] {
            assert_allows(conjugation, &["んだ", "んで"]);
            assert_rejects(conjugation, &["んた", "った"]);
        }
    }

    #[test]
    fn godan_with_sokuonbin() {
        for conjugation in ["五段-タ行", "五段-ラ行", "五段-ワア行", "五段-カ行-促音便"]
        {
            assert_allows(conjugation, &["った", "って"]);
        }
        assert_allows("五段-ワア行", &["わない", "う", "おう"]);
        assert_rejects("五段-カ行-促音便", &["いた"]);
        assert_allows("五段-カ行-促音便", &["かない", "きます"]);
    }

    #[test]
    fn godan_sa_and_u_onbin() {
        assert_allows("五段-サ行", &["した", "して", "さない"]);
        assert_allows("五段-ワア行-ウ音便", &["うた", "うて", "わない"]);
        assert_rejects("五段-ワア行-ウ音便", &["った"]);
    }

    #[test]
    fn godan_ra_special() {
        assert_allows("五段-ラ行-特殊", &["い", "います", "った", "らない", "る"]);
    }

    #[test]
    fn ichidan_stems_end_in_their_row() {
        for conjugation in ["下一段-バ行", "上一段-マ行", "上一段-カ行"] {
            assert_allows(
                conjugation,
                &[
                    "",
                    "る",
                    "た",
                    "て",
                    "ない",
                    "よう",
                    "ろ",
                    "れば",
                    "ます",
                    "たい",
                    "なかった",
                    "ません",
                ],
            );
            assert_rejects(conjugation, &["だ", "らない"]);
        }
    }

    #[test]
    fn sahen() {
        assert_allows(
            "サ行変格",
            &[
                "する",
                "した",
                "して",
                "しない",
                "しよう",
                "すれば",
                "しろ",
                "します",
            ],
        );
        assert_rejects("サ行変格", &["", "す"]);
    }

    #[test]
    fn adjectives() {
        assert_allows(
            "形容詞",
            &[
                "い",
                "く",
                "かった",
                "くて",
                "ければ",
                "かろう",
                "くない",
                "かったら",
            ],
        );
        assert_rejects("形容詞", &["くます", "かって", "くた"]);
    }

    #[test]
    fn suffixes_are_at_most_four_kana() {
        let table = ConjugationTable::builtin();
        for conjugation in table.suffixes.keys() {
            for suffix in table.suffixes(conjugation).unwrap() {
                assert!(
                    suffix.chars().count() <= MAX_SUFFIX_KANA,
                    "{conjugation} + {suffix}"
                );
            }
        }
    }

    #[test]
    fn unknown_types_are_not_in_the_table() {
        assert!(
            ConjugationTable::builtin()
                .suffixes("存在しない型")
                .is_none()
        );
    }

    #[test]
    fn sahen_negates_with_shi_nai_and_sen() {
        assert_allows("サ行変格", &["しない", "せん"]);
        assert_rejects("サ行変格", &["しん", "せない"]);
    }

    #[test]
    fn godan_ra_special_takes_i_only_before_masu() {
        assert_allows("五段-ラ行-特殊", &["います", "りたい", "りながら"]);
        assert_rejects("五段-ラ行-特殊", &["いたい", "いながら"]);
    }

    #[test]
    fn godan_ra_special_does_not_take_masu_after_ri() {
        assert_rejects("五段-ラ行-特殊", &["ります", "りました", "りません"]);
        assert_allows("五段-ラ行-特殊", &["り"]);
    }

    #[test]
    fn passive_and_causative_attach_by_the_kind_of_verb() {
        // れる・せる after the godan irrealis, られる・させる after the ichidan one.
        assert_allows("五段-ワア行", &["われて", "われない", "わせて"]);
        assert_allows("五段-カ行", &["かれた", "かせ"]);
        assert_allows("五段-サ行", &["される"]);
        assert_allows("五段-バ行", &["ばれる"]);
        assert_allows("五段-ラ行", &["らせて"]);
        assert_allows("五段-マ行", &["ませて"]);
        assert_allows("下一段-ア行", &["られる", "させて"]);
        assert_allows("下一段-マ行", &["られて"]);
        assert_allows("上一段-マ行", &["られる"]);
        assert_allows("下一段-ガ行", &["られる", "させて"]);
        assert_allows("下一段-ダ行", &["させて"]);
        assert_allows("サ行変格", &["される", "させる", "せず", "すべき"]);
    }

    #[test]
    fn passive_and_causative_never_cross_the_kinds() {
        assert_rejects("五段-カ行", &["かられる", "かさせる"]);
        assert_rejects("下一段-バ行", &["れる", "せる"]);
    }

    #[test]
    fn negatives_ず_ぬ_ざる_and_their_casual_forms() {
        assert_allows("五段-ワア行", &["わず", "わざる"]);
        assert_allows("下一段-ア行", &["ず", "ないで", "なきゃ"]);
        assert_allows("五段-ラ行", &["らず", "らねえ", "らね"]);
        assert_allows("下一段-ラ行", &["ぬ"]);
        assert_allows("形容詞", &["からぬ"]);
        assert_allows("五段-ワア行", &["わないで"]);
    }

    #[test]
    fn べき_after_the_terminal_form() {
        assert_allows("五段-サ行", &["すべき"]);
        assert_allows("五段-ラ行", &["るべき", "るべし"]);
        assert_allows("五段-マ行", &["むべき"]);
    }

    #[test]
    fn ている_contracted() {
        assert_allows("五段-ラ行", &["ってる"]);
        assert_allows("五段-カ行", &["いてる"]);
        assert_allows("五段-マ行", &["んでる"]);
        assert_allows("下一段-ア行", &["てる"]);
    }

    #[test]
    fn そう_of_appearance_after_the_continuative_and_the_adjective_stem() {
        assert_allows("下一段-バ行", &["そう", "そうな", "そうに", "そうだ"]);
        assert_allows("五段-カ行", &["きそう", "きそうな"]);
        assert_allows("五段-ラ行-特殊", &["りそう"]);
        assert_allows("サ行変格", &["しそう"]);
        assert_allows("形容詞", &["そう", "そうな", "そうに"]);
        assert_rejects("五段-カ行", &["くそう", "かそう"]);
    }

    #[test]
    fn そうだ_of_hearsay_after_the_terminal_form() {
        assert_allows("下一段-バ行", &["るそうだ", "たそうだ"]);
        assert_allows("形容詞", &["いそうだ"]);
        assert_rejects("下一段-バ行", &["るそう", "るそうな"]);
    }

    #[test]
    fn らしい_after_the_terminal_form_conjugates_without_a_stem_alone() {
        assert_allows("下一段-バ行", &["るらしい", "るらしく", "たらしい"]);
        assert_allows("五段-カ行", &["くらしい"]);
        assert_allows("形容詞", &["いらしい"]);
        assert_rejects("下一段-バ行", &["るらし", "るらしう"]);
    }

    #[test]
    fn みたい_after_the_terminal_form() {
        assert_allows("下一段-バ行", &["るみたい", "たみたい"]);
        assert_allows("形容詞", &["いみたい"]);
        assert_rejects("五段-カ行", &["きみたい"]);
    }

    #[test]
    fn ても_after_the_te_form() {
        assert_allows("下一段-バ行", &["ても", "なくても"]);
        assert_allows("五段-ガ行", &["いでも"]);
        assert_allows("形容詞", &["くても"]);
        assert_rejects("五段-ガ行", &["いても"]);
    }

    #[test]
    fn ている_uncontracted() {
        assert_allows("下一段-バ行", &["ている", "ていた", "ていない", "ていて"]);
        assert_allows("五段-カ行", &["いている", "いていた"]);
        assert_allows("五段-ガ行", &["いでいる", "いでいた"]);
        assert_rejects("下一段-バ行", &["ていました", "ていません"]);
        assert_rejects("形容詞", &["くている"]);
    }

    #[test]
    fn adjectives_alone_with_です_and_in_the_u_euphony() {
        assert_allows("形容詞", &["", "いです", "いでした", "う", "ゅう"]);
    }

    #[test]
    fn だろう_and_なら_after_the_terminal_form() {
        assert_allows("形容詞", &["いだろう"]);
        assert_allows("五段-ラ行", &["るだろう"]);
        assert_allows("五段-ワア行", &["うなら"]);
    }

    #[test]
    fn ない_and_たい_have_no_stem_alone_or_u_euphony() {
        assert_allows("五段-カ行", &["かない", "きたい"]);
        assert_allows("下一段-ア行", &["ないです", "たいです"]);
        assert_rejects("五段-カ行", &["かな", "かなう", "きた"]);
    }

    #[test]
    fn verb_only_attachments_stay_off_adjectives_and_polite_forms() {
        assert_rejects("形容詞", &["くてる", "いべき", "いでせん", "いでせ"]);
        assert_rejects("下一段-バ行", &["ますべき"]);
        assert_allows("形容詞", &["くねえ"]);
    }

    #[test]
    fn the_casual_negative_follows_ない() {
        assert_allows("サ行変格", &["しねえ"]);
        assert_rejects("サ行変格", &["せねえ"]);
        assert_allows("サ行変格", &["するべき"]);
    }

    #[test]
    fn ないで_is_for_verbs_and_なきゃ_for_adjectives_too() {
        assert_rejects("形容詞", &["くないで"]);
        assert_allows("形容詞", &["くなきゃ"]);
        assert_allows("サ行変格", &["しないで"]);
    }

    #[test]
    fn a_type_whose_stem_reading_changes_is_not_in_the_table() {
        assert!(ConjugationTable::builtin().suffixes("カ行変格").is_none());
    }

    #[test]
    fn a_suffix_is_at_most_max_suffix_kana_long() {
        assert_eq!(MAX_SUFFIX_KANA, 4);
    }

    #[test]
    fn a_dictionary_form_ends_with_the_terminal_ending() {
        assert_eq!(terminal_ending("五段-カ行"), Some("く"));
        assert_eq!(terminal_ending("五段-ワア行"), Some("う"));
        assert_eq!(terminal_ending("下一段-バ行"), Some("る"));
        assert_eq!(terminal_ending("上一段-マ行"), Some("る"));
        assert_eq!(terminal_ending("サ行変格"), Some("する"));
        assert_eq!(terminal_ending("形容詞"), Some("い"));
        assert_eq!(terminal_ending(String::from("五段-ラ行")), Some("る"));
    }

    #[test]
    fn an_unknown_type_has_no_terminal_ending() {
        assert_eq!(terminal_ending("存在しない型"), None);
        assert_eq!(terminal_ending("カ行変格"), None);
    }

    #[test]
    fn a_reading_may_follow_a_stem_whole_or_as_its_start() {
        assert!(may_follow_stem("五段-カ行", "いた"));
        assert!(may_follow_stem("五段-カ行", "い"));
        assert!(may_follow_stem("下一段-バ行", ""));
        assert!(may_follow_stem("サ行変格", String::from("し")));
        assert!(!may_follow_stem("五段-カ行", "った"));
        assert!(!may_follow_stem("サ行変格", "せない"));
    }

    #[test]
    fn nothing_may_follow_a_stem_of_an_unknown_type() {
        assert!(!may_follow_stem("存在しない型", ""));
        assert!(!may_follow_stem("カ行変格", "る"));
    }
}
