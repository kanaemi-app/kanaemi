//! Keys the IME posts to the application. Input Method Kit hands a posted
//! key back to the IME without the mark it was posted with, so each is kept
//! here until it comes back, to be passed on unseen by the core.

use std::collections::VecDeque;

/// How long a posted key is waited for. One that never comes back is
/// dropped then, not mistaken later for a key typed.
pub const WAIT_MS: u64 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// Sent in place of the key pressed.
    InPlace,
    /// A Backspace erasing a commit undone.
    Erasing,
}

#[derive(Debug, Default)]
pub struct Posted {
    /// Key code, when posted and why, oldest first.
    waiting: VecDeque<(u16, u64, Purpose)>,
}

impl Posted {
    pub fn post(&mut self, code: u16, at_ms: u64, purpose: Purpose) {
        self.waiting.push_back((code, at_ms, purpose));
    }

    /// Why the key pressed with `code` at `at_ms` was posted, if it was: the
    /// oldest such key still waited for.
    pub fn take(&mut self, code: u16, at_ms: u64) -> Option<Purpose> {
        let fresh = |posted: u64| at_ms.saturating_sub(posted) <= WAIT_MS;
        // A Backspace erasing text that is overdue stays for `expire`, which
        // alone tells the core the text is not all gone.
        self.waiting
            .retain(|&(_, posted, purpose)| purpose == Purpose::Erasing || fresh(posted));
        let index = self
            .waiting
            .iter()
            .position(|&(c, posted, _)| c == code && fresh(posted))?;
        self.waiting.remove(index).map(|(.., purpose)| purpose)
    }

    /// Whether a Backspace erasing text is still waited for.
    pub fn erasing(&self) -> bool {
        self.waiting
            .iter()
            .any(|&(.., purpose)| purpose == Purpose::Erasing)
    }

    /// Drops the keys waited for too long as of `now_ms`; whether one of
    /// them was erasing text, which is then not all gone.
    pub fn expire(&mut self, now_ms: u64) -> bool {
        let before = self.erasing();
        self.waiting
            .retain(|&(_, posted, _)| now_ms.saturating_sub(posted) <= WAIT_MS);
        before && !self.erasing()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACKSPACE: u16 = 51;
    const A: u16 = 0;

    #[test]
    fn a_posted_key_is_known_once_when_it_comes_back() {
        let mut posted = Posted::default();
        posted.post(BACKSPACE, 100, Purpose::InPlace);
        assert_eq!(posted.take(A, 110), None);
        assert_eq!(posted.take(BACKSPACE, 110), Some(Purpose::InPlace));
        assert_eq!(posted.take(BACKSPACE, 120), None, "typed, not posted");
    }

    #[test]
    fn keys_of_the_same_code_come_back_in_the_order_posted() {
        let mut posted = Posted::default();
        posted.post(BACKSPACE, 100, Purpose::InPlace);
        posted.post(BACKSPACE, 101, Purpose::Erasing);
        assert_eq!(posted.take(BACKSPACE, 110), Some(Purpose::InPlace));
        assert!(posted.erasing());
        assert_eq!(posted.take(BACKSPACE, 111), Some(Purpose::Erasing));
        assert!(!posted.erasing());
    }

    #[test]
    fn a_key_that_never_came_back_is_not_taken_for_one_typed_later() {
        let mut posted = Posted::default();
        posted.post(BACKSPACE, 100, Purpose::InPlace);
        assert_eq!(posted.take(BACKSPACE, 100 + WAIT_MS + 1), None);
    }

    #[test]
    fn expiring_tells_whether_text_was_left_half_erased() {
        let mut posted = Posted::default();
        posted.post(BACKSPACE, 100, Purpose::Erasing);
        assert!(!posted.expire(100 + WAIT_MS));
        assert!(posted.expire(100 + WAIT_MS + 1));
        assert!(!posted.expire(100 + WAIT_MS + 2), "told once");
    }

    #[test]
    fn a_key_typed_late_leaves_the_text_half_erased_to_be_told() {
        let mut posted = Posted::default();
        posted.post(BACKSPACE, 100, Purpose::Erasing);
        assert_eq!(posted.take(A, 100 + WAIT_MS + 1), None);
        assert_eq!(posted.take(BACKSPACE, 100 + WAIT_MS + 1), None);
        assert!(posted.expire(100 + WAIT_MS + 1));
    }
}
