//! A stateful tooltip wrapper: hover or keyboard focus opens it; pointer
//! exit, pointer press, Escape, or focus loss dismisses it. The tooltip
//! surface itself is read-only and never captures the triggering event.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::keyboard::key::Named;
use iced::keyboard::{self, Key};
use iced::{Element, Event, Length, Rectangle, Size, Vector, mouse};
use taskmanager_theme::tokens;
use taskmanager_ui_contract::{
    OverlayDismissCause, OverlayDismissContext, OverlayKind, dismiss_outcome,
};

use crate::app::Message;

#[derive(Default)]
struct State {
    open: bool,
    hovering: bool,
    pointer_suppressed: bool,
    focus_dismissed: bool,
    was_focused: bool,
}

/// What opens this tooltip: a pointer hover, or hover plus keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TooltipTrigger {
    Hover,
    Focus,
}

impl TooltipTrigger {
    const fn follows_focus(self) -> bool {
        matches!(self, Self::Focus)
    }
}

/// Wraps one trigger/content element and one read-only tooltip surface.
pub(crate) struct Tooltip<'a> {
    content: Element<'a, Message, iced::Theme, iced::Renderer>,
    tooltip: Element<'a, Message, iced::Theme, iced::Renderer>,
    trigger: TooltipTrigger,
    gap: f32,
}

impl<'a> Tooltip<'a> {
    pub(crate) fn new(
        content: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
        tooltip: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
        trigger: TooltipTrigger,
    ) -> Self {
        Self {
            content: content.into(),
            tooltip: tooltip.into(),
            trigger,
            gap: f32::from(tokens::SPACE_4),
        }
    }
}

fn tooltip_dismisses(cause: OverlayDismissCause) -> bool {
    dismiss_outcome(OverlayKind::Tooltip, cause, OverlayDismissContext::single()).dismiss
}

fn sync_focus(state: &mut State, focus_open: bool) {
    if state.was_focused == focus_open {
        return;
    }
    state.was_focused = focus_open;
    state.pointer_suppressed = false;
    state.focus_dismissed = false;
    state.open = if focus_open {
        true
    } else if tooltip_dismisses(OverlayDismissCause::FocusLoss) {
        false
    } else {
        state.open
    };
}

impl Widget<Message, iced::Theme, iced::Renderer> for Tooltip<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State {
            open: self.trigger.follows_focus(),
            was_focused: self.trigger.follows_focus(),
            ..State::default()
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content), Tree::new(&self.tooltip)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content, &self.tooltip]);
        sync_focus(
            tree.state.downcast_mut::<State>(),
            self.trigger.follows_focus(),
        );
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        sync_focus(state, self.trigger.follows_focus());
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let over = cursor.is_over(layout.bounds());
                if over {
                    if !state.hovering {
                        state.hovering = true;
                        if !state.pointer_suppressed
                            && (!self.trigger.follows_focus() || !state.focus_dismissed)
                        {
                            state.open = true;
                        }
                    }
                } else if state.hovering {
                    state.hovering = false;
                    state.pointer_suppressed = false;
                    if !self.trigger.follows_focus()
                        && tooltip_dismisses(OverlayDismissCause::PointerExit)
                    {
                        state.open = false;
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(_)) => {
                if tooltip_dismisses(OverlayDismissCause::PointerPress) {
                    state.open = false;
                    state.pointer_suppressed = true;
                    state.focus_dismissed = self.trigger.follows_focus();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(Named::Escape),
                ..
            }) if tooltip_dismisses(OverlayDismissCause::Escape) => {
                state.open = false;
                state.pointer_suppressed = true;
                state.focus_dismissed = self.trigger.follows_focus();
            }
            _ => {}
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

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        let open = tree.state.downcast_ref::<State>().open;
        let (content_tree, tooltip_tree) = tree.children.split_at_mut(1);
        let content = self.content.as_widget_mut().overlay(
            &mut content_tree[0],
            layout,
            renderer,
            viewport,
            translation,
        );
        let tooltip = open.then(|| {
            overlay::Element::new(Box::new(TooltipOverlay {
                content_bounds: layout.bounds(),
                tooltip: &mut self.tooltip,
                tree: &mut tooltip_tree[0],
                gap: self.gap,
            }))
        });
        if content.is_some() || tooltip.is_some() {
            Some(
                overlay::Group::with_children(content.into_iter().chain(tooltip).collect())
                    .overlay(),
            )
        } else {
            None
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
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
}

impl<'a> From<Tooltip<'a>> for Element<'a, Message, iced::Theme, iced::Renderer> {
    fn from(tooltip: Tooltip<'a>) -> Self {
        Self::new(tooltip)
    }
}

struct TooltipOverlay<'a, 'b> {
    content_bounds: Rectangle,
    tooltip: &'b mut Element<'a, Message, iced::Theme, iced::Renderer>,
    tree: &'b mut Tree,
    gap: f32,
}

impl overlay::Overlay<Message, iced::Theme, iced::Renderer> for TooltipOverlay<'_, '_> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let viewport = Rectangle::with_size(bounds);
        let tooltip_layout = self.tooltip.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        let size = tooltip_layout.size();
        let x = (self.content_bounds.x + (self.content_bounds.width - size.width) / 2.0)
            .clamp(0.0, (viewport.width - size.width).max(0.0));
        let y = (self.content_bounds.y + self.content_bounds.height + self.gap)
            .min((viewport.height - size.height).max(0.0));
        layout::Node::with_children(size, vec![tooltip_layout]).translate(Vector::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        if let Some(child) = layout.children().next() {
            self.tooltip.as_widget().draw(
                self.tree,
                renderer,
                theme,
                style,
                child,
                cursor,
                &Rectangle::with_size(Size::INFINITE),
            );
        }
    }
}
