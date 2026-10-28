//! Toolkit-neutral editable-text caret and selection state machine.
//!
//! One authority for the editing vocabulary every frontend exposes: a caret,
//! an optional selection anchor, UTF-8-boundary moves, word/line motions,
//! selection replacement, and the delete family. Offsets are byte offsets on
//! UTF-8 character boundaries, so `String` slicing is always safe and every
//! frontend can map its native offsets onto the same state without a second
//! unit vocabulary. Renderers keep their own geometry; this module owns only
//! the semantic transitions.
//!
//! The state operates on a caller-owned buffer: the shell owns the search
//! query, while a frontend's widget tree owns its field text. Mutating
//! methods therefore take `&mut String` and report whether the buffer
//! changed.

use std::ops::Range;

/// A caret motion shared by every text-editing surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextMotion {
    /// One character left.
    Left,
    /// One character right.
    Right,
    /// The start of the previous word.
    WordLeft,
    /// The start of the next word.
    WordRight,
    /// The start of the current line.
    Home,
    /// The end of the current line.
    End,
    /// The start of the whole buffer.
    TextStart,
    /// The end of the whole buffer.
    TextEnd,
}

impl TextMotion {
    /// Whether this motion moves toward the buffer start (the edge a
    /// non-extending move collapses an active selection onto).
    #[must_use]
    const fn faces_start(self) -> bool {
        matches!(
            self,
            Self::Left | Self::WordLeft | Self::Home | Self::TextStart
        )
    }
}

/// A caret plus an optional selection anchor over a UTF-8 buffer.
///
/// Byte offsets are clamped to the buffer and back onto a character boundary
/// on every read, so a stale value from an earlier buffer can never panic a
/// slice. The anchor is `None` when no selection is being extended; a
/// selection exists only when the anchor differs from the caret.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextEditState {
    caret: usize,
    anchor: Option<usize>,
}

impl TextEditState {
    /// A caret at the buffer start with no selection.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            caret: 0,
            anchor: None,
        }
    }

    /// The raw caret offset (unclamped; read through [`Self::caret_in`]).
    #[must_use]
    pub const fn caret(self) -> usize {
        self.caret
    }

    /// The caret clamped into `text` and onto a character boundary.
    #[must_use]
    pub fn caret_in(self, text: &str) -> usize {
        char_boundary_at_or_before(text, self.caret)
    }

    /// Place the caret at `offset` and clear the selection.
    pub fn set_caret(&mut self, text: &str, offset: usize) {
        self.caret = char_boundary_at_or_before(text, offset);
        self.anchor = None;
    }

    /// Place the caret at the end of `text` and clear the selection.
    pub fn set_caret_at_end(&mut self, text: &str) {
        self.set_caret(text, text.len());
    }

    /// The normalized selection range, if the anchor differs from the caret.
    #[must_use]
    pub fn selection(self, text: &str) -> Option<Range<usize>> {
        let anchor = char_boundary_at_or_before(text, self.anchor?);
        let caret = self.caret_in(text);
        (anchor != caret).then(|| anchor.min(caret)..anchor.max(caret))
    }

    /// The selected slice, if any.
    #[must_use]
    pub fn selected_text(self, text: &str) -> Option<&str> {
        self.selection(text).map(|range| &text[range])
    }

    /// Clear the selection and keep the caret.
    pub fn clear_selection(&mut self) {
        self.anchor = None;
    }

    /// Start a pointer selection at `offset` (a plain click): the anchor and
    /// caret both land on the clicked boundary.
    pub fn begin_selection(&mut self, text: &str, offset: usize) {
        let offset = char_boundary_at_or_before(text, offset);
        self.anchor = Some(offset);
        self.caret = offset;
    }

    /// Extend the selection started by [`Self::begin_selection`] to `offset`.
    /// Without an anchor this starts a selection at the current caret.
    pub fn extend_selection(&mut self, text: &str, offset: usize) {
        if self.anchor.is_none() {
            self.anchor = Some(self.caret_in(text));
        }
        self.caret = char_boundary_at_or_before(text, offset);
    }

    /// Select the whole buffer.
    pub fn select_all(&mut self, text: &str) {
        self.anchor = Some(0);
        self.caret = text.len();
    }

    /// Select the word (or the single separator character) at `offset` and
    /// return the selected range.
    pub fn select_word_at(&mut self, text: &str, offset: usize) -> Range<usize> {
        let range = word_range(text, offset);
        self.anchor = Some(range.start);
        self.caret = range.end;
        range
    }

    /// Select the line containing `offset` (the line terminator excluded) and
    /// return the selected range.
    pub fn select_line_at(&mut self, text: &str, offset: usize) -> Range<usize> {
        let range = line_range(text, offset);
        self.anchor = Some(range.start);
        self.caret = range.end;
        range
    }

    /// Move the caret by one [`TextMotion`]. With `select`, the selection
    /// anchor is preserved and grown; without it, an active selection
    /// collapses onto the edge the motion faces before the caret moves.
    /// Returns whether the caret (or selection) changed.
    pub fn move_caret(&mut self, text: &str, motion: TextMotion, select: bool) -> bool {
        let caret = self.caret_in(text);
        if !select && let Some(range) = self.selection(text) {
            let target = if motion.faces_start() {
                range.start
            } else {
                range.end
            };
            self.caret = target;
            self.anchor = None;
            return true;
        }
        let target = match motion {
            TextMotion::Left => previous_boundary(text, caret),
            TextMotion::Right => next_boundary(text, caret),
            TextMotion::WordLeft => word_left(text, caret),
            TextMotion::WordRight => word_right(text, caret),
            TextMotion::Home => line_start(text, caret),
            TextMotion::End => line_end(text, caret),
            TextMotion::TextStart => 0,
            TextMotion::TextEnd => text.len(),
        };
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = target;
        target != caret
    }

    /// Insert one character at the caret, replacing any selection.
    pub fn insert_char(&mut self, text: &mut String, character: char) -> bool {
        if character.is_control() {
            return false;
        }
        self.replace_selection(text);
        let caret = self.caret_in(text);
        text.insert(caret, character);
        self.caret = caret + character.len_utf8();
        self.anchor = None;
        true
    }

    /// Insert a string at the caret, replacing any selection. With
    /// `single_line`, line breaks and tabs collapse into one space (runs
    /// never double) and other control bytes are dropped; without it,
    /// line breaks are preserved and other controls are dropped.
    pub fn insert_str(&mut self, text: &mut String, input: &str, single_line: bool) -> bool {
        let mut changed = self.selection(text).is_some();
        self.replace_selection(text);
        let mut caret = self.caret_in(text);
        for character in input.chars() {
            let inserted = match character {
                '\r' => None,
                '\n' | '\t' if single_line => {
                    if text[..caret].ends_with(' ') {
                        None
                    } else {
                        Some(' ')
                    }
                }
                '\n' => Some('\n'),
                '\t' => Some('\t'),
                character if character.is_control() => None,
                character => Some(character),
            };
            if let Some(character) = inserted {
                text.insert(caret, character);
                caret += character.len_utf8();
                changed = true;
            }
        }
        self.caret = caret;
        self.anchor = None;
        changed
    }

    /// Delete the selection, or the character before the caret.
    pub fn delete_backward(&mut self, text: &mut String) -> bool {
        if self.selection(text).is_some() {
            self.replace_selection(text);
            return true;
        }
        let caret = self.caret_in(text);
        if caret == 0 {
            return false;
        }
        let start = previous_boundary(text, caret);
        text.replace_range(start..caret, "");
        self.caret = start;
        self.anchor = None;
        true
    }

    /// Delete the selection, or the character after the caret.
    pub fn delete_forward(&mut self, text: &mut String) -> bool {
        if self.selection(text).is_some() {
            self.replace_selection(text);
            return true;
        }
        let caret = self.caret_in(text);
        if caret >= text.len() {
            return false;
        }
        let end = next_boundary(text, caret);
        text.replace_range(caret..end, "");
        self.caret = caret;
        self.anchor = None;
        true
    }

    /// Delete the selection, or the previous word (plus the separators before
    /// it).
    pub fn delete_word_backward(&mut self, text: &mut String) -> bool {
        if self.selection(text).is_some() {
            self.replace_selection(text);
            return true;
        }
        let caret = self.caret_in(text);
        let mut start = word_left(text, caret);
        if start == caret {
            return false;
        }
        while start > 0 {
            let previous = previous_boundary(text, start);
            let Some(character) = text[previous..start].chars().next() else {
                break;
            };
            if is_word_char(character) {
                break;
            }
            start = previous;
        }
        text.replace_range(start..caret, "");
        self.caret = start;
        self.anchor = None;
        true
    }

    /// Delete the selection, or the next word (plus the separators after it).
    pub fn delete_word_forward(&mut self, text: &mut String) -> bool {
        if self.selection(text).is_some() {
            self.replace_selection(text);
            return true;
        }
        let caret = self.caret_in(text);
        let end = word_right(text, caret);
        if end == caret {
            return false;
        }
        text.replace_range(caret..end, "");
        self.caret = caret;
        self.anchor = None;
        true
    }

    /// Clear the whole buffer and place the caret at the start.
    pub fn clear(&mut self, text: &mut String) -> bool {
        let changed = !text.is_empty() || self.selection(text).is_some();
        text.clear();
        self.caret = 0;
        self.anchor = None;
        changed
    }

    fn replace_selection(&mut self, text: &mut String) {
        if let Some(range) = self.selection(text) {
            text.replace_range(range.clone(), "");
            self.caret = range.start;
            self.anchor = None;
        }
    }
}

/// Clamp `index` into `text` and back onto a UTF-8 character boundary.
#[must_use]
pub fn char_boundary_at_or_before(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// The word (or single separator character) at `index`.
///
/// A word is a contiguous run of alphanumeric characters plus `.`, `:`, `-`,
/// and `_` — identifiers, addresses, and MAC-like values select whole. A
/// click on a separator selects that separator alone.
#[must_use]
pub fn word_range(text: &str, index: usize) -> Range<usize> {
    let index = char_boundary_at_or_before(text, index);
    let Some(character) = text[index..].chars().next() else {
        return index..index;
    };
    if !is_word_char(character) {
        return index..index + character.len_utf8();
    }
    let mut start = index;
    for (offset, character) in text[..index].char_indices().rev() {
        if is_word_char(character) {
            start = offset;
        } else {
            break;
        }
    }
    let mut end = index + character.len_utf8();
    for character in text[end..].chars() {
        if !is_word_char(character) {
            break;
        }
        end += character.len_utf8();
    }
    start..end
}

/// The line containing `index`, excluding its terminator.
#[must_use]
pub fn line_range(text: &str, index: usize) -> Range<usize> {
    let index = char_boundary_at_or_before(text, index);
    line_start(text, index)..line_end(text, index)
}

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '.' | ':' | '-' | '_')
}

fn previous_boundary(text: &str, index: usize) -> usize {
    text[..index]
        .char_indices()
        .next_back()
        .map_or(0, |(offset, _)| offset)
}

fn next_boundary(text: &str, index: usize) -> usize {
    text[index..]
        .chars()
        .next()
        .map_or(text.len(), |character| index + character.len_utf8())
}

fn word_left(text: &str, index: usize) -> usize {
    let mut index = index;
    while index > 0 {
        let previous = previous_boundary(text, index);
        let Some(character) = text[previous..index].chars().next() else {
            break;
        };
        if is_word_char(character) {
            break;
        }
        index = previous;
    }
    while index > 0 {
        let previous = previous_boundary(text, index);
        let Some(character) = text[previous..index].chars().next() else {
            break;
        };
        if !is_word_char(character) {
            break;
        }
        index = previous;
    }
    index
}

fn word_right(text: &str, index: usize) -> usize {
    let mut index = index;
    // Finish the current word, then cross its trailing separators so a
    // forward word delete takes the whole `word + gap` run.
    while index < text.len() {
        let Some(character) = text[index..].chars().next() else {
            break;
        };
        if !is_word_char(character) {
            break;
        }
        index += character.len_utf8();
    }
    while index < text.len() {
        let Some(character) = text[index..].chars().next() else {
            break;
        };
        if is_word_char(character) {
            break;
        }
        index += character.len_utf8();
    }
    index
}

fn line_start(text: &str, index: usize) -> usize {
    text[..index].rfind('\n').map_or(0, |offset| offset + 1)
}

fn line_end(text: &str, index: usize) -> usize {
    text[index..]
        .find('\n')
        .map_or(text.len(), |offset| index + offset)
}

#[cfg(test)]
#[path = "../tests/headless/ui_text_edit.rs"]
mod tests;
