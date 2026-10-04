//! Where a popup goes on the screen, beside the text it is about.

/// Where a popup of `size` goes by `at`, a screen rectangle of the text: just
/// below it, or above it when there is no room below, and moved in from the
/// edges of `work`, the part of the screen windows may use.
pub(crate) fn place(at: Rect, size: (i32, i32), gap: i32, work: Rect) -> (i32, i32) {
    let (width, height) = size;
    let below = at.bottom + gap;
    let y = if below + height <= work.bottom {
        below
    } else {
        at.top - gap - height
    };
    let x = at.left.min(work.right - width).max(work.left);
    (x, y.max(work.top))
}

/// A rectangle by its edges, as Windows gives one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    fn text(left: i32, top: i32) -> Rect {
        Rect {
            left,
            top,
            right: left + 40,
            bottom: top + 20,
        }
    }

    #[test]
    fn a_popup_goes_just_below_the_text() {
        assert_eq!(place(text(100, 200), (300, 100), 2, SCREEN), (100, 222));
    }

    #[test]
    fn a_popup_by_the_right_edge_moves_in() {
        assert_eq!(place(text(1850, 200), (300, 100), 2, SCREEN).0, 1620);
    }

    #[test]
    fn a_popup_without_room_below_goes_above() {
        assert_eq!(place(text(100, 1000), (300, 100), 2, SCREEN), (100, 898));
    }
}
