#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub surface: String,
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
}
