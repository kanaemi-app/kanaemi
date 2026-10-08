//! What the text services active on one thread share, kept only while one
//! of them is active.
//!
//! A thread-local is dropped as its thread ends or the DLL is unloaded, and
//! Windows runs both under the loader lock, where releasing it may hang the
//! application: dropping it runs the user's functions' collector, closes
//! files and lets COM objects go. It is therefore handed back to be dropped
//! as the last text service deactivates, and one still held when the
//! thread-local is dropped is left behind instead.

use std::mem::ManuallyDrop;

pub(crate) struct PerThread<T> {
    value: Option<ManuallyDrop<T>>,
    /// How many text services on the thread are active.
    active: usize,
}

impl<T> PerThread<T> {
    pub(crate) const fn new() -> Self {
        Self {
            value: None,
            active: 0,
        }
    }

    pub(crate) fn activate(&mut self) {
        self.active += 1;
    }

    /// The value, opened with `open` when there is none yet. While no text
    /// service is active there is none: one opened then would be held until
    /// the thread ends.
    pub(crate) fn get_or_open(&mut self, open: impl FnOnce() -> T) -> Option<&mut T> {
        if self.active == 0 {
            return None;
        }
        Some(self.value.get_or_insert_with(|| ManuallyDrop::new(open())))
    }

    /// Counts a text service deactivated; once none is active, hands the
    /// value back for the caller to drop, outside any borrow of this.
    pub(crate) fn deactivate(&mut self) -> Option<T> {
        self.active = self.active.saturating_sub(1);
        if self.active > 0 {
            return None;
        }
        self.value.take().map(ManuallyDrop::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    /// Counts how many times it was dropped.
    struct Counted(Rc<Cell<usize>>);

    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    fn counted() -> (Rc<Cell<usize>>, impl Fn() -> Counted) {
        let drops = Rc::new(Cell::new(0));
        let open = {
            let drops = drops.clone();
            move || Counted(drops.clone())
        };
        (drops, open)
    }

    #[test]
    fn the_value_is_opened_once_while_active() {
        let mut held = PerThread::new();
        let opened = Cell::new(0);
        held.activate();
        for _ in 0..3 {
            held.get_or_open(|| opened.set(opened.get() + 1));
        }
        assert_eq!(opened.get(), 1);
    }

    #[test]
    fn nothing_is_opened_while_no_text_service_is_active() {
        let mut held = PerThread::new();
        assert!(held.get_or_open(|| ()).is_none());
        held.activate();
        held.deactivate();
        assert!(held.get_or_open(|| ()).is_none());
    }

    #[test]
    fn the_value_is_handed_back_once_the_last_text_service_deactivates() {
        let mut held = PerThread::new();
        held.activate();
        held.activate();
        held.get_or_open(|| 1);
        assert_eq!(held.deactivate(), None);
        assert_eq!(held.deactivate(), Some(1));
    }

    #[test]
    fn a_text_service_active_again_opens_the_value_again() {
        let mut held = PerThread::new();
        held.activate();
        held.get_or_open(|| 1);
        held.deactivate();
        held.activate();
        assert_eq!(held.get_or_open(|| 2).copied(), Some(2));
    }

    #[test]
    fn deactivating_more_than_was_activated_keeps_the_count() {
        let mut held = PerThread::new();
        held.deactivate();
        held.activate();
        held.get_or_open(|| 1);
        assert_eq!(held.deactivate(), Some(1));
    }

    #[test]
    fn a_value_still_held_when_the_thread_ends_is_not_dropped() {
        let (drops, open) = counted();
        {
            let mut held = PerThread::new();
            held.activate();
            held.get_or_open(open);
        }
        assert_eq!(drops.get(), 0);
    }

    #[test]
    fn a_value_handed_back_is_dropped_by_the_caller() {
        let (drops, open) = counted();
        {
            let mut held = PerThread::new();
            held.activate();
            held.get_or_open(open);
            drop(held.deactivate());
        }
        assert_eq!(drops.get(), 1);
    }
}
