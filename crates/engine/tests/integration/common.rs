//! Helpers the integration tests share.

use std::cell::RefCell;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use kanaemi_core::Effect;
use kanaemi_engine::{Dictionary, Engine, LineSink, RankingModel, open_dictionary};

/// What the core would report, fed to an engine one effect at a time.
pub trait Learn {
    fn commit(&mut self, reading: &str, surface: &str);
    /// `reading` marks okurigana with `*`, as a text dictionary writes it.
    fn register(&mut self, reading: &str, surface: &str);
    fn delete(&mut self, reading: &str, surface: &str);
    fn withdraw(&mut self, reading: &str, surface: &str);
    fn type_text(&mut self, text: &str);
    fn erase(&mut self, text: &str);
    fn move_focus(&mut self);
}

impl Learn for Engine {
    fn commit(&mut self, reading: &str, surface: &str) {
        self.learn(&Effect::Committed {
            reading: reading.to_owned(),
            okurigana: None,
            surface: surface.to_owned(),
        });
    }

    fn register(&mut self, reading: &str, surface: &str) {
        let (reading, okurigana) = match reading.split_once('*') {
            Some((stem, okurigana)) => (stem, Some(okurigana.to_owned())),
            None => (reading, None),
        };
        self.learn(&Effect::Registered {
            reading: reading.to_owned(),
            okurigana_head: okurigana.clone(),
            okurigana,
            surface: surface.to_owned(),
        });
    }

    fn delete(&mut self, reading: &str, surface: &str) {
        self.learn(&Effect::Forgotten {
            reading: reading.to_owned(),
            okurigana: None,
            surface: surface.to_owned(),
        });
    }

    fn withdraw(&mut self, reading: &str, surface: &str) {
        self.learn(&Effect::Withdrawn {
            reading: reading.to_owned(),
            okurigana: None,
            surface: surface.to_owned(),
        });
    }

    fn type_text(&mut self, text: &str) {
        self.learn(&Effect::Typed(text.to_owned()));
    }

    fn erase(&mut self, text: &str) {
        self.learn(&Effect::Erased(text.to_owned()));
    }

    fn move_focus(&mut self) {
        self.learn(&Effect::FocusMoved);
    }
}

/// Writes nothing, for an engine whose registrations need not last.
pub struct Discard;

impl LineSink for Discard {
    fn append(&mut self, _line: &str) -> io::Result<()> {
        Ok(())
    }
}

/// Keeps the lines written, for a test to look at.
#[derive(Clone, Default)]
pub struct Lines(pub Rc<RefCell<Vec<String>>>);

impl LineSink for Lines {
    fn append(&mut self, line: &str) -> io::Result<()> {
        self.0.borrow_mut().push(line.to_owned());
        Ok(())
    }
}

/// A path no other test uses, in a folder of this test run.
pub fn temp_path(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("kanaemi-engine-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{}-{name}", NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// A text dictionary opened from a file, as the input method opens one.
pub fn dictionary(text: &str) -> Box<dyn Dictionary> {
    let path = temp_path("dictionary.tsv");
    std::fs::write(&path, text).unwrap();
    let (dictionary, invalid) = open_dictionary(&path).unwrap();
    assert_eq!(invalid, [], "{text:?}");
    dictionary
}

/// A ranking model file of `2^bits` f32 weights, written as the ranking model
/// spec lays it out, and opened as the input method opens one. Each feature
/// is its name and values joined by U+001F.
pub fn model(bits: u8, weights: &[(&str, f32)]) -> Arc<RankingModel> {
    let mut all = vec![0.0f32; 1 << bits];
    for (feature, weight) in weights {
        let index = xxhash_rust::xxh3::xxh3_64(feature.as_bytes()) & ((1 << bits) - 1);
        all[index as usize] += weight;
    }
    let body: Vec<u8> = all.iter().flat_map(|w| w.to_le_bytes()).collect();
    let mut file = Vec::new();
    file.extend_from_slice(b"KANAEMIM");
    file.extend_from_slice(&3u32.to_le_bytes());
    file.extend_from_slice(&[bits, 0, 0, 0]);
    file.extend_from_slice(&1.0f32.to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&xxhash_rust::xxh3::xxh3_64(&body).to_le_bytes());
    file.extend_from_slice(&body);
    let path = temp_path("ranking.model");
    std::fs::write(&path, file).unwrap();
    Arc::new(RankingModel::open(&path).unwrap())
}
