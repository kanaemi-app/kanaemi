//! What the candidate window shows and where, worked out apart from AppKit:
//! the rows of a page, how they are laid out with the highlighted
//! candidate's meaning beside them, where the window goes by the caret, and
//! which meanings are known or still to be looked up.

use std::collections::{HashMap, VecDeque};

use kanaemi_core::CandidateView;

/// The most of a meaning shown, in characters.
const MEANING_CHARS: usize = 240;

/// How many meanings are kept once looked up, the oldest forgotten first.
const MEANINGS_KEPT: usize = 256;

const ELLIPSIS: char = '…';

/// One candidate on the page as the window shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The digit that picks it.
    pub number: String,
    pub surface: String,
    pub source: Option<String>,
    /// For a reading to complete with, its first candidate.
    pub preview: Option<String>,
}

impl Row {
    /// What is shown small beside the candidate: its dictionary, or for a
    /// reading to complete with, its first candidate.
    pub fn beside(&self) -> Option<&str> {
        self.source.as_deref().or(self.preview.as_deref())
    }
}

/// The rows of the page, numbered from 1 as the digits pick them.
pub fn rows(view: &CandidateView) -> Vec<Row> {
    view.items
        .iter()
        .enumerate()
        .map(|(index, candidate)| Row {
            number: (index + 1).to_string(),
            surface: candidate.surface.clone(),
            source: candidate.source.clone(),
            preview: candidate.preview.clone(),
        })
        .collect()
}

/// The candidate whose meaning the window shows: the highlighted one, when
/// a dictionary gave it. A form of the reading, such as its katakana, or a
/// reading to complete with is not a word with a meaning of its own.
pub fn meaning_wanted(view: &CandidateView) -> Option<&str> {
    view.items
        .get(view.selected)
        .filter(|candidate| candidate.source.is_some())
        .map(|candidate| candidate.surface.as_str())
}

/// The candidates on the page whose meanings are looked up, to mark those
/// that have one: each a dictionary gave, the highlighted one first.
pub fn to_look_up(view: &CandidateView) -> Vec<String> {
    let highlighted = meaning_wanted(view);
    let rest = view
        .items
        .iter()
        .filter(|candidate| candidate.source.is_some())
        .map(|candidate| candidate.surface.as_str())
        .filter(|surface| Some(*surface) != highlighted);
    highlighted
        .into_iter()
        .chain(rest)
        .map(str::to_owned)
        .collect()
}

/// What a screen reader is told of the highlighted `row`, as the window
/// draws text it cannot read: the candidate, then what is beside it.
pub fn spoken(row: &Row) -> String {
    match row.beside() {
        Some(beside) => format!("{}、{beside}", row.surface),
        None => row.surface.clone(),
    }
}

/// `text` cut to at most `chars` characters, the last of them an ellipsis
/// when anything was cut.
pub fn shorten(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(chars.saturating_sub(1)).collect();
    short.push(ELLIPSIS);
    short
}

/// A meaning as the window shows it: its lines trimmed, blank ones left
/// out, and cut short when long.
pub fn excerpt(meaning: &str) -> String {
    let lines: Vec<&str> = meaning
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    shorten(&lines.join("\n"), MEANING_CHARS)
}

/// What tells the page shown and how many there are, under the rows; none
/// for a list that fits a page.
pub fn footer(view: &CandidateView) -> Option<String> {
    (view.pages > 1).then(|| format!("‹ {} / {} ›", view.page + 1, view.pages))
}

/// What the pane beside the list shows: for a reading to complete with,
/// its candidates after the first, one to a line; for a candidate, its
/// meaning.
pub fn pane_text(more: &[String], meaning: Option<&str>) -> Option<String> {
    if !more.is_empty() {
        return Some(more.join("\n"));
    }
    meaning.map(excerpt)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// A rectangle on the screen, its origin at the bottom left as AppKit's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Space between the caret's line and the window.
const CARET_GAP: f64 = 4.0;

/// Where the window of `size` goes for the caret's line `caret` on `screen`,
/// the part of the screen windows may use: below the line, or above it when
/// there is no room below, and always on the screen.
pub fn place(caret: Rect, size: Size, screen: Rect) -> (f64, f64) {
    let below = caret.y - CARET_GAP - size.height;
    let y = if below >= screen.y {
        below
    } else {
        caret.y + caret.height + CARET_GAP
    };
    let top = screen.y + screen.height - size.height;
    let right = screen.x + screen.width - size.width;
    (caret.x.min(right).max(screen.x), y.min(top).max(screen.y))
}

/// NSPopUpMenuWindowLevel: above ordinary windows, as Input Method Kit's panel.
pub const MENU_LEVEL: isize = 101;

/// The window level the candidates take for a client whose window is at
/// `client`: above it, however high it floats, and never below menus.
pub fn window_level(client: isize) -> isize {
    MENU_LEVEL.max(client.saturating_add(1))
}

/// Space around the rows and between the columns.
pub const PADDING: f64 = 8.0;

/// The widest a candidate is shown; a longer one is cut short, so the
/// window, its dictionary names and meaning included, stays on the screen.
pub const SURFACE_WIDTH: f64 = 420.0;

/// The widest a dictionary's name is shown: room for a description such as
/// an official dictionary's; only a longer one is cut short.
pub const SOURCE_WIDTH: f64 = 260.0;

/// The width of the slot for the mark of a candidate with a meaning. Every
/// row keeps it, marked or not, so the columns stay put as meanings come in.
pub const MARK_WIDTH: f64 = 14.0;

/// How wide each part of a row is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowWidths {
    pub number: f64,
    pub surface: f64,
    pub source: f64,
}

/// Where everything in the window goes, from its top left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Layout {
    pub size: Size,
    /// The left edge of each column.
    pub number_x: f64,
    pub surface_x: f64,
    pub mark_x: f64,
    pub source_x: f64,
    /// How wide the candidates are drawn.
    pub surface_width: f64,
    /// How wide the dictionaries' names are drawn.
    pub source_width: f64,
    /// The width of the list, the rows' highlight included.
    pub list_width: f64,
    /// The meaning's pane, right of the list, as tall as the window.
    pub pane: Option<Rect>,
    /// Where the page's place is told, under the rows at the right.
    pub footer: Option<Rect>,
}

/// Lays out rows `row_height` tall, their parts as wide as `widths`, a
/// footer of `footer` size under them, and a pane for a meaning of `meaning`
/// size beside them. Every column is as wide as its widest, the candidates'
/// no wider than [`SURFACE_WIDTH`] and the dictionaries' names no wider than
/// [`SOURCE_WIDTH`]; the names take no room when no row has one.
pub fn layout(
    widths: &[RowWidths],
    row_height: f64,
    meaning: Option<Size>,
    footer: Option<Size>,
) -> Layout {
    let widest = |part: fn(&RowWidths) -> f64| widths.iter().map(part).fold(0.0, f64::max);
    let number_x = PADDING;
    let surface_x = number_x + widest(|w| w.number) + PADDING;
    let surface_width = widest(|w| w.surface).min(SURFACE_WIDTH);
    let mark_x = surface_x + surface_width + PADDING / 2.0;
    let source_x = mark_x + MARK_WIDTH + PADDING;
    let source_width = widest(|w| w.source).min(SOURCE_WIDTH);
    let rows_width = if source_width > 0.0 {
        source_x + source_width + PADDING
    } else {
        mark_x + MARK_WIDTH + PADDING
    };
    let list_width = rows_width.max(footer.map_or(0.0, |f| f.width + PADDING * 2.0));
    let rows_bottom = PADDING + row_height * widths.len() as f64;
    let footer = footer.map(|f| Rect {
        x: list_width - f.width - PADDING,
        y: rows_bottom + PADDING / 2.0,
        width: f.width,
        height: f.height,
    });
    let list_height = footer.map_or(rows_bottom, |f| f.y + f.height) + PADDING;
    let pane_height = meaning.map_or(0.0, |m| m.height + PADDING * 2.0);
    let height = list_height.max(pane_height);
    let pane = meaning.map(|m| Rect {
        x: list_width,
        y: 0.0,
        width: m.width + PADDING * 2.0,
        height,
    });
    Layout {
        size: Size {
            width: list_width + pane.map_or(0.0, |p| p.width),
            height,
        },
        number_x,
        surface_x,
        mark_x,
        source_x,
        surface_width,
        source_width,
        list_width,
        pane,
        footer,
    }
}

/// The row at `(x, y)` from the window's top left, among `rows` rows
/// `row_height` tall in a list `list_width` wide.
pub fn row_at(x: f64, y: f64, rows: usize, row_height: f64, list_width: f64) -> Option<usize> {
    if !(0.0..list_width).contains(&x) || y < PADDING {
        return None;
    }
    let row = ((y - PADDING) / row_height) as usize;
    (row < rows).then_some(row)
}

/// The meanings looked up so far, those of the page shown, and those asked
/// for and not answered yet, so a page is asked for once and an answer that
/// comes for a page gone is kept without being shown.
#[derive(Default)]
pub struct Meanings {
    looked: HashMap<String, Option<String>>,
    order: VecDeque<String>,
    page: Vec<String>,
    asked: Vec<String>,
}

impl Meanings {
    /// Wants the meanings of `page`, in order, from now on. Returns those
    /// to look up, in that order, when they are not the ones asked for
    /// last; an empty list stops the lookups asked for before.
    pub fn want(&mut self, page: Vec<String>) -> Option<Vec<String>> {
        let unknown: Vec<String> = page
            .iter()
            .filter(|surface| !self.looked.contains_key(*surface))
            .cloned()
            .collect();
        self.page = page;
        if unknown == self.asked {
            return None;
        }
        self.asked = unknown.clone();
        Some(unknown)
    }

    /// The meaning of `surface`: `None` while not looked up, `Some(None)`
    /// when it has none.
    pub fn get(&self, surface: &str) -> Option<Option<&str>> {
        self.looked.get(surface).map(Option::as_deref)
    }

    /// Keeps the meaning of `surface` that was looked up; returns whether
    /// it is on the page shown, to be drawn.
    pub fn answer(&mut self, surface: String, meaning: Option<String>) -> bool {
        self.asked.retain(|asked| *asked != surface);
        let shown = self.page.contains(&surface);
        if !self.looked.contains_key(&surface) {
            if self.order.len() >= MEANINGS_KEPT
                && let Some(oldest) = self.order.pop_front()
            {
                self.looked.remove(&oldest);
            }
            self.order.push_back(surface.clone());
        }
        self.looked.insert(surface, meaning);
        shown
    }
}

#[cfg(test)]
mod tests {
    use kanaemi_core::Candidate;

    use super::*;

    fn candidate(surface: &str, source: Option<&str>) -> Candidate {
        Candidate {
            surface: surface.to_owned(),
            source: source.map(str::to_owned),
            preview: None,
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rows_are_numbered_from_one_and_name_their_dictionary_in_full() {
        let view = CandidateView {
            items: vec![
                candidate("漢字", Some("Kanaemi 公式辞書・基本（kanaemi-dict）")),
                candidate("カンジ", None),
            ],
            selected: 1,
            page: 0,
            pages: 1,
            more: Vec::new(),
        };
        assert_eq!(
            rows(&view),
            [
                Row {
                    number: "1".to_owned(),
                    surface: "漢字".to_owned(),
                    source: Some("Kanaemi 公式辞書・基本（kanaemi-dict）".to_owned()),
                    preview: None,
                },
                Row {
                    number: "2".to_owned(),
                    surface: "カンジ".to_owned(),
                    source: None,
                    preview: None,
                },
            ]
        );
    }

    #[test]
    fn a_reading_to_complete_with_shows_its_first_candidate_beside_it() {
        let view = CandidateView {
            items: vec![
                Candidate {
                    surface: "かんじ".to_owned(),
                    source: None,
                    preview: Some("漢字".to_owned()),
                },
                candidate("かお", None),
            ],
            selected: 0,
            page: 0,
            pages: 1,
            more: Vec::new(),
        };
        let shown = rows(&view);
        assert_eq!(shown[0].beside(), Some("漢字"));
        assert_eq!(shown[1].beside(), None);
        assert_eq!(spoken(&shown[0]), "かんじ、漢字");
        assert_eq!(
            to_look_up(&view),
            Vec::<String>::new(),
            "a reading has no meaning looked up"
        );
    }

    #[test]
    fn a_candidate_shows_its_dictionary_beside_it() {
        let view = CandidateView {
            items: vec![candidate("漢字", Some("ユーザー辞書"))],
            selected: 0,
            page: 0,
            pages: 1,
            more: Vec::new(),
        };
        assert_eq!(rows(&view)[0].beside(), Some("ユーザー辞書"));
    }

    #[test]
    fn the_meaning_of_the_highlighted_candidate_is_wanted_when_a_dictionary_gave_it() {
        let view = |selected| CandidateView {
            items: vec![
                candidate("漢字", Some("ユーザー辞書")),
                candidate("カンジ", None),
            ],
            selected,
            page: 0,
            pages: 1,
            more: Vec::new(),
        };
        assert_eq!(meaning_wanted(&view(0)), Some("漢字"));
        assert_eq!(
            meaning_wanted(&view(1)),
            None,
            "a form of the reading, or a reading to complete with"
        );
    }

    #[test]
    fn every_candidate_a_dictionary_gave_is_looked_up_the_highlighted_first() {
        let view = CandidateView {
            items: vec![
                candidate("漢字", Some("A")),
                candidate("感じ", Some("A")),
                candidate("カンジ", None),
                candidate("幹事", Some("B")),
            ],
            selected: 1,
            page: 0,
            pages: 1,
            more: Vec::new(),
        };
        assert_eq!(to_look_up(&view), strings(&["感じ", "漢字", "幹事"]));
    }

    #[test]
    fn the_pane_shows_the_other_candidates_of_a_reading_or_else_the_meaning() {
        assert_eq!(
            pane_text(&strings(&["感じ", "幹事"]), None),
            Some("感じ\n幹事".to_owned())
        );
        assert_eq!(
            pane_text(&[], Some("かんじ【漢字】\n\n文字")),
            Some("かんじ【漢字】\n文字".to_owned())
        );
        assert_eq!(pane_text(&[], None), None);
    }

    #[test]
    fn a_highlighted_row_is_spoken_as_its_candidate_then_its_dictionary() {
        let view = CandidateView {
            items: vec![
                candidate("漢字", Some("ユーザー辞書")),
                candidate("カンジ", None),
            ],
            selected: 0,
            page: 0,
            pages: 1,
            more: Vec::new(),
        };
        let shown = rows(&view);
        assert_eq!(spoken(&shown[0]), "漢字、ユーザー辞書");
        assert_eq!(spoken(&shown[1]), "カンジ");
    }

    #[test]
    fn the_window_floats_above_menus_and_above_a_client_that_floats_higher() {
        assert_eq!(window_level(0), MENU_LEVEL);
        assert_eq!(window_level(MENU_LEVEL), MENU_LEVEL + 1);
        assert_eq!(window_level(1000), 1001);
    }

    #[test]
    fn a_short_text_is_not_cut() {
        assert_eq!(shorten("新聞の語", 4), "新聞の語");
        assert_eq!(shorten("新聞の語", 3), "新聞…");
    }

    #[test]
    fn a_meaning_drops_blank_lines_and_space_around_each_line() {
        assert_eq!(
            excerpt("かんじ【漢字】\n\n  中国で作られた文字。 \n"),
            "かんじ【漢字】\n中国で作られた文字。"
        );
    }

    #[test]
    fn a_long_meaning_is_cut_short() {
        let shown = excerpt(&"字".repeat(MEANING_CHARS * 2));
        assert_eq!(shown.chars().count(), MEANING_CHARS);
        assert!(shown.ends_with(ELLIPSIS));
    }

    const SCREEN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1000.0,
        height: 800.0,
    };

    fn caret(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 1.0,
            height: 20.0,
        }
    }

    const WINDOW: Size = Size {
        width: 200.0,
        height: 100.0,
    };

    #[test]
    fn the_window_goes_below_the_caret_line_from_its_left() {
        assert_eq!(
            place(caret(300.0, 400.0), WINDOW, SCREEN),
            (300.0, 400.0 - CARET_GAP - 100.0)
        );
    }

    #[test]
    fn without_room_below_the_window_goes_above_the_caret_line() {
        assert_eq!(
            place(caret(300.0, 50.0), WINDOW, SCREEN),
            (300.0, 50.0 + 20.0 + CARET_GAP)
        );
    }

    #[test]
    fn the_window_stays_on_the_screen() {
        assert_eq!(place(caret(950.0, 400.0), WINDOW, SCREEN).0, 800.0);
        assert_eq!(place(caret(-50.0, 400.0), WINDOW, SCREEN).0, 0.0);
        let beside = Rect {
            x: 1000.0,
            y: 0.0,
            width: 1000.0,
            height: 800.0,
        };
        assert_eq!(place(caret(1500.0, 400.0), WINDOW, beside).0, 1500.0);
        let tall = Size {
            width: 200.0,
            height: 790.0,
        };
        assert_eq!(place(caret(300.0, 50.0), tall, SCREEN).1, 10.0);
    }

    fn widths(number: f64, surface: f64, source: f64) -> RowWidths {
        RowWidths {
            number,
            surface,
            source,
        }
    }

    #[test]
    fn each_column_is_as_wide_as_its_widest_with_a_slot_for_the_mark_between() {
        let laid = layout(
            &[widths(8.0, 30.0, 0.0), widths(8.0, 50.0, 40.0)],
            20.0,
            None,
            None,
        );
        assert_eq!(laid.surface_x, PADDING + 8.0 + PADDING);
        assert_eq!(laid.mark_x, laid.surface_x + 50.0 + PADDING / 2.0);
        assert_eq!(laid.source_x, laid.mark_x + MARK_WIDTH + PADDING);
        assert_eq!(laid.list_width, laid.source_x + 40.0 + PADDING);
        assert_eq!(laid.size.width, laid.list_width);
        assert_eq!(laid.size.height, 20.0 * 2.0 + PADDING * 2.0);
        assert_eq!(laid.pane, None);
    }

    #[test]
    fn a_typical_dictionary_name_gets_its_full_width() {
        // About as wide as ユーザー辞書, SKK-JISYO.L and an official
        // dictionary's description are at the names' size.
        for name in [66.0, 62.0, 230.0] {
            let laid = layout(&[widths(8.0, 30.0, name)], 20.0, None, None);
            assert_eq!(laid.source_width, name);
            assert_eq!(laid.list_width, laid.source_x + name + PADDING);
        }
    }

    #[test]
    fn only_a_very_long_dictionary_name_is_cut_short() {
        let laid = layout(&[widths(8.0, 30.0, SOURCE_WIDTH * 2.0)], 20.0, None, None);
        assert_eq!(laid.source_width, SOURCE_WIDTH);
        assert_eq!(laid.list_width, laid.source_x + SOURCE_WIDTH + PADDING);
    }

    #[test]
    fn rows_without_a_dictionary_keep_only_the_slot_for_the_mark() {
        let laid = layout(&[widths(8.0, 30.0, 0.0)], 20.0, None, None);
        assert_eq!(laid.source_width, 0.0);
        assert_eq!(laid.list_width, laid.mark_x + MARK_WIDTH + PADDING);
    }

    #[test]
    fn a_long_candidate_is_given_no_more_than_the_widest_a_candidate_is_shown() {
        let laid = layout(
            &[widths(8.0, SURFACE_WIDTH * 3.0, 40.0)],
            20.0,
            Some(Size {
                width: 100.0,
                height: 10.0,
            }),
            None,
        );
        assert_eq!(laid.surface_width, SURFACE_WIDTH);
        assert_eq!(laid.mark_x, laid.surface_x + SURFACE_WIDTH + PADDING / 2.0);
        let short = layout(&[widths(8.0, 30.0, 0.0)], 20.0, None, None);
        assert_eq!(short.surface_width, 30.0);
    }

    #[test]
    fn a_meaning_takes_a_pane_beside_the_list_that_may_make_the_window_taller() {
        let rows = [widths(8.0, 30.0, 0.0)];
        let short = layout(
            &rows,
            20.0,
            Some(Size {
                width: 100.0,
                height: 10.0,
            }),
            None,
        );
        assert_eq!(
            short.size.height,
            20.0 + PADDING * 2.0,
            "as tall as the list"
        );
        let pane = short.pane.unwrap();
        assert_eq!(
            (pane.x, pane.width),
            (short.list_width, 100.0 + PADDING * 2.0)
        );
        assert_eq!(short.size.width, short.list_width + pane.width);
        let tall = layout(
            &rows,
            20.0,
            Some(Size {
                width: 100.0,
                height: 200.0,
            }),
            None,
        );
        assert_eq!(tall.size.height, 200.0 + PADDING * 2.0);
        assert_eq!(tall.pane.unwrap().height, tall.size.height);
    }

    #[test]
    fn the_footer_tells_the_page_shown_of_how_many_only_when_there_are_more() {
        let view = |page, pages| CandidateView {
            items: vec![candidate("高", None)],
            selected: 0,
            page,
            pages,
            more: Vec::new(),
        };
        assert_eq!(footer(&view(1, 5)).as_deref(), Some("‹ 2 / 5 ›"));
        assert_eq!(footer(&view(0, 1)), None);
    }

    #[test]
    fn the_footer_goes_under_the_rows_and_widens_the_list_to_fit() {
        let rows = [widths(8.0, 30.0, 0.0), widths(8.0, 30.0, 0.0)];
        let without = layout(&rows, 20.0, None, None);
        let laid = layout(
            &rows,
            20.0,
            None,
            Some(Size {
                width: 300.0,
                height: 12.0,
            }),
        );
        let footer = laid.footer.unwrap();
        assert_eq!(footer.y, PADDING + 20.0 * 2.0 + PADDING / 2.0);
        assert_eq!((footer.width, footer.height), (300.0, 12.0));
        assert_eq!(laid.list_width, 300.0 + PADDING * 2.0);
        assert_eq!(laid.size.height, without.size.height + 12.0 + PADDING / 2.0);
        assert_eq!(without.footer, None);
    }

    #[test]
    fn the_footer_sits_at_the_right_under_rows_wider_than_it() {
        let rows = [widths(8.0, 30.0, 200.0)];
        let small = Size {
            width: 40.0,
            height: 12.0,
        };
        let laid = layout(&rows, 20.0, None, Some(small));
        let footer = laid.footer.unwrap();
        assert_eq!(footer.x + footer.width, laid.list_width - PADDING);
    }

    #[test]
    fn a_click_on_the_footer_picks_nothing() {
        let laid = layout(
            &[widths(8.0, 30.0, 0.0)],
            20.0,
            None,
            Some(Size {
                width: 10.0,
                height: 12.0,
            }),
        );
        let footer = laid.footer.unwrap();
        assert_eq!(row_at(10.0, footer.y + 1.0, 1, 20.0, laid.list_width), None);
    }

    #[test]
    fn a_click_picks_the_row_under_it_in_the_list_only() {
        let at = |x, y| row_at(x, y, 3, 20.0, 100.0);
        assert_eq!(at(10.0, PADDING + 1.0), Some(0));
        assert_eq!(at(10.0, PADDING + 45.0), Some(2));
        assert_eq!(at(10.0, PADDING - 1.0), None, "above the first row");
        assert_eq!(at(10.0, PADDING + 61.0), None, "below the last row");
        assert_eq!(at(150.0, PADDING + 1.0), None, "in the meaning's pane");
    }

    #[test]
    fn a_page_is_asked_for_once() {
        let mut meanings = Meanings::default();
        let page = strings(&["漢字", "感じ"]);
        assert_eq!(meanings.want(page.clone()), Some(page.clone()));
        assert_eq!(meanings.want(page), None, "their answers are on the way");
    }

    #[test]
    fn only_what_is_not_known_yet_is_asked_for() {
        let mut meanings = Meanings::default();
        meanings.want(strings(&["漢字"]));
        meanings.answer("漢字".to_owned(), Some("文字".to_owned()));
        assert_eq!(
            meanings.want(strings(&["感じ", "漢字", "幹事"])),
            Some(strings(&["感じ", "幹事"]))
        );
    }

    #[test]
    fn a_page_with_everything_known_stops_the_lookups_asked_for_before() {
        let mut meanings = Meanings::default();
        meanings.want(strings(&["漢字"]));
        assert_eq!(meanings.want(Vec::new()), Some(Vec::new()));
        assert_eq!(meanings.want(Vec::new()), None);
    }

    #[test]
    fn a_meaning_looked_up_is_kept_and_drawn_while_its_page_is_shown() {
        let mut meanings = Meanings::default();
        meanings.want(strings(&["漢字"]));
        assert!(meanings.answer("漢字".to_owned(), Some("文字".to_owned())));
        assert_eq!(meanings.get("漢字"), Some(Some("文字")));
        assert_eq!(meanings.get("感じ"), None, "not looked up yet");
    }

    #[test]
    fn a_meaning_that_comes_after_its_page_went_is_kept_undrawn() {
        let mut meanings = Meanings::default();
        meanings.want(strings(&["漢字"]));
        meanings.want(strings(&["記者"]));
        assert!(!meanings.answer("漢字".to_owned(), Some("文字".to_owned())));
        assert_eq!(meanings.get("漢字"), Some(Some("文字")));
    }

    #[test]
    fn having_no_meaning_is_kept_too() {
        let mut meanings = Meanings::default();
        meanings.want(strings(&["カンジ"]));
        meanings.answer("カンジ".to_owned(), None);
        assert_eq!(meanings.get("カンジ"), Some(None));
        assert_eq!(
            meanings.want(strings(&["カンジ"])),
            None,
            "nothing to look up"
        );
    }

    #[test]
    fn the_oldest_meanings_are_forgotten_past_those_kept() {
        let mut meanings = Meanings::default();
        for n in 0..=MEANINGS_KEPT {
            meanings.answer(n.to_string(), Some("意味".to_owned()));
        }
        assert_eq!(meanings.get("0"), None);
        assert_eq!(meanings.get("1"), Some(Some("意味")));
    }
}
