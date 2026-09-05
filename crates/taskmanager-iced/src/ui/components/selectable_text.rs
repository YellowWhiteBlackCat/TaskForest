//! Read-only selectable text — the iced port of the reference
//! `taskmanager-ui::SelectableText` semantics (GPUI-05).
//!
//! A read-only value that a user can select with the pointer and copy, the
//! way the GPUI shell's detail panels behave: click sets an anchor, dragging
//! extends the selection (a drag session keeps working past the widget's own
//! bounds — iced delivers every runtime event to the tree, the widget simply
//! stops filtering by its bounds while the session is open), double-click
//! selects a word, triple-click selects everything. Finishing a drag
//! publishes the selection to the primary clipboard on Wayland
//! (middle-click paste, the Linux reference behavior); Ctrl/Cmd-C copies
//! through the standard clipboard.
//!
//! One active selection per window, like the reference: beginning a selection
//! claims ownership ([`Message::TextSelectionClaimed`]); a widget that lost
//! ownership clears its highlight. Select-all via keyboard is deliberately
//! NOT ported — Ctrl-A belongs to the shared command vocabulary in this
//! frontend (row-summary copy), and a widget-local binding would shadow it
//! everywhere.
//!
//! The widget is single-line by contract (values, addresses, identifiers);
//! wrapping is off so a selection rectangle is one horizontal span. Selection
//! offsets are byte ranges (the reference shape); caret lookup converts bytes
//! to grapheme indices because cosmic-text's hit test and caret query speak
//! different units.

use iced::advanced::mouse::click::Kind;
use iced::advanced::text::paragraph::Plain;
use iced::advanced::text::{self, Hit, Paragraph, Wrapping};
use iced::advanced::widget::{self, Id, Tree};
use iced::advanced::{Clipboard, Layout, Renderer, Shell, Widget, layout};
use iced::{Color, Element, Event, Length, Point, Rectangle, Size};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

use crate::app::Message;

type StateParagraph = Plain<<iced::Renderer as text::Renderer>::Paragraph>;

/// One selectable read-only value. `id` must be stable per call site — it
/// names the selection owner across frames.
pub(crate) struct SelectableText {
    id: Id,
    content: String,
    size: f32,
    color: Color,
    is_owner: bool,
    width: Length,
}

impl SelectableText {
    pub(crate) fn new(id: Id, content: String, size: f32, color: Color) -> Self {
        Self {
            id,
            content,
            size,
            color,
            is_owner: false,
            width: Length::Shrink,
        }
    }

    /// Whether this widget currently owns the window's one active selection.
    /// The view resolves it against the app's selection owner each frame.
    pub(crate) fn selection_owner(mut self, is_owner: bool) -> Self {
        self.is_owner = is_owner;
        self
    }
}

#[derive(Default)]
struct State {
    paragraph: StateParagraph,
    content: String,
    last_click: Option<iced::advanced::mouse::Click>,
    /// The reference `SelectableTextState` shape: an anchor plus a byte-range
    /// selection, cleared whenever the text changes.
    anchor: Option<usize>,
    selection: Range<usize>,
    dragging: bool,
}

impl State {
    fn clear(&mut self) {
        self.anchor = None;
        self.selection = 0..0;
        self.dragging = false;
    }

    fn selected_text(&self) -> Option<String> {
        (!self.selection.is_empty()).then(|| self.content[self.selection.clone()].to_owned())
    }

    /// The byte caret for one pointer position: the hit-tested offset,
    /// clamped onto a char boundary and into the content. A miss past the
    /// text means "caret at the end" — dragging beyond the last glyph must
    /// not silently stop.
    fn caret_at(&self, position: Point, bounds: Rectangle) -> usize {
        let relative = Point::new(position.x - bounds.x, position.y - bounds.y);
        match self.paragraph.raw().hit_test(relative).map(Hit::cursor) {
            Some(index) => char_boundary_at_or_before(&self.content, index),
            None if relative.x > 0.0 && relative.y >= 0.0 => self.content.len(),
            None => 0,
        }
    }

    /// One click plants the anchor, a double-click takes the word under it,
    /// a triple-click takes everything (the reference `begin` ladder).
    fn begin(&mut self, index: usize, click: Kind) {
        let index = char_boundary_at_or_before(&self.content, index);
        match click {
            Kind::Single => {
                self.anchor = Some(index);
                self.selection = index..index;
            }
            Kind::Double => {
                self.anchor = None;
                self.selection = word_range(&self.content, index);
            }
            Kind::Triple => {
                self.anchor = None;
                self.selection = 0..self.content.len();
            }
        }
    }

    fn extend(&mut self, index: usize) {
        let Some(anchor) = self.anchor else {
            return;
        };
        let head = char_boundary_at_or_before(&self.content, index);
        self.selection = anchor.min(head)..anchor.max(head);
    }

    /// Finish a drag session; the returned selection goes to the primary
    /// clipboard (the reference writes pointer selections there too).
    fn finish(&mut self) -> Option<String> {
        self.dragging = false;
        self.anchor = None;
        self.selected_text()
    }
}

/// The reference's boundary rule: a hit offset resolves to the character it
/// points into, never the middle of a multi-byte character.
fn char_boundary_at_or_before(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// The reference's word selection: an identifier-like run of alphanumeric
/// bytes plus `.`, `:`, `-`, `_` — so `192.168.1.1` and `aa:bb:cc:dd:ee:ff`
/// each select whole around any clicked byte. A click on a separator (or
/// past the end) takes the separator alone, never a neighboring identifier.
fn word_range(text: &str, index: usize) -> Range<usize> {
    let bytes = text.as_bytes();
    let is_word =
        |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b'-' | b'_');
    let clicked = char_boundary_at_or_before(text, index);
    if clicked >= text.len() {
        return clicked..clicked;
    }
    if !is_word(bytes[clicked]) {
        let mut end = clicked + 1;
        while end < text.len() && !text.is_char_boundary(end) {
            end += 1;
        }
        return clicked..end;
    }
    let mut start = clicked;
    while start > 0 && is_word(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = clicked + 1;
    while end < text.len() && is_word(bytes[end]) {
        end += 1;
    }
    start..end
}

/// Byte caret → grapheme index: the units `hit_test` and
/// `grapheme_position` speak.
fn grapheme_index_at(text: &str, byte: usize) -> usize {
    text.get(..byte)
        .map(|prefix| prefix.graphemes(true).count())
        .unwrap_or(0)
}

impl Widget<Message, iced::Theme, iced::Renderer> for SelectableText {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State {
            content: self.content.clone(),
            ..State::default()
        })
    }

    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();
        if state.content != self.content {
            // The reference's sync rule: changed text invalidates any
            // selection, never leaves one pointing into old bytes.
            state.content = self.content.clone();
            state.clear();
        }
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        layout::sized(limits, self.width, Length::Shrink, |limits| {
            let _ = state.paragraph.update(text::Text {
                content: &state.content,
                bounds: limits.max(),
                size: self.size.into(),
                line_height: text::LineHeight::default(),
                font: iced::Font::default(),
                align_x: text::Alignment::Default,
                align_y: iced::alignment::Vertical::Top,
                shaping: text::Shaping::Auto,
                wrapping: Wrapping::None,
            });
            state.paragraph.min_bounds()
        })
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        _renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();

        match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                let copy_intent = matches!(key, iced::keyboard::Key::Character(c)
                    if c.eq_ignore_ascii_case("c"))
                    && (modifiers.control() || modifiers.logo());
                if copy_intent && let Some(selected) = state.selected_text() {
                    // The explicit copy intent goes to the standard clipboard.
                    clipboard.write(iced::advanced::clipboard::Kind::Standard, selected);
                    shell.capture_event();
                }
            }
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                let Some(position) = cursor.position() else {
                    return;
                };
                let click = iced::advanced::mouse::Click::new(
                    position,
                    iced::mouse::Button::Left,
                    state.last_click,
                );
                state.last_click = Some(click);
                let index = state.caret_at(position, bounds);
                state.begin(index, click.kind());
                state.dragging = click.kind() == Kind::Single;
                // The reference's registry rule: beginning a selection makes
                // this widget the window's one selection owner.
                shell.publish(Message::TextSelectionClaimed(self.id.clone()));
            }
            Event::Mouse(iced::mouse::Event::CursorMoved { .. }) if state.dragging => {
                if let Some(position) = cursor.position() {
                    state.extend(state.caret_at(position, bounds));
                }
            }
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left))
                if state.dragging =>
            {
                if let Some(selected) = state.finish()
                    && !selected.is_empty()
                {
                    clipboard.write(iced::advanced::clipboard::Kind::Primary, selected);
                }
            }
            _ => {}
        }

        if !self.is_owner && !state.selection.is_empty() {
            // Ownership lost to another selectable value this frame.
            state.clear();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        _cursor: iced::advanced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let raw = state.paragraph.raw();

        if !state.selection.is_empty() {
            let start_x = raw
                .grapheme_position(0, grapheme_index_at(&state.content, state.selection.start))
                .map(|point| point.x);
            let end_x = if state.selection.end >= state.content.len() {
                Some(raw.min_bounds().width)
            } else {
                raw.grapheme_position(0, grapheme_index_at(&state.content, state.selection.end))
                    .map(|point| point.x)
            };
            if let (Some(start_x), Some(end_x)) = (start_x, end_x) {
                renderer.fill_quad(
                    iced::advanced::renderer::Quad {
                        bounds: Rectangle::new(
                            Point::new(bounds.x + start_x, bounds.y),
                            Size::new((end_x - start_x).max(1.0), bounds.height),
                        ),
                        ..iced::advanced::renderer::Quad::default()
                    },
                    // The selection surface is the text color at read-back
                    // strength — visible on every skin without a second token.
                    iced::Color {
                        a: 0.28,
                        ..self.color
                    },
                );
            }
        }

        iced::widget::text::draw(
            renderer,
            style,
            bounds,
            raw,
            iced::widget::text::Style {
                color: Some(self.color),
            },
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> iced::advanced::mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            iced::mouse::Interaction::Text
        } else {
            iced::mouse::Interaction::None
        }
    }
}

impl From<SelectableText> for Element<'static, Message, iced::Theme, iced::Renderer> {
    fn from(text: SelectableText) -> Self {
        Self::new(text)
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/selectable_text_tests.rs"]
mod tests;
