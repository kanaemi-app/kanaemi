use std::fs::File;
use std::io::Read;
use std::path::Path;

/// How much of the field's committed text a conversion looks back on.
pub const CONTEXT_CHARS: usize = 16;
/// How many commits the history keeps, and the model counts within.
pub const HISTORY_LEN: usize = 300;

const MAGIC: &[u8; 8] = b"KANAEMIM";
/// The format version of the model files this engine reads.
pub const MODEL_FORMAT_VERSION: u32 = 3;
const HEADER_LEN: usize = 32;
const BITS: std::ops::RangeInclusive<u8> = 10..=28;
/// The length of a file of the most weights, each of the widest type.
const MAX_LEN: u64 = HEADER_LEN as u64 + (4 << *BITS.end());
/// Joins a feature's name and values; no reading or surface holds it.
const SEPARATOR: char = '\u{1f}';

/// What one conversion is ranked against.
#[derive(Clone, Copy, Debug)]
pub struct RankingInput<'a> {
    pub reading: &'a str,
    /// The field's commits, oldest first, at most [`HISTORY_LEN`].
    pub history: &'a [(String, String)],
    /// The end of the text committed to the field, at most [`CONTEXT_CHARS`].
    pub context: &'a str,
}

/// A candidate with what the model knows of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateFacts {
    pub surface: String,
    /// The place of its dictionary in the list, from 0.
    pub dictionary: usize,
    pub cost: u32,
    /// Built from a conjugating stem rather than found whole.
    pub built: bool,
    /// The reading and surface of the item with placeholders it was filled
    /// from.
    pub template: Option<(String, String)>,
}

impl CandidateFacts {
    /// A word found whole in the first dictionary at no cost.
    pub(crate) fn plain(surface: String) -> Self {
        Self {
            surface,
            dictionary: 0,
            cost: 0,
            built: false,
            template: None,
        }
    }

    /// The (reading, surface) the history, the picks and hide lines know it
    /// by, `reading` being what was converted: the item with placeholders it was
    /// filled from, so what is learned with one number or value holds for
    /// every one.
    pub fn recorded<'a>(&'a self, reading: &'a str) -> (&'a str, &'a str) {
        match &self.template {
            Some((reading, surface)) => (reading, surface),
            None => (reading, &self.surface),
        }
    }
}

/// One part of a feature: its name or one of its values.
enum Part<'a> {
    Text(&'a str),
    Number(usize),
    Char(char),
}

impl Part<'_> {
    fn write_to(&self, out: &mut Vec<u8>) {
        match self {
            Part::Text(s) => out.extend_from_slice(s.as_bytes()),
            Part::Number(n) => {
                use std::io::Write;
                write!(out, "{n}").expect("writing to memory cannot fail");
            }
            Part::Char(c) => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }
}

/// Where the weights of a candidate's features are, among `2^bits`: what
/// scoring and training both call for every candidate, so the model sees the
/// features it learned from. A feature is its name and values joined by
/// U+001F (`s\u{1f}記者`).
///
/// # Panics
///
/// When `bits` is out of the range a model may have.
pub fn feature_indices(input: &RankingInput, candidate: &CandidateFacts, bits: u8) -> Vec<usize> {
    assert!(BITS.contains(&bits), "the hash bits are out of range");
    let mut out = Vec::new();
    let mut bytes = Vec::new();
    each_feature(input, candidate, |parts| {
        join(parts, &mut bytes);
        out.push(index_of(&bytes, bits));
    });
    out
}

#[cfg(test)]
/// Where a feature's weight is, among `2^bits`.
pub(crate) fn feature_index(feature: &str, bits: u8) -> usize {
    index_of(feature.as_bytes(), bits)
}

fn index_of(feature: &[u8], bits: u8) -> usize {
    (xxhash_rust::xxh3::xxh3_64(feature) & ((1u64 << bits) - 1)) as usize
}

/// Writes a feature's parts into `out`, joined by [`SEPARATOR`].
fn join(parts: &[Part], out: &mut Vec<u8>) {
    out.clear();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(SEPARATOR.encode_utf8(&mut [0; 4]).as_bytes());
        }
        part.write_to(out);
    }
}

/// The one definition of the features: each goes to `emit` as its parts. A
/// candidate is known by its [recorded](CandidateFacts::recorded) pair, the
/// form the history holds, so a filled candidate's features are its item's.
fn each_feature(input: &RankingInput, candidate: &CandidateFacts, mut emit: impl FnMut(&[Part])) {
    use Part::{Char, Number, Text};
    let (reading, s) = candidate.recorded(input.reading);
    let cost = band(candidate.cost as usize);
    emit(&[Text("s"), Text(s)]);
    emit(&[Text("rs"), Text(reading), Text(s)]);
    emit(&[Text("c"), Number(cost)]);
    emit(&[Text("dc"), Number(candidate.dictionary), Number(cost)]);
    emit(&[Text("x"), Text(flag(candidate.built))]);

    let history = input.history;
    let last = history
        .iter()
        .rev()
        .find(|(r, _)| r == reading)
        .is_some_and(|(_, previous)| previous == s);
    let count_within = |n: usize| {
        history
            .iter()
            .rev()
            .take(n)
            .filter(|(_, previous)| previous == s)
            .count()
    };
    let distance = history.iter().rev().position(|(_, previous)| previous == s);
    emit(&[Text("hl"), Text(flag(last))]);
    emit(&[Text("hr"), Text(flag(distance.is_some()))]);
    emit(&[Text("hn"), Number(band(count_within(100)))]);
    emit(&[Text("hm"), Number(band(count_within(HISTORY_LEN)))]);
    match distance {
        Some(d) => emit(&[Text("hd"), Number(band(d))]),
        None => emit(&[Text("hd"), Text("-")]),
    }
    emit(&[Text("hc"), Number(band(history.len())), Text(s)]);
    if let Some((_, previous)) = history.last() {
        emit(&[Text("p"), Text(previous), Text(s)]);
    }

    let context = input.context;
    let length = context.chars().count();
    let tail = |n: usize| -> &str {
        let start = context
            .char_indices()
            .nth(length - n)
            .map_or(context.len(), |(i, _)| i);
        &context[start..]
    };
    for (name, n) in [("a", 1), ("b", 2), ("t", 3)] {
        if length >= n {
            emit(&[Text(name), Text(tail(n)), Text(s)]);
        }
    }
    let mut seen: Vec<char> = Vec::new();
    for c in tail(length.min(8)).chars() {
        if !seen.contains(&c) {
            seen.push(c);
            emit(&[Text("g"), Char(c), Text(s)]);
        }
    }
}

/// `floor(log2(n + 1))`: counts and costs by their order of magnitude.
fn band(n: usize) -> usize {
    (n + 1).ilog2() as usize
}

fn flag(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

/// The weights a model file holds.
#[derive(Clone, Debug, PartialEq)]
enum Weights {
    F32(Vec<f32>),
    /// Each weight is the byte times `scale`.
    I8 {
        scale: f32,
        weights: Vec<i8>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("not a Kanaemi ranking model")]
    Magic,
    #[error("model format version {0} is not supported")]
    Version(u32),
    #[error("the model is malformed: {0}")]
    Malformed(&'static str),
    #[error("the weights do not match their checksum")]
    Checksum,
}

/// A linear model scoring candidates by their features.
pub struct RankingModel {
    bits: u8,
    weights: Weights,
}

impl RankingModel {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ModelError> {
        let file = File::open(path)?;
        // Refused before reading, so whatever lies in the model's place takes
        // no more memory than a model can; a file that grows meanwhile is cut
        // at that and refused for its length.
        if file.metadata()?.len() > MAX_LEN {
            return Err(ModelError::Malformed("larger than any model"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_LEN + 1).read_to_end(&mut bytes)?;
        Self::from_bytes(bytes)
    }

    fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, ModelError> {
        let b = bytes.as_ref();
        if b.len() < HEADER_LEN || &b[..8] != MAGIC {
            return Err(ModelError::Magic);
        }
        let version = u32::from_le_bytes(b[8..12].try_into().expect("four bytes"));
        if version != MODEL_FORMAT_VERSION {
            return Err(ModelError::Version(version));
        }
        let bits = b[12];
        if !BITS.contains(&bits) {
            return Err(ModelError::Malformed("the hash bits are out of range"));
        }
        if b[14..16] != [0, 0] || b[20..24] != [0; 4] {
            return Err(ModelError::Malformed("a reserved field is not zero"));
        }
        let scale = f32::from_le_bytes(b[16..20].try_into().expect("four bytes"));
        let checksum = u64::from_le_bytes(b[24..32].try_into().expect("eight bytes"));
        let body = &b[HEADER_LEN..];
        let count = 1usize << bits;
        let width = match b[13] {
            0 if scale != 1.0 => return Err(ModelError::Malformed("an f32 model scales by 1")),
            0 => 4,
            1 => 1,
            _ => return Err(ModelError::Malformed("unknown weight type")),
        };
        if body.len() != count * width {
            return Err(ModelError::Malformed("the weights are not 2^bits long"));
        }
        if xxhash_rust::xxh3::xxh3_64(body) != checksum {
            return Err(ModelError::Checksum);
        }
        let weights = match width {
            4 => Weights::F32(
                body.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|w| f32::from_le_bytes(*w))
                    .collect(),
            ),
            _ => Weights::I8 {
                scale,
                weights: body.iter().map(|&w| w as i8).collect(),
            },
        };
        Self::new(bits, weights)
    }

    /// A model of `2^bits` weights, each a finite number.
    fn new(bits: u8, weights: Weights) -> Result<Self, ModelError> {
        if !BITS.contains(&bits) {
            return Err(ModelError::Malformed("the hash bits are out of range"));
        }
        let (count, finite) = match &weights {
            Weights::F32(w) => (w.len(), w.iter().all(|w| w.is_finite())),
            Weights::I8 { scale, weights } => (weights.len(), scale.is_finite()),
        };
        if count != 1 << bits {
            return Err(ModelError::Malformed("the weights are not 2^bits long"));
        }
        if !finite {
            return Err(ModelError::Malformed("a weight is not a finite number"));
        }
        Ok(Self { bits, weights })
    }

    #[cfg(test)]
    /// The model as a file.
    fn to_bytes(&self) -> Vec<u8> {
        let bits = self.bits;
        let (kind, scale, body): (u8, f32, Vec<u8>) = match &self.weights {
            Weights::F32(w) => (0, 1.0, w.iter().flat_map(|w| w.to_le_bytes()).collect()),
            Weights::I8 { scale, weights } => {
                (1, *scale, weights.iter().map(|&w| w as u8).collect())
            }
        };
        let mut out = Vec::with_capacity(HEADER_LEN + body.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&MODEL_FORMAT_VERSION.to_le_bytes());
        out.push(bits);
        out.push(kind);
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&scale.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&xxhash_rust::xxh3::xxh3_64(&body).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    fn weight(&self, index: usize) -> f32 {
        match &self.weights {
            Weights::F32(w) => w[index],
            Weights::I8 { scale, weights } => f32::from(weights[index]) * scale,
        }
    }

    /// The candidate's score: the sum of the weights of its features.
    pub(crate) fn score_candidate(&self, input: &RankingInput, candidate: &CandidateFacts) -> f32 {
        feature_indices(input, candidate, self.bits)
            .into_iter()
            .map(|i| self.weight(i))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;

    use super::*;

    /// The features of a candidate as strings, whose indices
    /// [`feature_indices`] gives.
    fn features(input: &RankingInput, candidate: &CandidateFacts) -> Vec<String> {
        let mut out = Vec::new();
        let mut bytes = Vec::new();
        each_feature(input, candidate, |parts| {
            join(parts, &mut bytes);
            out.push(String::from_utf8(bytes.clone()).expect("parts are UTF-8"));
        });
        out
    }

    fn facts(surface: &str, dictionary: usize, cost: u32, built: bool) -> CandidateFacts {
        CandidateFacts {
            surface: surface.to_owned(),
            dictionary,
            cost,
            built,
            template: None,
        }
    }

    fn history(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(r, s)| ((*r).to_owned(), (*s).to_owned()))
            .collect()
    }

    fn names(features: &[String]) -> Vec<String> {
        features.iter().map(|f| f.replace('\u{1f}', "|")).collect()
    }

    #[test]
    fn a_candidate_without_history_or_context_has_its_own_features() {
        let input = RankingInput {
            reading: "きしゃ",
            history: &[],
            context: "",
        };
        assert_eq!(
            names(&features(&input, &facts("記者", 1, 6, false))),
            [
                "s|記者",
                "rs|きしゃ|記者",
                "c|2",
                "dc|1|2",
                "x|0",
                "hl|0",
                "hr|0",
                "hn|0",
                "hm|0",
                "hd|-",
                "hc|0|記者",
            ]
        );
    }

    #[test]
    fn history_and_context_add_their_features() {
        let history = history(&[("きしゃ", "汽車"), ("かんじ", "漢字"), ("きしゃ", "記者")]);
        let input = RankingInput {
            reading: "きしゃ",
            history: &history,
            context: "今日の漢字は",
        };
        let got = names(&features(&input, &facts("記者", 0, 0, true)));
        for expected in [
            "x|1",
            "hl|1",
            "hr|1",
            "hn|1",
            "hm|1",
            "hd|0",
            "hc|2|記者",
            "p|記者|記者",
            "a|は|記者",
            "b|字は|記者",
            "t|漢字は|記者",
            "g|今|記者",
            "g|は|記者",
        ] {
            assert!(got.contains(&expected.to_owned()), "{expected} in {got:?}");
        }
        let g = got.iter().filter(|f| f.starts_with("g|")).count();
        assert_eq!(g, 6, "each distinct character of the last eight once");
    }

    #[test]
    fn a_number_is_known_by_its_numeric_item_as_the_history_records_it() {
        let history = history(&[("{}こ", "{kanji}個")]);
        let input = RankingInput {
            reading: "5こ",
            history: &history,
            context: "箱が",
        };
        let candidate = CandidateFacts {
            template: Some(("{}こ".to_owned(), "{kanji}個".to_owned())),
            ..facts("五個", 0, 0, false)
        };
        let got = names(&features(&input, &candidate));
        for expected in [
            "s|{kanji}個",
            "rs|{}こ|{kanji}個",
            "hl|1",
            "hr|1",
            "hd|0",
            "hc|1|{kanji}個",
            "p|{kanji}個|{kanji}個",
            "a|が|{kanji}個",
            "g|箱|{kanji}個",
        ] {
            assert!(got.contains(&expected.to_owned()), "{expected} in {got:?}");
        }
    }

    #[test]
    fn a_model_file_reads_back_as_written() {
        let mut weights = vec![0.0f32; 1 << 10];
        weights[3] = 1.5;
        weights[7] = -0.25;
        let read = RankingModel::from_bytes(
            RankingModel::new(10, Weights::F32(weights))
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(read.weight(3), 1.5);
        assert_eq!(read.weight(7), -0.25);

        let quantized = Weights::I8 {
            scale: 0.5,
            weights: (0..1 << 10).map(|i| (i % 7) as i8 - 3).collect(),
        };
        let read =
            RankingModel::from_bytes(RankingModel::new(10, quantized).unwrap().to_bytes()).unwrap();
        assert_eq!(read.weight(0), -1.5);
        assert_eq!(read.weight(6), 1.5);
    }

    #[test]
    fn feature_indices_are_the_indices_of_the_features() {
        let history = history(&[("きしゃ", "汽車"), ("かんじ", "漢字"), ("きしゃ", "記者")]);
        let inputs = [
            RankingInput {
                reading: "きしゃ",
                history: &[],
                context: "",
            },
            RankingInput {
                reading: "きしゃ",
                history: &history,
                context: "今日の\n漢字は",
            },
            RankingInput {
                reading: "か",
                history: &history[..1],
                context: "a",
            },
        ];
        let candidates = [
            facts("記者", 0, 0, false),
            facts("汽車", 3, 1_000_000, true),
            facts("x\u{1f}y", 12, 7, false),
        ];
        for input in &inputs {
            for candidate in &candidates {
                for bits in [10, 20, 28] {
                    let expected: Vec<usize> = features(input, candidate)
                        .iter()
                        .map(|f| feature_index(f, bits))
                        .collect();
                    assert_eq!(feature_indices(input, candidate, bits), expected);
                }
            }
        }
    }

    #[test]
    fn feature_indices_are_fixed_and_within_the_bits() {
        let index = feature_index("s\u{1f}記者", 20);
        assert!(index < 1 << 20);
        assert_eq!(index, feature_index("s\u{1f}記者", 20));
        assert_eq!(index, feature_index("s\u{1f}記者", 24) & ((1 << 20) - 1));
        // Pinned: training and running must agree on every index.
        assert_eq!(feature_index("s\u{1f}記者", 16), 0x4020);
    }

    #[test]
    fn a_broken_model_file_is_refused() {
        let good = RankingModel::new(10, Weights::F32(vec![0.0; 1 << 10]))
            .unwrap()
            .to_bytes();
        let mut magic = good.clone();
        magic[0] = b'X';
        assert!(RankingModel::from_bytes(magic).is_err());
        let mut changed = good.clone();
        changed[40] ^= 1;
        assert!(RankingModel::from_bytes(changed).is_err());
        assert!(RankingModel::from_bytes(&good[..good.len() - 1]).is_err());
        let mut bits = good.clone();
        bits[12] = 40;
        assert!(RankingModel::from_bytes(bits).is_err());
    }

    #[test]
    fn a_model_header_off_the_format_is_refused() {
        let good = RankingModel::new(10, Weights::F32(vec![0.0; 1 << 10]))
            .unwrap()
            .to_bytes();
        for at in [14, 15, 20, 23] {
            let mut reserved = good.clone();
            reserved[at] = 1;
            assert!(RankingModel::from_bytes(reserved).is_err(), "byte {at}");
        }
        let mut scale = good.clone();
        scale[16..20].copy_from_slice(&2.0f32.to_le_bytes());
        assert!(
            RankingModel::from_bytes(scale).is_err(),
            "an f32 model scales by 1"
        );
    }

    #[test]
    fn a_model_with_weights_that_are_not_numbers_is_refused() {
        for bad in [f32::NAN, f32::INFINITY] {
            let mut weights = vec![0.0f32; 1 << 10];
            weights[5] = bad;
            assert!(RankingModel::new(10, Weights::F32(weights)).is_err());
            let quantized = Weights::I8 {
                scale: bad,
                weights: vec![0; 1 << 10],
            };
            assert!(RankingModel::new(10, quantized).is_err());
        }
    }

    #[test]
    fn a_model_of_an_earlier_format_is_refused() {
        let mut old = RankingModel::new(10, Weights::F32(vec![0.0; 1 << 10]))
            .unwrap()
            .to_bytes();
        old[8..12].copy_from_slice(&2u32.to_le_bytes());
        assert!(RankingModel::from_bytes(old).is_err());
    }

    // Extending a file leaves the new length sparse on Unix file systems,
    // while NTFS reserves room for all of it.
    #[cfg(unix)]
    #[test]
    fn a_file_larger_than_any_model_is_refused_without_being_read() {
        let path =
            std::env::temp_dir().join(format!("kanaemi-engine-huge-{}.model", std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        // Reading it whole would take 64 GiB of memory.
        file.set_len(1 << 36).unwrap();
        let (done, refused) = std::sync::mpsc::channel();
        std::thread::spawn({
            let path = path.clone();
            move || done.send(RankingModel::open(&path).is_err())
        });
        let refused = refused.recv_timeout(std::time::Duration::from_secs(5));
        let _ = std::fs::remove_file(&path);
        assert_eq!(refused, Ok(true));
    }

    // Robustness: whatever the bytes, reading them either refuses them or
    // gives a model that scores every candidate.

    fn score_some(model: &RankingModel) {
        let history = history(&[("きしゃ", "汽車"), ("かんじ", "漢字")]);
        let input = RankingInput {
            reading: "きしゃ",
            history: &history,
            context: "今日の漢字は",
        };
        for candidate in [facts("記者", 0, 0, false), facts("汽車", 3, u32::MAX, true)] {
            model.score_candidate(&input, &candidate);
        }
    }

    fn read_and_score(bytes: &[u8]) {
        if let Ok(model) = RankingModel::from_bytes(bytes) {
            score_some(&model);
        }
    }

    fn models() -> impl Strategy<Value = Vec<u8>> {
        let f32_model = vec(
            any::<f32>().prop_filter("finite", |w| w.is_finite()),
            1 << 10,
        )
        .prop_map(|weights| {
            RankingModel::new(10, Weights::F32(weights))
                .unwrap()
                .to_bytes()
        });
        let i8_model = (-4.0f32..4.0, vec(any::<i8>(), 1 << 10)).prop_map(|(scale, weights)| {
            RankingModel::new(10, Weights::I8 { scale, weights })
                .unwrap()
                .to_bytes()
        });
        prop_oneof![f32_model, i8_model]
    }

    proptest! {
        #[test]
        fn any_bytes_are_refused_or_score_without_panicking(
            bytes in vec(any::<u8>(), 0..256),
        ) {
            read_and_score(&bytes);
        }

        #[test]
        fn any_header_is_refused_or_scores_without_panicking(
            version in prop_oneof![Just(MODEL_FORMAT_VERSION), any::<u32>()],
            bits in prop_oneof![10..=11u8, any::<u8>()],
            kind in prop_oneof![0..=1u8, any::<u8>()],
            reserved in prop_oneof![Just([0u8; 6]), any::<[u8; 6]>()],
            scale in prop_oneof![Just(1.0f32), any::<f32>()],
            body in prop_oneof![
                vec(any::<u8>(), 1 << 10),
                vec(any::<u8>(), 4 << 10),
                vec(any::<u8>(), 0..(4 << 10) + 8),
            ],
            checksum in proptest::option::of(any::<u64>()),
        ) {
            let mut bytes = MAGIC.to_vec();
            bytes.extend_from_slice(&version.to_le_bytes());
            bytes.extend_from_slice(&[bits, kind, reserved[0], reserved[1]]);
            bytes.extend_from_slice(&scale.to_le_bytes());
            bytes.extend_from_slice(&reserved[2..]);
            let checksum = checksum.unwrap_or_else(|| xxhash_rust::xxh3::xxh3_64(&body));
            bytes.extend_from_slice(&checksum.to_le_bytes());
            bytes.extend(body);
            read_and_score(&bytes);
        }

        #[test]
        fn a_changed_model_is_refused_or_scores_without_panicking(
            model in models(),
            at in any::<prop::sample::Index>(),
            byte: u8,
            cut in any::<prop::sample::Index>(),
            truncate: bool,
        ) {
            let mut bytes = model;
            let at = at.index(bytes.len());
            bytes[at] = byte;
            if truncate {
                bytes.truncate(cut.index(bytes.len()));
            }
            read_and_score(&bytes);
        }

        #[test]
        fn a_changed_weight_fails_the_checksum(
            model in models(),
            at in any::<prop::sample::Index>(),
            bit in 0..8u8,
        ) {
            let mut bytes = model;
            let at = HEADER_LEN + at.index(bytes.len() - HEADER_LEN);
            bytes[at] ^= 1 << bit;
            prop_assert!(matches!(
                RankingModel::from_bytes(bytes),
                Err(ModelError::Checksum)
            ));
        }
    }
}
