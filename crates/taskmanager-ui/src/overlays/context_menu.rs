//! Right-click context menu host (absorption §3.5 `ContextMenuExt`).

use std::rc::Rc;

use crate::MenuBuilder;
use gpui::{
    App, Bounds, DismissEvent, Element, ElementId, Entity, FocusHandle, GlobalElementId,
    InspectorElementId, InteractiveElement, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    ParentElement, Pixels, Styled, Subscription, Window,
};
use taskmanager_theme::Palette;

use crate::overlays::popup::PopupMenuState;

/// Extension trait: attach a right-click context menu to any element.
/// The menu entity is rebuilt on every open (dynamic menus; absorption
/// 3.6-7: no entity/state leaks because the host owns and replaces it).
pub trait ContextMenuExt:
    ParentElement + Styled + Sized + Element + InteractiveElement + 'static
{
    /// Attach a context menu built by `f` from a fresh [`PopupMenuState`].
    fn context_menu<F>(self, id: impl Into<ElementId>, palette: Palette, f: F) -> ContextMenu<Self>
    where
        F: Fn(PopupMenuState, &mut App) -> PopupMenuState + 'static,
    {
        ContextMenu::new(id, self, palette, f)
    }
}

impl<E: ParentElement + Styled + Sized + Element + InteractiveElement + 'static> ContextMenuExt
    for E
{
}

/// Element state for the context menu host (per-element, survives frames).
pub struct ContextMenuState {
    open: bool,
    bounds: Bounds<Pixels>,
    menu: Option<Entity<PopupMenuState>>,
    /// Focus anchor restored when the menu dismisses (absorption 3.3-B).
    focus_handle: FocusHandle,
    /// Menu-entity subscription closing the menu on `DismissEvent`
    /// (dismissal paths: item activation, outside click, Escape). Without
    /// this subscriber `open` stays `true` and the menu renders forever —
    /// the regression this field locks out.
    dismiss_subscription: Option<Subscription>,
}

/// A wrapper element that opens a popup menu at the right-click position.
pub struct ContextMenu<E> {
    id: ElementId,
    element: Option<E>,
    builder: MenuBuilder,
    palette: Palette,
}

impl<E: ParentElement + Styled + Sized + IntoElement + InteractiveElement + 'static>
    ContextMenu<E>
{
    /// Wrap `element` with a context menu.
    pub fn new(
        id: impl Into<ElementId>,
        element: E,
        palette: Palette,
        builder: impl Fn(PopupMenuState, &mut App) -> PopupMenuState + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            element: Some(element),
            builder: Rc::new(builder),
            palette,
        }
    }
}

impl<E: ParentElement + Styled + Sized + Element + InteractiveElement + 'static> IntoElement
    for ContextMenu<E>
{
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<E: ParentElement + Styled + Sized + Element + InteractiveElement + 'static> Element
    for ContextMenu<E>
{
    type RequestLayoutState = (E::RequestLayoutState, Entity<ContextMenuState>);
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let state = window.use_state(cx, |_window, cx| ContextMenuState {
            open: false,
            bounds: Bounds::default(),
            menu: None,
            focus_handle: cx.focus_handle().tab_stop(true),
            dismiss_subscription: None,
        });
        let (open, menu) = state.read_with(cx, |state, _| (state.open, state.menu.clone()));

        // Right-click detection lives on the WRAPPED element, not the window
        // (absorption 3.6-8): the element is rebuilt every frame, so its
        // hitbox/listener are recreated per frame and never accumulate — a
        // window-level listener registered during paint would pile up one
        // registration per frame (N listeners, N opens per right-click).
        // The element-level hitbox self-scopes to the wrapped bounds, so no
        // geometric bounds check is needed. The menu opens at the exact
        // right-click position (the component anchors itself; the host only
        // mounts the entity as a child element).
        let builder = self.builder.clone();
        let palette = self.palette;
        let state_for_click = state.clone();
        let element = self.element.take().expect("element must be set");
        let element = element.on_mouse_down(
            MouseButton::Right,
            move |event: &MouseDownEvent, window, cx| {
                // Read the focus anchor before opening so the host state is
                // not borrowed across the menu-entity construction.
                let anchor = state_for_click.read_with(cx, |state, _| state.focus_handle.clone());
                let menu = PopupMenuState::open(
                    |state, cx| builder(state, cx),
                    Some(anchor),
                    palette,
                    event.position,
                    window,
                    cx,
                );
                state_for_click.update(cx, |state, cx| {
                    state.open = true;
                    state.dismiss_subscription =
                        Some(cx.subscribe(&menu, |state, _menu, _: &DismissEvent, cx| {
                            state.open = false;
                            state.menu = None;
                            cx.notify();
                        }));
                    state.menu = Some(menu);
                });
                window.refresh();
            },
        );

        let mut element = element;
        if open && let Some(menu) = menu {
            element = element.child(menu);
        }

        // The wrapped element is usually `Stateful` (rows, buttons): it needs
        // its `GlobalElementId` so gpui can key its element state (focus
        // handles, hitboxes, click/drag state). Passing `None` here silently
        // drops that state — for a Stateful<Div> inside a `uniform_list` the
        // measurement came back degenerate and the row never rendered.
        // Push the WRAPPED element's id (the row/button), layered on top of
        // the global id the parent already pushed for this ContextMenu. The
        // wrapped element is usually `Stateful` and needs its own global id to
        // key its element state (focus handles, hitboxes, click/drag state);
        // passing `None` silently drops that state — inside a `uniform_list`
        // the measurement came back degenerate and rows never rendered.
        let (layout_id, request_layout) = match Element::id(&element) {
            Some(element_id) => window.with_global_id(element_id, |global_id, window| {
                element.request_layout(Some(global_id), inspector_id, window, cx)
            }),
            None => element.request_layout(None, inspector_id, window, cx),
        };

        self.element = Some(element);
        (layout_id, (request_layout, state))
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        request.1.update(cx, |s, _| {
            s.bounds = bounds;
        });
        // Forward the lifecycle: the wrapped row must prepaint/paint like any
        // other element, or it never draws (and its hitboxes never register).
        let mut element = self.element.take().expect("element must be set");
        let prepaint = match Element::id(&element) {
            Some(element_id) => window.with_global_id(element_id, |global_id, window| {
                element.prepaint(
                    Some(global_id),
                    inspector_id,
                    bounds,
                    &mut request.0,
                    window,
                    cx,
                )
            }),
            None => element.prepaint(None, inspector_id, bounds, &mut request.0, window, cx),
        };
        self.element = Some(element);
        prepaint
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut element = self.element.take().expect("element must be set");
        match Element::id(&element) {
            Some(element_id) => window.with_global_id(element_id, |global_id, window| {
                element.paint(
                    Some(global_id),
                    inspector_id,
                    bounds,
                    &mut request.0,
                    prepaint,
                    window,
                    cx,
                )
            }),
            None => element.paint(
                None,
                inspector_id,
                bounds,
                &mut request.0,
                prepaint,
                window,
                cx,
            ),
        };
        self.element = Some(element);
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_overlays_context_menu_tests.rs"]
mod tests;
