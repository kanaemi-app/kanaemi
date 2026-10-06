use unicode_segmentation::UnicodeSegmentation;

/// How many Backspace presses take `text` off the end of a field: text views
/// delete a whole grapheme at a time, the base and its marks together.
pub fn backspaces(text: &str) -> usize {
    text.graphemes(true).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_backspace_takes_each_grapheme() {
        assert_eq!(backspaces("格調する"), 4);
        assert_eq!(backspaces("𥸮"), 1, "a character out of the BMP");
        assert_eq!(backspaces("か\u{3099}"), 1, "a base and its mark");
        assert_eq!(backspaces("葛\u{E0100}"), 1, "a variation sequence");
        assert_eq!(backspaces(""), 0);
    }
}
