//! gpui `EntityInputHandler` bridge for the single-line text input
//! (absorption §6.2 protocol; byte↔UTF16 conversion happens inside the trait
//! methods).

use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, UTF16Selection, Window};

use super::TextInputState;

/// gpui input-handler bridge (absorption 6.2 protocol; byte↔UTF16 conversion
/// happens inside the trait methods).
impl EntityInputHandler for TextInputState {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let (start, end) = byte_range_from_utf16(&self.text, range_utf16);
        *adjusted_range = Some(start..end);
        Some(self.text[start..end].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let range = utf16_range_from_bytes(&self.text, self.selection.range.clone());
        Some(UTF16Selection {
            range,
            reversed: self.selection.reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.ime_marked_range
            .clone()
            .map(|range| utf16_range_from_bytes(&self.text, range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.ime_marked_range = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(range_utf16) = range_utf16 {
            let (start, end) = byte_range_from_utf16(&self.text, range_utf16);
            self.selection.select(start..end, false);
        }
        self.replace_selection(text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        // IME composition: no mask, no whole-string validation (6.6-3).
        let range = match range_utf16 {
            Some(range) => {
                let (start, end) = byte_range_from_utf16(&self.text, range);
                start..end
            }
            None => self.selection.range.clone(),
        };
        if new_text.is_empty() {
            // Esc during composition cancels the marked text.
            self.ime_marked_range = None;
            cx.notify();
            return;
        }
        let mut pending = self.text.clone();
        pending.replace_range(range.clone(), new_text);
        self.text = pending;
        let start = range.start;
        self.ime_marked_range = Some(start..start + new_text.len());
        let new_caret = match new_selected_range {
            Some(sel) => {
                let (_, end) = byte_range_from_utf16(&self.text, sel);
                end
            }
            None => start + new_text.len(),
        };
        self.selection.collapse_to(new_caret);
        self.pause_blink(cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // Single-line: the caret row is the element's own bounds.
        Some(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        // Approximate: map the x fraction to a byte offset by character count.
        let width = f32::from(point.x).max(1.0);
        let chars = self.text.chars().count();
        let fraction = width / 1000.0;
        let ix = (fraction * chars as f32) as usize;
        Some(
            self.text
                .char_indices()
                .nth(ix)
                .map(|(b, _)| b)
                .unwrap_or(self.text.len()),
        )
    }
}

/// Convert a UTF-16 range to a byte range in `text`.
pub(super) fn byte_range_from_utf16(text: &str, range: Range<usize>) -> (usize, usize) {
    let mut byte = 0;
    let mut utf16 = 0;
    let mut start = None;
    for ch in text.chars() {
        if utf16 == range.start {
            start = Some(byte);
        }
        if utf16 == range.end {
            return (start.unwrap_or(0), byte);
        }
        byte += ch.len_utf8();
        utf16 += ch.len_utf16();
    }
    if utf16 == range.start {
        start = Some(byte);
    }
    (start.unwrap_or(0), byte)
}

/// Convert a byte range to a UTF-16 range in `text`.
pub(super) fn utf16_range_from_bytes(text: &str, range: Range<usize>) -> Range<usize> {
    let mut utf16 = 0;
    let mut start = None;
    for (byte, ch) in text.char_indices() {
        if byte == range.start {
            start = Some(utf16);
        }
        if byte == range.end {
            return (start.unwrap_or(0))..utf16;
        }
        utf16 += ch.len_utf16();
    }
    if text.len() == range.end {
        start = start.or(Some(utf16));
    }
    (start.unwrap_or(0))..utf16
}
