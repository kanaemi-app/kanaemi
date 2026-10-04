//! Text with a cursor: a reading's stem and the text being registered are
//! edited the same way.

#[derive(Clone, Copy, Debug)]
pub(crate) enum CursorMove {
    Left,
    Right,
    Home,
    End,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Editable {
    text: String,
    /// In characters; `None` is the end, where typing usually goes.
    cursor: Option<usize>,
}

impl Editable {
    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Where the cursor is, in characters.
    pub(crate) fn cursor(&self) -> usize {
        self.at()
    }

    pub(crate) fn at_end(&self) -> bool {
        self.cursor.is_none()
    }

    /// The text before and after the cursor.
    pub(crate) fn split(&self) -> (&str, &str) {
        self.text.split_at(self.byte_at(self.at()))
    }

    pub(crate) fn insert(&mut self, s: &str) {
        let at = self.at();
        self.text.insert_str(self.byte_at(at), s);
        if self.cursor.is_some() {
            self.set_cursor(at + s.chars().count());
        }
    }

    /// Removes the character before the cursor; `false` when there is none.
    pub(crate) fn backspace(&mut self) -> bool {
        let at = self.at();
        if at == 0 {
            return false;
        }
        self.text.remove(self.byte_at(at - 1));
        if self.cursor.is_some() {
            self.set_cursor(at - 1);
        }
        true
    }

    /// Removes the character after the cursor.
    pub(crate) fn delete(&mut self) {
        if let Some(at) = self.cursor {
            self.text.remove(self.byte_at(at));
            self.set_cursor(at);
        }
    }

    pub(crate) fn move_cursor(&mut self, to: CursorMove) {
        let len = self.len();
        let at = self.at();
        self.set_cursor(match to {
            CursorMove::Left => at.saturating_sub(1),
            CursorMove::Right => (at + 1).min(len),
            CursorMove::Home => 0,
            CursorMove::End => len,
        });
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn at(&self) -> usize {
        self.cursor.unwrap_or_else(|| self.len())
    }

    fn set_cursor(&mut self, at: usize) {
        self.cursor = (at < self.len()).then_some(at);
    }

    fn byte_at(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map_or(self.text.len(), |(i, _)| i)
    }
}
