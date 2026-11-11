//! A stateful tooltip wrapper: hover or keyboard focus opens it; pointer
//! exit, pointer press, Escape, or focus loss dismisses it. The tooltip
//! surface itself is read-only and never captures the triggering event.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::keyboard::key::Named;
use iced::keyboard::{self, Key};
use iced::{Element, Event, Length, Rectangle, Size, Vector, mouse};
use std::path::PathBuf;
use taskmanager_theme::tokens;
use taskmanager_ui_contract::{OverlayDismissCause, tooltip_dismisses};

use crate::app::Message;

#[derive(Default)]
struct State {
    open: bool,
    hovering: bool,
    pointer_suppressed: bool,
    focus_dismissed: bool,
    was_focused: bool,
    capture_marker_emitted: bool,
}

#[derive(Default)]
struct FocusedDescendant {
    focused: bool,
}

impl Operation<()> for FocusedDescendant {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        visit(self);
    }

    fn focusable(
        &mut self,
        _id: Option<&widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn Focusable,
    ) {
        self.focused |= state.is_focused();
    }
}

/// Wraps one content element and its read-only tooltip surface. Pointer hover
/// and keyboard-visible focus on a real descendant both open the hint.
pub(crate) struct Tooltip<'a> {
    content: Element<'a, Message, iced::Theme, iced::Renderer>,
    tooltip: Element<'a, Message, iced::Theme, iced::Renderer>,
    focus_visible: bool,
    gap: f32,
    capture_open_marker: Option<PathBuf>,
}

impl<'a> Tooltip<'a> {
    pub(crate) fn new(
        content: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
        tooltip: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
    ) -> Self {
        Self {
            content: content.into(),
            tooltip: tooltip.into(),
            focus_visible: false,
            gap: f32::from(tokens::SPACE_4),
            capture_open_marker: None,
        }
    }

    pub(crate) fn with_focus_visible(mut self, focus_visible: bool) -> Self {
        self.focus_visible = focus_visible;
        self
    }

    /// Keep this tooltip open and emit one mounted-surface marker for real
    /// capture. Ordinary production launches never install this path.
    pub(crate) fn open_for_capture(mut self, marker: PathBuf) -> Self {
        self.capture_open_marker = Some(marker);
        self
    }
}

fn sync_focus(state: &mut State, focus_open: bool) {
    if state.was_focused == focus_open {
        return;
    }
    state.was_focused = focus_open;
    if focus_open {
        state.pointer_suppressed = false;
        state.focus_dismissed = false;
        state.open = true;
    } else if tooltip_dismisses(OverlayDismissCause::FocusLoss) {
        if !state.pointer_suppressed {
            state.focus_dismissed = false;
        }
        state.open = false;
    }
}

impl Widget<Message, iced::Theme, iced::Renderer> for Tooltip<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State {
            open: self.capture_open_marker.is_some(),
            ..State::default()
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content), Tree::new(&self.tooltip)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content, &self.tooltip]);
        let capture_open = self.capture_open_marker.is_some();
        if capture_open {
            tree.state.downcast_mut::<State>().open = true;
        }
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
        let focused =
            focused_descendant(&mut self.content, &mut tree.children[0], layout, renderer);
        let state = tree.state.downcast_mut::<State>();
        sync_focus(state, self.focus_visible && focused);
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let over = cursor.is_over(layout.bounds());
                if over {
                    if !state.hovering {
                        state.hovering = true;
                        if !state.pointer_suppressed && !state.focus_dismissed {
                            state.open = true;
                        }
                    }
                } else if state.hovering {
                    state.hovering = false;
                    state.pointer_suppressed = false;
                    if !state.was_focused {
                        state.focus_dismissed = false;
                    }
                    if !state.was_focused && tooltip_dismisses(OverlayDismissCause::PointerExit) {
                        state.open = false;
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(_)) => {
                if tooltip_dismisses(OverlayDismissCause::PointerPress) {
                    state.open = false;
                    state.pointer_suppressed = true;
                    state.focus_dismissed = state.was_focused;
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(Named::Escape),
                ..
            }) if tooltip_dismisses(OverlayDismissCause::Escape) => {
                state.open = false;
                state.pointer_suppressed = true;
                state.focus_dismissed = state.was_focused;
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
        let focused =
            focused_descendant(&mut self.content, &mut tree.children[0], layout, renderer);
        sync_focus(
            tree.state.downcast_mut::<State>(),
            self.focus_visible && focused,
        );
        let (open, capture_marker) = {
            let state = tree.state.downcast_mut::<State>();
            let marker = (state.open && !state.capture_marker_emitted)
                .then(|| self.capture_open_marker.clone())
                .flatten();
            if marker.is_some() {
                state.capture_marker_emitted = true;
            }
            (state.open, marker)
        };
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
        if tooltip.is_some()
            && let Some(marker) = capture_marker.as_deref()
        {
            crate::capture::append_tooltip_overlay_marker(marker);
        }
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

fn focused_descendant(
    content: &mut Element<'_, Message, iced::Theme, iced::Renderer>,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
) -> bool {
    let mut focused = FocusedDescendant::default();
    content
        .as_widget_mut()
        .operate(tree, layout, renderer, &mut focused);
    focused.focused
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
