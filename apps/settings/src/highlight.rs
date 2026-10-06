//! Luau source cut into the kinds of text a reader tells apart by colour, as
//! the Luau grammar for Tree-sitter and its own highlight query tell them.

use std::sync::OnceLock;

use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

/// The captures of the highlight query given a colour; a capture names the
/// longest of these it starts with (`keyword.return` is a `keyword`).
const COLOURED: [&str; 9] = [
    "comment",
    "string",
    "number",
    "boolean",
    "keyword",
    "function",
    "constant",
    "type",
    "variable.builtin",
];

fn configuration() -> Option<&'static HighlightConfiguration> {
    static CONFIGURATION: OnceLock<Option<HighlightConfiguration>> = OnceLock::new();
    CONFIGURATION
        .get_or_init(|| {
            let mut configuration = HighlightConfiguration::new(
                tree_sitter_luau::LANGUAGE.into(),
                "luau",
                tree_sitter_luau::HIGHLIGHTS_QUERY,
                "",
                tree_sitter_luau::LOCALS_QUERY,
            )
            .ok()?;
            configuration.configure(&COLOURED);
            Some(configuration)
        })
        .as_ref()
}

/// `source` in pieces, in order, each with the class the style sheet colours
/// it by, or none; together they are `source` again. All of it is plain when
/// it cannot be highlighted.
pub fn luau(source: &str) -> Vec<(Option<String>, &str)> {
    let plain = || vec![(None, source)];
    let Some(configuration) = configuration() else {
        return plain();
    };
    let mut highlighter = Highlighter::new();
    let Ok(events) = highlighter.highlight(configuration, source.as_bytes(), None, None, |_| None)
    else {
        return plain();
    };
    let mut pieces = Vec::new();
    let mut open = Vec::new();
    for event in events {
        match event {
            Ok(HighlightEvent::HighlightStart(highlight)) => open.push(highlight.0),
            Ok(HighlightEvent::HighlightEnd) => {
                open.pop();
            }
            Ok(HighlightEvent::Source { start, end }) => {
                let class = open
                    .last()
                    .map(|&at| format!("hl-{}", COLOURED[at].replace('.', "-")));
                pieces.push((class, &source[start..end]));
            }
            Err(_) => return plain(),
        }
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pieces of `source`, checked to make it again.
    fn pieces(source: &str) -> Vec<(Option<String>, &str)> {
        let pieces = luau(source);
        assert_eq!(
            pieces.iter().map(|(_, text)| *text).collect::<String>(),
            source,
            "the pieces make the source again"
        );
        pieces
    }

    #[test]
    fn keywords_strings_numbers_and_comments_are_coloured() {
        let pieces = pieces("-- 数\nlocal n = 12\nreturn \"a\" .. n");
        let class_of = |text: &str| {
            pieces
                .iter()
                .find(|(_, t)| *t == text)
                .and_then(|(class, _)| class.as_deref())
        };
        assert_eq!(class_of("-- 数"), Some("hl-comment"));
        assert_eq!(class_of("local"), Some("hl-keyword"));
        assert_eq!(class_of("12"), Some("hl-number"));
        assert_eq!(class_of("return"), Some("hl-keyword"));
        assert_eq!(class_of("\"a\""), Some("hl-string"));
    }

    #[test]
    fn source_it_cannot_parse_is_still_all_there() {
        pieces("return function( -- 途中");
    }
}
