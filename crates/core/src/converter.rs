#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub surface: String,
    /// The name of the dictionary it came from, for the host to show beside
    /// it. `None` for one no dictionary gave, such as the reading as
    /// katakana.
    pub source: Option<String>,
    /// For a reading to complete with, the candidate converting it gives
    /// first, for the host to show beside it. `None` otherwise, and for a
    /// reading that converts to nothing.
    pub preview: Option<String>,
}

/// Turns readings into candidates. What the user commits, registers and
/// forgets comes back as [`crate::Effect`]s for the host to learn from, so
/// converting is all the core asks of it.
///
/// Readings are hiragana. A reading with okurigana includes it: 書く is `かく` with
/// okurigana `く`.
pub trait Converter {
    /// Candidates for `reading`, best first. `okurigana` is given when the
    /// user marked where it starts: its first chunk, then any kana finished
    /// from romaji that chunk left over (`った` of `;ka;tta`).
    fn convert(&self, reading: &str, okurigana: Option<&str>) -> Vec<Candidate>;

    /// The text a word registered as `surface` for `reading` enters the
    /// field as: a converter whose words can stand for more than one text
    /// (`{}個` for 1個 and 2個) writes it for this reading. By default, the
    /// word as typed.
    fn registered_text(&self, reading: &str, okurigana: Option<&str>, surface: &str) -> String {
        let _ = (reading, okurigana);
        surface.to_owned()
    }

    /// The first `limit` candidates converting `reading` gives, best first,
    /// shown with it while it is listed to complete with. The user has not
    /// converted it, so a converter that keeps what it converted to learn
    /// from keeps nothing of this. By default, those of
    /// [`Converter::convert`].
    fn preview(&self, reading: &str, limit: usize) -> Vec<String> {
        self.convert(reading, None)
            .into_iter()
            .take(limit)
            .map(|candidate| candidate.surface)
            .collect()
    }

    /// Readings longer than `reading` that start with it, best first, to
    /// complete it with when the user asks. None by default.
    fn complete(&self, reading: &str) -> Vec<String> {
        let _ = reading;
        Vec::new()
    }
}
