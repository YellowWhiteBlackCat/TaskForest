//! The anchored floating-panel primitive: the iced mirror of the GPUI
//! component layer's popover seam (GPUI-05 reference semantics).
//!
//! A [`Popover`] wraps one anchor element and one floating panel element.
//! The anchor stays in the layout tree; the panel mounts through iced's
//! overlay channel (`Widget::overlay`) for exactly the frames the wrapper
//! exists, so the panel can never be clipped by an ancestor scrollable and
//! never scrolls away with the page the way an inlined action panel does.
//! Callers construct the wrapper only while their surface is open — the
//! primitive owns no open/close state and no surface truth.
//!
//! Behavior contract (matching `taskmanager-ui/src/overlays/popup.rs` where
//! the toolkits overlap):
//! - anchoring: below the anchor when it fits, flipped above when it does
//!   not, always inside the window ([`anchor`], pure and behavior-tested);
//! - an outside press dismisses through the shared overlay policy; the
//!   reference GPUI shape forwards the same press to the background target,
//!   so this primitive does not capture it unless the policy says the frontend
//!   deliberately blocks the background. (iced delivers every runtime event to
//!   overlays regardless of pointer position; an uncaptured event reaches the
//!   base tree, a captured one skips it.)
//! - Escape, focus restore, and surface precedence stay with the shell's
//!   surface state machine.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

use crate::app::Message;
use taskmanager_theme::tokens::SPACE_1;
use taskmanager_ui_contract::{
    OverlayDismissCause, OverlayDismissContext, OverlayKind, dismiss_outcome,
};

mod anchor;

/// A floating panel anchored to one element. Construct per open surface; the
/// panel mounts above everything else until the wrapper leaves the tree.
pub(crate) struct Popover<'a> {
    anchor: Element<'a, Message, iced::Theme, iced::Renderer>,
    panel: Element<'a, Message, iced::Theme, iced::Renderer>,
    on_dismiss: Message,
    gap: f32,
    kind: OverlayKind,
}

impl<'a> Popover<'a> {
    pub(crate) fn new(
        anchor: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
        panel: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
        on_dismiss: Message,
    ) -> Self {
        Self {
            anchor: anchor.into(),
            panel: panel.into(),
            on_dismiss,
            gap: f32::from(SPACE_1),
            kind: OverlayKind::DropdownMenu,
        }
    }

    /// A row/action menu anchored to a targeted object.
    pub(crate) fn context_menu(
        anchor: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
        panel: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
        on_dismiss: Message,
    ) -> Self {
        Self {
            kind: OverlayKind::ContextMenu,
            ..Self::new(anchor, panel, on_dismiss)
        }
    }
}

impl Widget<Message, iced::Theme, iced::Renderer> for Popover<'_> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.anchor), Tree::new(&self.panel)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.anchor, &self.panel]);
    }

    fn size(&self) -> Size<Length> {
        self.anchor.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.anchor
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
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.anchor
                .as_widget_mut()
                .operate(&mut tree.children[0], layout, renderer, operation);
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
        self.anchor.as_widget_mut().update(
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
        self.anchor.as_widget().draw(
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
        self.anchor.as_widget().mouse_interaction(
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
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        let mut children = tree.children.iter_mut();
        let anchor_tree = children.next().expect("anchor tree");
        let panel_tree = children.next().expect("panel tree");

        let anchor_overlays = self.anchor.as_widget_mut().overlay(
            anchor_tree,
            layout,
            renderer,
            viewport,
            translation,
        );
        let bounds = layout.bounds();
        let panel = overlay::Element::new(Box::new(PopoverOverlay {
            position: Point::new(bounds.x + translation.x, bounds.y + translation.y),
            size: bounds.size(),
            gap: self.gap,
            on_dismiss: self.on_dismiss.clone(),
            kind: self.kind,
            panel: &mut self.panel,
            tree: panel_tree,
        }));

        Some(
            overlay::Group::with_children(anchor_overlays.into_iter().chain(Some(panel)).collect())
                .overlay(),
        )
    }
}

impl<'a> From<Popover<'a>> for Element<'a, Message, iced::Theme, iced::Renderer> {
    fn from(popover: Popover<'a>) -> Self {
        Self::new(popover)
    }
}

/// The mounted floating panel: one anchored rectangle that renders the panel
/// element above the window and dismisses on outside presses.
struct PopoverOverlay<'a, 'b> {
    /// The anchor's top-left corner in window space.
    position: Point,
    size: Size,
    gap: f32,
    on_dismiss: Message,
    kind: OverlayKind,
    panel: &'b mut Element<'a, Message, iced::Theme, iced::Renderer>,
    tree: &'b mut Tree,
}

impl overlay::Overlay<Message, iced::Theme, iced::Renderer> for PopoverOverlay<'_, '_> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let viewport = Rectangle::with_size(bounds);
        let panel_layout = self.panel.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        let placement = anchor::below(
            Rectangle::new(self.position, self.size),
            panel_layout.size(),
            viewport,
            self.gap,
        );
        layout::Node::with_children(panel_layout.size(), vec![panel_layout])
            .translate(Vector::new(placement.x, placement.y))
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let Some(panel_layout) = layout.children().next() else {
            return;
        };
        // A press that does not land on the panel closes the surface. The
        // shared policy decides whether the same press is forwarded to the
        // background target; the reference GPUI shape forwards it.
        if let Event::Mouse(iced::mouse::Event::ButtonPressed(_)) = event {
            let over_panel = cursor.is_over(panel_layout.bounds());
            if !over_panel {
                let outcome = dismiss_outcome(
                    self.kind,
                    OverlayDismissCause::OutsidePress,
                    OverlayDismissContext::single(),
                );
                if outcome.dismiss {
                    shell.publish(self.on_dismiss.clone());
                    if !outcome.forward_outside_press {
                        shell.capture_event();
                    }
                    return;
                }
            }
        }
        self.panel.as_widget_mut().update(
            self.tree,
            event,
            panel_layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &Rectangle::with_size(Size::INFINITE),
        );
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
    ) {
        let Some(panel_layout) = layout.children().next() else {
            return;
        };
        self.panel.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            panel_layout,
            cursor,
            &Rectangle::with_size(Size::INFINITE),
        );
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(panel_layout) = layout.children().next() {
            self.panel
                .as_widget_mut()
                .operate(self.tree, panel_layout, renderer, operation);
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> iced::advanced::mouse::Interaction {
        layout.children().next().map_or_else(
            iced::advanced::mouse::Interaction::default,
            |panel_layout| {
                self.panel.as_widget().mouse_interaction(
                    self.tree,
                    panel_layout,
                    cursor,
                    &Rectangle::with_size(Size::INFINITE),
                    renderer,
                )
            },
        )
    }
}
