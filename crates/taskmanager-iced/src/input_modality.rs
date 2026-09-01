//! Renderer-local input modality for the Iced frontend: which input origin
//! last pressed in this window, and therefore whether focused controls paint
//! their focus ring. The value lives in [`crate::app::InputState`], not in a
//! process-global variable.
//!
//! This is the iced counterpart of the GPUI root's `InputModality` tracker
//! (taskmanager-gpui `gpui_app/root/input_modality.rs`, GPUI-05 reference
//! semantics): neither toolkit attaches an input origin to focus state, so
//! both synthesize focus-visible from one small tracker. The policy is strict
//! and identical — only keyboard input paints focus rings; programmatic focus
//! inherits the previous origin and never paints one by itself.
//!
//! Observation points mirror the GPUI capture listeners:
//! - every normalized key press (`Message::Key`, including unmappable keys
//!   and bare modifiers) marks keyboard input;
//! - the root [`Observer`] widget publishes [`Message::PointerPressed`] for
//!   any pointer button before the tree below handles the same event — the
//!   iced analog of a capture-phase mouse-down listener (a root `mouse_area`
//!   cannot do this: it captures the presses it reports).
//!
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

use crate::app::Message;

/// The most recent input origin capable of changing focus. One-to-one with
/// the GPUI root tracker's vocabulary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum InputModality {
    /// Initial state, and focus changes initiated by application code.
    #[default]
    Programmatic = 0,
    /// A keyboard event was observed (any key, including bare modifiers).
    Keyboard = 1,
    /// A pointer button was pressed over the root surface.
    Pointer = 2,
}

impl InputModality {
    /// Strict focus-visible policy: only keyboard modality paints an outset
    /// ring, matching the shared palette contract (ring alpha encodes
    /// focus-visible) and the GPUI shell's decision.
    #[must_use]
    pub(crate) const fn shows_focus_ring(self) -> bool {
        matches!(self, Self::Keyboard)
    }
}

/// The root input observer: a transparent pass-through shell around the whole
/// view tree that watches pointer presses before the wrapped tree handles the
/// same event. It never captures, so every descendant control sees the press
/// unchanged — only the one observation message is added.
pub(crate) struct Observer<'a> {
    content: Element<'a, Message, iced::Theme, iced::Renderer>,
}

impl<'a> Observer<'a> {
    pub(crate) fn new(content: Element<'a, Message, iced::Theme, iced::Renderer>) -> Self {
        Self { content }
    }
}

impl Widget<Message, iced::Theme, iced::Renderer> for Observer<'_> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.traverse(&mut |operation| {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                operation,
            );
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // Observe BEFORE forwarding: the descendant that handles the press may
        // capture it, and this tracker must see every press regardless.
        if matches!(event, Event::Mouse(iced::mouse::Event::ButtonPressed(_))) {
            shell.publish(Message::PointerPressed);
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> iced::advanced::mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Observer<'a>> for Element<'a, Message, iced::Theme, iced::Renderer> {
    fn from(observer: Observer<'a>) -> Self {
        Self::new(observer)
    }
}

#[cfg(test)]
#[path = "../tests/gui/input_modality_tests.rs"]
mod tests;
