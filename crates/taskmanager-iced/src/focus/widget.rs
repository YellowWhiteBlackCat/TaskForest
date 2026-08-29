//! Focusable interactive button and widget wrappers for keyboard accessibility in Iced.

use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{self, Clipboard, Layout, Renderer, Shell, Widget};
use iced::keyboard::key::{Key, Named};
use iced::{Background, Border, Color, Element, Event, Length, Rectangle, Size, Theme, Vector};

use crate::app::{FocusTarget, Message};

/// A thin focusable parent around an ordinary Iced button.
pub(crate) struct FocusableButton<'a> {
    id: widget::Id,
    content: Element<'a, Message, Theme, iced::Renderer>,
    on_press: Message,
    focus_target: FocusTarget,
    focus_color: Color,
    focus_radius: f32,
    activate_on_pointer: bool,
    hover_color: Option<Color>,
    right_press: Option<Message>,
}

impl<'a> FocusableButton<'a> {
    pub(crate) fn new(
        id: impl Into<widget::Id>,
        content: Element<'a, Message, Theme, iced::Renderer>,
        on_press: Message,
        focus_target: FocusTarget,
        focus_color: Color,
        focus_radius: f32,
        activate_on_pointer: bool,
    ) -> Self {
        Self {
            id: id.into(),
            content,
            on_press,
            focus_target,
            focus_color,
            focus_radius,
            activate_on_pointer,
            hover_color: None,
            right_press: None,
        }
    }

    pub(crate) fn with_hover(mut self, hover_color: Color) -> Self {
        self.hover_color = Some(hover_color);
        self
    }

    pub(crate) fn with_right_press(mut self, message: Option<Message>) -> Self {
        self.right_press = message;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct State {
    pub(crate) focused: bool,
}

impl Focusable for State {
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

impl Widget<Message, Theme, iced::Renderer> for FocusableButton<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
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

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &advanced::layout::Limits,
    ) -> advanced::layout::Node {
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
        let state = tree.state.downcast_mut::<State>();
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
        cursor: iced::advanced::mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        let focused = state.focused;

        if focused
            && matches!(
                event,
                Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: Key::Named(Named::Enter | Named::Space),
                    ..
                })
            )
        {
            shell.publish(Message::Focus(self.focus_target));
            shell.publish(self.on_press.clone());
            shell.capture_event();
            return;
        }

        if matches!(
            event,
            Event::Mouse(iced::advanced::mouse::Event::ButtonPressed(
                iced::advanced::mouse::Button::Left,
            ))
        ) && cursor.is_over(bounds)
        {
            shell.publish(Message::Focus(self.focus_target));
            if self.activate_on_pointer {
                shell.publish(self.on_press.clone());
                shell.capture_event();
                return;
            }
        }

        if matches!(
            event,
            Event::Mouse(iced::advanced::mouse::Event::ButtonPressed(
                iced::advanced::mouse::Button::Right,
            ))
        ) && cursor.is_over(bounds)
            && let Some(message) = self.right_press.clone()
        {
            shell.publish(Message::Focus(self.focus_target));
            shell.publish(message);
            shell.capture_event();
            return;
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
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let hovered = !tree.state.downcast_ref::<State>().focused
            && cursor.is_over(layout.bounds())
            && self.hover_color.is_some();
        if hovered {
            renderer.fill_quad(
                iced::advanced::renderer::Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        color: self.focus_color,
                        width: 0.0,
                        radius: self.focus_radius.into(),
                    },
                    ..iced::advanced::renderer::Quad::default()
                },
                Background::Color(self.hover_color.unwrap_or(Color::TRANSPARENT)),
            );
        }
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );

        let focused = tree.state.downcast_ref::<State>().focused;
        // Ring visibility follows the shared palette contract (the ring
        // token's alpha encodes focus-visible): only keyboard focus stays
        // opaque, per `crate::input_modality`.
        let ring_visible = focused && self.focus_color.a > 0.0;
        if ring_visible {
            renderer.fill_quad(
                iced::advanced::renderer::Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        color: self.focus_color,
                        width: crate::theme::FOCUS_RING_WIDTH,
                        radius: self.focus_radius.into(),
                    },
                    ..iced::advanced::renderer::Quad::default()
                },
                Background::Color(Color::TRANSPARENT),
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> iced::advanced::mouse::Interaction {
        if self.activate_on_pointer && cursor.is_over(layout.bounds()) {
            return iced::advanced::mouse::Interaction::Pointer;
        }
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
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<FocusableButton<'a>> for Element<'a, Message, Theme, iced::Renderer> {
    fn from(button: FocusableButton<'a>) -> Self {
        Self::new(button)
    }
}
