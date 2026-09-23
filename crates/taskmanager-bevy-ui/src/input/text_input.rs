//! Text input editing state machine for Bevy UI.

use bevy::prelude::Resource;

/// State for text input editing (cursor position, selection, clipboard buffer).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TextInputState {
    /// Cursor character index (0..=text_len).
    pub(crate) cursor: usize,
    /// Selection anchor character index if a range is selected.
    pub(crate) selection_anchor: Option<usize>,
    /// Clipboard content for paste operations.
    pub(crate) clipboard: String,
}

impl TextInputState {
    /// Bounded cursor position in character count.
    #[must_use]
    pub(crate) fn cursor_pos(&self, text: &str) -> usize {
        self.cursor.min(text.chars().count())
    }

    /// Move the cursor left by one character.
    pub(crate) fn move_left(&mut self, text: &str) {
        self.cursor = self.cursor_pos(text).saturating_sub(1);
        self.selection_anchor = None;
    }

    /// Move the cursor right by one character.
    pub(crate) fn move_right(&mut self, text: &str) {
        let count = text.chars().count();
        self.cursor = (self.cursor_pos(text) + 1).min(count);
        self.selection_anchor = None;
    }

    /// Move cursor to the start of the line.
    pub(crate) fn move_home(&mut self) {
        self.cursor = 0;
        self.selection_anchor = None;
    }

    /// Move cursor to the end of the line.
    pub(crate) fn move_end(&mut self, text: &str) {
        self.cursor = text.chars().count();
        self.selection_anchor = None;
    }

    /// Move cursor one word to the left.
    pub(crate) fn move_word_left(&mut self, text: &str) {
        let current = self.cursor_pos(text);
        let chars: Vec<char> = text.chars().collect();
        if current == 0 {
            return;
        }
        let mut idx = current;
        while idx > 0 && chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        while idx > 0 && !chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        self.cursor = idx;
        self.selection_anchor = None;
    }

    /// Move cursor one word to the right.
    pub(crate) fn move_word_right(&mut self, text: &str) {
        let current = self.cursor_pos(text);
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        if current >= len {
            return;
        }
        let mut idx = current;
        while idx < len && !chars[idx].is_whitespace() {
            idx += 1;
        }
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }
        self.cursor = idx;
        self.selection_anchor = None;
    }

    /// Insert a character at the current cursor position.
    pub(crate) fn insert_char(&mut self, text: &mut String, ch: char) {
        let current = self.cursor_pos(text);
        let mut chars: Vec<char> = text.chars().collect();
        chars.insert(current, ch);
        *text = chars.into_iter().collect();
        self.cursor = current + 1;
        self.selection_anchor = None;
    }

    /// Insert a string at the current cursor position.
    pub(crate) fn insert_str(&mut self, text: &mut String, s: &str) {
        let current = self.cursor_pos(text);
        let mut chars: Vec<char> = text.chars().collect();
        for (i, ch) in s.chars().enumerate() {
            chars.insert(current + i, ch);
        }
        self.cursor = current + s.chars().count();
        self.selection_anchor = None;
        *text = chars.into_iter().collect();
    }

    /// Delete character before cursor (Backspace).
    pub(crate) fn delete_backward(&mut self, text: &mut String) -> bool {
        let current = self.cursor_pos(text);
        if current == 0 {
            return false;
        }
        let mut chars: Vec<char> = text.chars().collect();
        chars.remove(current - 1);
        *text = chars.into_iter().collect();
        self.cursor = current - 1;
        self.selection_anchor = None;
        true
    }

    /// Delete word before cursor (Ctrl+Backspace).
    pub(crate) fn delete_word_backward(&mut self, text: &mut String) -> bool {
        let current = self.cursor_pos(text);
        if current == 0 {
            return false;
        }
        let prev = {
            let mut s = self.clone();
            s.move_word_left(text);
            s.cursor
        };
        let mut chars: Vec<char> = text.chars().collect();
        chars.drain(prev..current);
        *text = chars.into_iter().collect();
        self.cursor = prev;
        self.selection_anchor = None;
        true
    }

    /// Delete character after cursor (Delete key).
    pub(crate) fn delete_forward(&mut self, text: &mut String) -> bool {
        let current = self.cursor_pos(text);
        let mut chars: Vec<char> = text.chars().collect();
        if current >= chars.len() {
            return false;
        }
        chars.remove(current);
        *text = chars.into_iter().collect();
        self.selection_anchor = None;
        true
    }

    /// Clear the entire line (Ctrl+U).
    pub(crate) fn clear_line(&mut self, text: &mut String) {
        text.clear();
        self.cursor = 0;
        self.selection_anchor = None;
    }

    /// Copy current text to clipboard.
    pub(crate) fn copy_to_clipboard(&mut self, text: &str) {
        self.clipboard = text.to_owned();
    }

    /// Cut current text to clipboard.
    pub(crate) fn cut_to_clipboard(&mut self, text: &mut String) {
        self.clipboard = text.clone();
        self.clear_line(text);
    }

    /// Paste from clipboard at cursor position.
    pub(crate) fn paste_from_clipboard(&mut self, text: &mut String) {
        let clip = self.clipboard.clone();
        if !clip.is_empty() {
            self.insert_str(text, &clip);
        }
    }
}
