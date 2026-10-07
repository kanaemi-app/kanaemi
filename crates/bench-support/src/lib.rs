//! Synthetic dictionaries and ranking models the benchmarks run on: as large
//! as the ones users install, made on the spot so no file has to be shipped
//! or downloaded. The same arguments always make the same bytes, so runs
//! compare.

use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;

/// The kana readings are spelled with, each typed by one romaji syllable.
const KANA: &str = "あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわがぎぐげござじずぜぞだでどばびぶべぼぱぴぷぺぽ";

/// Where kanji start in Unicode, and how many follow there.
const KANJI_START: u32 = 0x4E00;
const KANJI_COUNT: u64 = 20_000;

/// The surfaces of こう: enough candidates to page through.
const KOU: &str =
    "高校行考効項構講公工功孝光広好交口向后厚坑抗攻更港硬綱興衡鋼酵稿購侯幸康江洪皇紅荒郊香";

/// Words the benchmarks type, as a dictionary has them.
const KNOWN: &str = "かんじ\t漢字\nかんじ\t感じ\nかんじ\t幹事\nかんじ\t監事\nかんじ\t完治\n\
きしゃ\t記者\nきしゃ\t汽車\nきしゃ\t貴社\nきしゃ\t帰社\n\
か\t書\t五段-カ行\nか*く\t書く\nか*く\t欠く\n\
{}こ\t{}個\n";

/// An empty folder of `name` for this process to write in, emptied of what
/// an earlier run left.
pub fn fresh_dir(name: impl AsRef<str>) -> PathBuf {
    let name = name.as_ref();
    let dir = std::env::temp_dir().join(format!("kanaemi-bench-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a folder in the temporary folder");
    dir
}

/// The reading at `index`: two kana or more, and a different one for every
/// index.
pub fn reading(index: usize) -> String {
    // Split once: a dictionary asks for hundreds of thousands of readings.
    static KANA_CHARS: LazyLock<Vec<char>> = LazyLock::new(|| KANA.chars().collect());
    let kana = &*KANA_CHARS;
    let base = kana.len();
    // Bijective numeration, past the numbers written with one digit.
    let mut n = index + base + 1;
    let mut reading = Vec::new();
    while n > 0 {
        n -= 1;
        reading.push(kana[n % base]);
        n /= base;
    }
    reading.into_iter().rev().collect()
}

/// A text dictionary with a word for each of `readings` readings, most
/// with a few candidates and some with many, some conjugating and some with
/// okurigana, and the words the benchmarks type.
pub fn text_dictionary(readings: usize) -> String {
    let mut text = String::from("# Synthetic\n");
    text.push_str(KNOWN);
    for kanji in KOU.chars() {
        text.push_str(&format!("こう\t{kanji}\n"));
    }
    for index in 0..readings {
        let mut random = Random(index as u64);
        let reading = reading(index);
        let surfaces = match random.below(16) {
            0 => 12,
            n => 1 + n % 4,
        };
        for _ in 0..surfaces {
            let len = 1 + random.below(3) as usize;
            let surface = random.kanji(len);
            match random.below(2) {
                0 => text.push_str(&format!("{reading}\t{surface}\n")),
                _ => text.push_str(&format!(
                    "{reading}\t{surface}\t\t{}\n",
                    random.below(10_000)
                )),
            }
        }
        match index % 8 {
            0 => text.push_str(&format!("{reading}\t{}\t五段-カ行\n", random.kanji(1))),
            4 => text.push_str(&format!("{reading}*く\t{}く\n", random.kanji(1))),
            _ => {}
        }
    }
    text
}

/// A ranking model file of `2^bits` i8 weights, as `docs/spec/ranking-model.md`
/// lays it out.
pub fn ranking_model(bits: u8) -> Vec<u8> {
    let mut random = Random(u64::from(bits));
    let body: Vec<u8> = (0..1u64 << bits).map(|_| random.below(256) as u8).collect();
    let mut file = Vec::with_capacity(32 + body.len());
    file.extend_from_slice(b"KANAEMIM");
    file.extend_from_slice(&3u32.to_le_bytes());
    file.push(bits);
    file.push(1);
    file.extend_from_slice(&[0, 0]);
    file.extend_from_slice(&0.01f32.to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&xxhash_rust::xxh3::xxh3_64(&body).to_le_bytes());
    file.extend_from_slice(&body);
    file
}

/// SplitMix64: enough to spread the words, and the same on every machine.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn kanji(&mut self, len: usize) -> String {
        (0..len)
            .map(|_| {
                let code = KANJI_START + self.below(KANJI_COUNT) as u32;
                char::from_u32(code).expect("a kanji")
            })
            .collect()
    }
}
