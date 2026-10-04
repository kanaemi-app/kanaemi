//! Dragging a row of a list to a new place.

/// Which side of `row` the dragged row would land on — `true` after it — or
/// `None` when `row` is not the one being rested on.
///
/// The direction of travel decides it: dragged downwards a row lands after
/// what it is over, upwards before it, so every place, first and last
/// included, can be reached.
pub fn drop_side(dragging: Option<usize>, target: Option<usize>, row: usize) -> Option<bool> {
    let (from, to) = (dragging?, target?);
    (to == row && from != to).then_some(from < to)
}

/// `items` with the row at `from` moved to where it was dropped on `to`.
pub fn moved<T: Clone>(items: &[T], from: usize, to: usize) -> Vec<T> {
    let mut items = items.to_vec();
    let item = items.remove(from);
    items.insert(to, item);
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_downwards_lands_after_the_row_it_is_over() {
        assert_eq!(drop_side(Some(0), Some(2), 2), Some(true));
        assert_eq!(moved(&["a", "b", "c"], 0, 2), ["b", "c", "a"]);
    }

    #[test]
    fn dragging_upwards_lands_before_the_row_it_is_over() {
        assert_eq!(drop_side(Some(2), Some(0), 0), Some(false));
        assert_eq!(moved(&["a", "b", "c"], 2, 0), ["c", "a", "b"]);
    }

    #[test]
    fn only_the_row_rested_on_gets_a_side() {
        assert_eq!(drop_side(Some(0), Some(2), 1), None);
        assert_eq!(drop_side(Some(1), Some(1), 1), None);
        assert_eq!(drop_side(None, Some(1), 1), None);
    }
}
