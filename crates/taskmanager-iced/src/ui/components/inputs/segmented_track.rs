//! Keyboard shell and widget implementation for the segmented choice control.

use iced::advanced;
use iced::advanced::layout::{Limits, Node};
use iced::advanced::mouse::{self, Button, Cursor, Interaction};
use iced::advanced::overlay;
use iced::advanced::renderer::{Quad, Style};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget};
use iced::keyboard;
use iced::keyboard::key::{Key, Named};
use iced::{
    Background, Border, Color, Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector,
};
use taskmanager_theme::tokens;

use super::{segmented_active_index, segmented_neighbor_index};
use crate::app::{FocusTarget, Message};

pub(super) struct SegmentedTrack<'a> {
    id: widget::Id,
    content: Element<'a, Message, Theme, Renderer>,
    on_change: Box<dyn Fn(usize) -> Message + 'a>,
    choices: Vec<(String, usize)>,
    active: usize,
    focus_target: FocusTarget,
    focus_color: Color,
    focus_radius: f32,
}

impl<'a> SegmentedTrack<'a> {
    // The ctor mirrors the struct's field set one-to-one; the fields are the
    // segmented control's own contract, not a grouping waiting to happen.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        id: impl Into<widget::Id>,
        content: Element<'a, Message, Theme, Renderer>,
        on_change: Box<dyn Fn(usize) -> Message + 'a>,
        choices: Vec<(String, usize)>,
        active: usize,
        focus_target: FocusTarget,
        focus_color: Color,
        focus_radius: f32,
    ) -> Self {
        Self {
            id: id.into(),
            content,
            on_change,
            choices,
            active,
            focus_target,
            focus_color,
            focus_radius,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct SegmentedState {
    focused: bool,
}

impl Focusable for SegmentedState {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Widget<Message, Theme, Renderer> for SegmentedTrack<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<SegmentedState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(SegmentedState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<SegmentedState>();
        operation.focusable(Some(&self.id), layout.bounds(), state);
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
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let focused = tree.state.downcast_ref::<SegmentedState>().focused;

        if focused
            && let Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(named),
                ..
            }) = event
        {
            match named {
                Named::Enter | Named::Space
                    if segmented_active_index(&self.choices, self.active).is_some() =>
                {
                    shell.publish(Message::Focus(self.focus_target));
                    shell.publish((self.on_change)(self.active));
                    shell.capture_event();
                    return;
                }
                Named::ArrowLeft | Named::ArrowRight => {
                    let right = matches!(named, Named::ArrowRight);
                    if let Some(neighbor) =
                        segmented_neighbor_index(&self.choices, self.active, right)
                    {
                        shell.publish(Message::Focus(self.focus_target));
                        shell.publish((self.on_change)(self.choices[neighbor].1));
                        shell.capture_event();
                        return;
                    }
                }
                _ => {}
            }
        }

        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(Button::Left))
        ) && cursor.is_over(layout.bounds())
        {
            shell.publish(Message::Focus(self.focus_target));
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
        renderer: &mut Renderer,
        theme: &Theme,
        style: &Style,
        layout: Layout<'_>,
        cursor: Cursor,
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
        if tree.state.downcast_ref::<SegmentedState>().focused {
            advanced::Renderer::fill_quad(
                renderer,
                Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        color: self.focus_color,
                        width: f32::from(tokens::SPACE_2),
                        radius: self.focus_radius.into(),
                    },
                    ..Quad::default()
                },
                Background::Color(Color::TRANSPARENT),
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> Interaction {
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
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<SegmentedTrack<'a>> for Element<'a, Message, Theme, Renderer> {
    fn from(track: SegmentedTrack<'a>) -> Self {
        Self::new(track)
    }
}
