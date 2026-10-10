//! What the candidate window shows and how it is laid out, worked out apart
//! from GDI: each candidate with, small beside it, the dictionary it came
//! from or, for a reading to complete with, its first candidate; and for a
//! highlighted reading, its other candidates in a pane beside the list.

use kanaemi_core::CandidateView;

/// Space around the rows and between the columns, in pixels.
pub const PADDING: i32 = 6;

/// The widest a candidate is drawn; a longer one is cut short, so the rest
/// of the window stays near it.
pub const SURFACE_WIDTH: i32 = 480;

/// The widest what is beside a candidate is drawn: room for a description
/// such as an official dictionary's; only a longer one is cut short.
pub const BESIDE_WIDTH: i32 = 300;

/// A page of candidates as the window shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    /// The candidates themselves, as the TSF UI element lists them too.
    pub items: Vec<String>,
    /// Beside `items`: the dictionary, or a reading's first candidate.
    pub beside: Vec<Option<String>>,
    pub selected: usize,
    /// For a highlighted reading to complete with, its other candidates.
    pub more: Vec<String>,
    /// Which page is shown of how many, when there are more than one.
    pub footer: Option<String>,
}

impl Page {
    pub fn new(view: &CandidateView) -> Self {
        Self {
            items: view.items.iter().map(|c| c.surface.clone()).collect(),
            beside: view
                .items
                .iter()
                .map(|c| c.source.clone().or_else(|| c.preview.clone()))
                .collect(),
            selected: view.selected,
            more: view.more.clone(),
            footer: (view.pages > 1).then(|| format!("‹ {} / {} ›", view.page + 1, view.pages)),
        }
    }
}

/// How wide each part of a row is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Widths {
    pub number: i32,
    pub surface: i32,
    pub beside: i32,
}

/// Where everything in the window goes, from its top left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub number_x: i32,
    pub surface_x: i32,
    pub surface_width: i32,
    pub beside_x: i32,
    pub beside_width: i32,
    /// The width of the list, the rows' highlight included.
    pub list_width: i32,
    /// The pane right of the list: where it starts and how wide it is.
    pub pane: Option<(i32, i32)>,
    /// Where the footer's text starts, at the right of the line under the rows.
    pub footer: Option<(i32, i32)>,
    pub width: i32,
    pub height: i32,
}

/// Lays out rows `line` tall, their parts as wide as `widths`, a footer
/// `footer` wide on a line under them, and a pane of `pane` lines `widest`
/// wide beside them. Every column is as wide as its widest, the candidates'
/// and the pane's no wider than [`SURFACE_WIDTH`] and what is beside them no wider than
/// [`BESIDE_WIDTH`]; that column takes no room when no row has anything
/// beside it.
pub fn layout(
    widths: &[Widths],
    line: i32,
    pane: Option<(usize, i32)>,
    footer: Option<i32>,
) -> Layout {
    let widest = |part: fn(&Widths) -> i32| widths.iter().map(part).max().unwrap_or(0);
    let number_x = PADDING;
    let surface_x = number_x + widest(|w| w.number) + PADDING;
    let surface_width = widest(|w| w.surface).min(SURFACE_WIDTH);
    let beside_x = surface_x + surface_width + PADDING * 2;
    let beside_width = widest(|w| w.beside).min(BESIDE_WIDTH);
    let rows_width = if beside_width > 0 {
        beside_x + beside_width + PADDING
    } else {
        surface_x + surface_width + PADDING
    };
    let list_width = rows_width.max(footer.map_or(0, |width| width + PADDING * 2));
    let rows_bottom = PADDING / 2 + line * widths.len() as i32;
    let footer = footer.map(|width| (list_width - width - PADDING, rows_bottom));
    let pane = pane.map(|(lines, widest)| {
        let width = widest.min(SURFACE_WIDTH) + PADDING * 2;
        (lines, (list_width, width))
    });
    let list_height = line * (widths.len() as i32 + i32::from(footer.is_some())) + PADDING;
    let pane_height = pane.map_or(0, |(lines, _)| line * lines as i32 + PADDING);
    Layout {
        number_x,
        surface_x,
        surface_width,
        beside_x,
        beside_width,
        list_width,
        pane: pane.map(|(_, at)| at),
        footer,
        width: list_width + pane.map_or(0, |(_, (_, width))| width),
        height: list_height.max(pane_height),
    }
}

/// The row at `(x, y)` in the window, among `rows` rows `line` tall in a
/// list `list_width` wide.
pub fn row_at(x: i32, y: i32, rows: usize, line: i32, list_width: i32) -> Option<usize> {
    if !(0..list_width).contains(&x) {
        return None;
    }
    let row = usize::try_from((y - PADDING / 2).div_euclid(line)).ok()?;
    (row < rows).then_some(row)
}

#[cfg(test)]
mod tests {
    use kanaemi_core::Candidate;

    use super::*;

    fn candidate(surface: &str, source: Option<&str>, preview: Option<&str>) -> Candidate {
        Candidate {
            surface: surface.to_owned(),
            source: source.map(str::to_owned),
            preview: preview.map(str::to_owned),
        }
    }

    #[test]
    fn a_page_lists_the_candidates_with_their_dictionary_or_first_candidate_beside() {
        let view = CandidateView {
            items: vec![
                candidate("漢字", Some("ユーザー辞書"), None),
                candidate("かんじ", None, Some("漢字")),
                candidate("カンジ", None, None),
            ],
            selected: 1,
            page: 1,
            pages: 3,
            more: vec!["感じ".to_owned()],
        };
        assert_eq!(
            Page::new(&view),
            Page {
                items: vec!["漢字".to_owned(), "かんじ".to_owned(), "カンジ".to_owned()],
                beside: vec![
                    Some("ユーザー辞書".to_owned()),
                    Some("漢字".to_owned()),
                    None
                ],
                selected: 1,
                more: vec!["感じ".to_owned()],
                footer: Some("‹ 2 / 3 ›".to_owned()),
            }
        );
    }

    fn widths(number: i32, surface: i32, beside: i32) -> Widths {
        Widths {
            number,
            surface,
            beside,
        }
    }

    #[test]
    fn each_column_is_as_wide_as_its_widest() {
        let laid = layout(&[widths(8, 30, 0), widths(8, 50, 40)], 20, None, None);
        assert_eq!(laid.surface_x, PADDING + 8 + PADDING);
        assert_eq!(laid.beside_x, laid.surface_x + 50 + PADDING * 2);
        assert_eq!(laid.list_width, laid.beside_x + 40 + PADDING);
        assert_eq!(
            (laid.width, laid.height),
            (laid.list_width, 20 * 2 + PADDING)
        );
        assert_eq!(laid.pane, None);
    }

    #[test]
    fn a_typical_dictionary_name_gets_its_full_width_and_only_a_very_long_one_is_cut() {
        for name in [72, 66, 260] {
            assert_eq!(
                layout(&[widths(8, 30, name)], 20, None, None).beside_width,
                name
            );
        }
        let long = layout(&[widths(8, 30, BESIDE_WIDTH * 2)], 20, None, None);
        assert_eq!(long.beside_width, BESIDE_WIDTH);
    }

    #[test]
    fn rows_with_nothing_beside_leave_no_room_for_it() {
        let laid = layout(&[widths(8, 30, 0)], 20, None, None);
        assert_eq!(laid.list_width, laid.surface_x + 30 + PADDING);
    }

    #[test]
    fn a_long_candidate_is_given_no_more_than_the_widest_a_candidate_is_shown() {
        let laid = layout(&[widths(8, SURFACE_WIDTH * 2, 0)], 20, None, None);
        assert_eq!(laid.surface_width, SURFACE_WIDTH);
    }

    #[test]
    fn the_other_candidates_take_a_pane_beside_the_list_that_may_make_it_taller() {
        let laid = layout(&[widths(8, 30, 40)], 20, Some((3, 50)), None);
        assert_eq!(laid.pane, Some((laid.list_width, 50 + PADDING * 2)));
        assert_eq!(laid.width, laid.list_width + 50 + PADDING * 2);
        assert_eq!(laid.height, 20 * 3 + PADDING);
    }

    #[test]
    fn a_long_other_candidate_is_given_no_more_than_the_widest_a_candidate_is_shown() {
        let laid = layout(&[widths(8, 30, 40)], 20, Some((3, 2400)), None);
        assert_eq!(
            laid.pane,
            Some((laid.list_width, SURFACE_WIDTH + PADDING * 2))
        );
        assert_eq!(laid.width, laid.list_width + SURFACE_WIDTH + PADDING * 2);
    }

    #[test]
    fn a_page_that_is_the_only_one_has_no_footer() {
        let view = CandidateView {
            items: vec![candidate("漢字", None, None)],
            selected: 0,
            page: 0,
            pages: 1,
            more: Vec::new(),
        };
        assert_eq!(Page::new(&view).footer, None);
    }

    #[test]
    fn the_footer_takes_a_line_under_the_rows_at_the_right_and_widens_the_list_to_fit() {
        let rows = [widths(8, 30, 0), widths(8, 30, 0)];
        let without = layout(&rows, 20, None, None);
        let laid = layout(&rows, 20, None, Some(200));
        assert_eq!(laid.footer, Some((PADDING, PADDING / 2 + 20 * 2)));
        assert_eq!(laid.list_width, 200 + PADDING * 2);
        assert_eq!(laid.height, without.height + 20);
        assert_eq!(without.footer, None);
        let narrow = layout(&rows, 20, None, Some(10));
        assert_eq!(
            narrow.footer,
            Some((narrow.list_width - 10 - PADDING, PADDING / 2 + 20 * 2))
        );
    }

    #[test]
    fn a_click_on_the_footer_picks_nothing() {
        let laid = layout(&[widths(8, 30, 0)], 20, None, Some(10));
        let (_, y) = laid.footer.unwrap();
        assert_eq!(row_at(10, y + 1, 1, 20, laid.list_width), None);
    }

    #[test]
    fn a_click_picks_the_row_under_it_in_the_list_only() {
        let at = |x, y| row_at(x, y, 3, 20, 100);
        assert_eq!(at(10, PADDING / 2 + 1), Some(0));
        assert_eq!(at(10, PADDING / 2 + 45), Some(2));
        assert_eq!(at(10, PADDING / 2 + 61), None, "below the last row");
        assert_eq!(at(150, PADDING / 2 + 1), None, "in the pane");
    }
}
