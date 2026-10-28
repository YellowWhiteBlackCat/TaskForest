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
//! The widget is single-line by default (values, addresses, identifiers);
//! wrapping is off so a selection rectangle is one horizontal span. A caller
//! that owns a *block* of text opts into word-wrapped lines through
//! [`SelectableText::wrapped`], and the same selection then spans the lines
//! it actually covers.
//!
//! Every caret and every selection rectangle comes from the measured line
//! geometry of the shaped paragraph (see [`crate::text_metrics`]): the line
//! under the pointer, the byte offset inside that line and the caret x of
//! each grapheme boundary are read from the shaped buffer, never estimated
//! from character counts and never pinned to a hard-coded line index.
//! Selection offsets are byte ranges (the reference shape); caret lookup
//! converts bytes to grapheme indices because cosmic-text's hit test and
//! caret query speak different units.

use iced::advanced::mouse::click::Kind;
use iced::advanced::text::{self, Hit, Paragraph};
use iced::advanced::widget::{self, Id, Tree};
use iced::advanced::{Clipboard, Layout, Renderer, Shell, Widget, layout};
use iced::{Color, Element, Event, Length, Point, Rectangle, Size};

use crate::app::Message;
use crate::text_metrics::{MeasuredParagraph, line_at_y};
use taskmanager_ui_contract::{TextEditState, char_boundary_at_or_before};

/// The wrapping of a value.
///
/// A value is one line unless it carries line breaks of its own: a block of
/// text is a block of text, and a selection that spans it must span the
/// lines it covers.
fn wrapping_for(content: &str) -> text::Wrapping {
    if content.contains('\n') {
        text::Wrapping::Word
    } else {
        text::Wrapping::None
    }
}

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
    measured: MeasuredParagraph,
    content: String,
    last_click: Option<iced::advanced::mouse::Click>,
    /// The shared byte-range selection state (the same caret/selection
    /// machine the shell search and Bevy readouts use), cleared whenever the
    /// text changes.
    edit: TextEditState,
    dragging: bool,
}

impl State {
    fn clear(&mut self) {
        self.edit.clear_selection();
        self.dragging = false;
    }

    fn selected_text(&self) -> Option<String> {
        self.edit
            .selected_text(&self.content)
            .map(ToOwned::to_owned)
    }

    /// The byte caret for one pointer position: the offset of the character
    /// the pointer landed on, resolved inside the line that owns that
    /// pointer position, clamped onto a char boundary and into the content. A
    /// miss past the text means "caret at the end" — dragging beyond the last
    /// glyph must not silently stop.
    fn caret_at(&self, position: Point, bounds: Rectangle) -> usize {
        let relative = Point::new(position.x - bounds.x, position.y - bounds.y);
        let lines = self.measured.lines();
        let Some(index) = line_at_y(lines, relative.y) else {
            return 0;
        };
        let line = &lines[index];
        // Hit-test inside the owning line's own band, so the cursor belongs
        // to that line instead of the nearest one.
        let y = relative
            .y
            .clamp(line.top, line.top + line.height.max(1.0) - 0.01);
        match self
            .measured
            .paragraph()
            .raw()
            .hit_test(Point::new(relative.x, y))
            .map(Hit::cursor)
        {
            Some(offset) => {
                let local = offset.min(line.end_byte.saturating_sub(line.start_byte));
                char_boundary_at_or_before(&self.content, line.start_byte + local)
            }
            None if relative.x > 0.0 && relative.y >= 0.0 => self.content.len(),
            None => 0,
        }
    }

    /// One click plants the anchor, a double-click takes the word under it,
    /// a triple-click takes everything (the reference `begin` ladder).
    fn begin(&mut self, index: usize, click: Kind) {
        let index = char_boundary_at_or_before(&self.content, index);
        match click {
            Kind::Single => self.edit.begin_selection(&self.content, index),
            Kind::Double => {
                self.edit.select_word_at(&self.content, index);
            }
            Kind::Triple => self.edit.select_all(&self.content),
        }
    }

    fn extend(&mut self, index: usize) {
        self.edit.extend_selection(&self.content, index);
    }

    /// Finish a drag session; the returned selection goes to the primary
    /// clipboard (the reference writes pointer selections there too). The
    /// range stays painted until another gesture replaces it.
    fn finish(&mut self) -> Option<String> {
        self.dragging = false;
        self.selected_text()
    }
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
            state.measured.update(
                &state.content,
                self.size,
                limits.max(),
                wrapping_for(&state.content),
            );
            state.measured.paragraph().min_bounds()
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
                let command = modifiers.control() || modifiers.logo();
                let copy_intent = matches!(key, iced::keyboard::Key::Character(c)
                    if c.eq_ignore_ascii_case("c"))
                    && command;
                let select_all_intent = matches!(key, iced::keyboard::Key::Character(c)
                    if c.eq_ignore_ascii_case("a"))
                    && command;
                if copy_intent && let Some(selected) = state.selected_text() {
                    // The explicit copy intent goes to the standard clipboard.
                    clipboard.write(iced::advanced::clipboard::Kind::Standard, selected);
                    shell.capture_event();
                } else if select_all_intent && self.is_owner {
                    // Select-all belongs to the readout that owns the window's
                    // one selection; otherwise Ctrl-A keeps the shared
                    // row-summary meaning.
                    state.edit.select_all(&state.content);
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

        if !self.is_owner && state.edit.selection(&state.content).is_some() {
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
        let raw = state.measured.paragraph().raw();

        if let Some(selection) = state.edit.selection(&state.content) {
            for line in state.measured.lines() {
                // The part of the selection that falls inside this line.
                let from = selection.start.max(line.start_byte);
                let to = selection.end.min(line.end_byte);
                if to <= from {
                    continue;
                }
                let from_x = line
                    .carets
                    .get(line.grapheme_index_at(&state.content, from))
                    .copied();
                let to_x = line
                    .carets
                    .get(line.grapheme_index_at(&state.content, to))
                    .copied();
                let (Some(from_x), Some(to_x)) = (from_x, to_x) else {
                    continue;
                };
                renderer.fill_quad(
                    iced::advanced::renderer::Quad {
                        bounds: Rectangle::new(
                            Point::new(bounds.x + from_x, bounds.y + line.top),
                            Size::new((to_x - from_x).max(1.0), line.height),
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

#[cfg(test)]
#[path = "../../../tests/gui/selectable_text_interaction_tests.rs"]
mod interaction_tests;
