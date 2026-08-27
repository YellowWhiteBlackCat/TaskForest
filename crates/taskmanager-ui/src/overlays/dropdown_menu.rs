//! Dropdown menu: a trigger element that opens a cached popup menu anchored
//! at the trigger (absorption §3.5 `DropdownMenu`).

use std::cell::RefCell;
use std::rc::Rc;

use crate::MenuBuilder;
use gpui::{
    App, Bounds, ClickEvent, DismissEvent, Element, ElementId, Entity, FocusHandle,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    ParentElement, Pixels, Point, StatefulInteractiveElement, Subscription, Window,
};
use taskmanager_theme::Palette;

use crate::overlays::popup::PopupMenuState;

/// Dropdown state (element state entity, held across frames).
struct DropdownMenuState {
    open: bool,
    position: Point<Pixels>,
    menu: Option<Entity<PopupMenuState>>,
    focus_handle: FocusHandle,
    /// Menu-entity subscription closing the dropdown on `DismissEvent`
    /// (menu dismissal: item confirm, outside click, Escape).
    dismiss_subscription: Option<Subscription>,
}

/// A trigger element with an attached dropdown menu. The menu entity is
/// cached and rebuilt only when closed (absorption 3.6-7: the content
/// closure runs every render, so entity creation lives in the open path).
pub struct DropdownMenu<E> {
    id: ElementId,
    element: Option<E>,
    builder: MenuBuilder,
    palette: Palette,
    /// Element-state entity captured during `request_layout` and reused in
    /// `prepaint` to pin the popup anchor (prepaint must not call
    /// `use_state`; the phases share via this per-frame field).
    anchor_state: Rc<RefCell<Option<Entity<DropdownMenuState>>>>,
}

impl<E: StatefulInteractiveElement + ParentElement + Element + 'static> DropdownMenu<E> {
    /// Wrap `element` (the trigger) with a dropdown menu.
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
            anchor_state: Rc::new(RefCell::new(None)),
        }
    }
}

impl<E: StatefulInteractiveElement + ParentElement + Element + 'static> IntoElement
    for DropdownMenu<E>
{
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<E: StatefulInteractiveElement + ParentElement + Element + 'static> Element
    for DropdownMenu<E>
{
    type RequestLayoutState = E::RequestLayoutState;
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
        let state = window.use_state(cx, |_window, cx| DropdownMenuState {
            focus_handle: cx.focus_handle().tab_stop(true),
            open: false,
            position: Point::new(Pixels::ZERO, Pixels::ZERO),
            menu: None,
            dismiss_subscription: None,
        });
        *self.anchor_state.borrow_mut() = Some(state.clone());
        let (open, menu) = state.read_with(cx, |state, _| (state.open, state.menu.clone()));

        // Open the menu (build the entity once, move focus in); right-click
        // on the trigger also opens it (native context-menu semantics)
        // without closing a popup already open. While open, the cached
        // entity is reused and only re-presented at the live trigger anchor.
        let open_menu = {
            let state = state.clone();
            let builder = self.builder.clone();
            let palette = self.palette;
            move |window: &mut Window, cx: &mut App| {
                if state.read_with(cx, |state, _| state.open) {
                    return;
                }
                let (anchor, position) =
                    state.read_with(cx, |state, _| (state.focus_handle.clone(), state.position));
                let menu = PopupMenuState::open(
                    |state, cx| builder(state, cx),
                    Some(anchor),
                    palette,
                    position,
                    window,
                    cx,
                );
                state.update(cx, |state, cx| {
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
            }
        };
        // Click toggles; closing also drops the menu entity so a later
        // reopen rebuilds it with fresh item state.
        let toggle = {
            let state = state.clone();
            let open_menu = open_menu.clone();
            move |window: &mut Window, cx: &mut App| {
                if state.read_with(cx, |state, _| state.open) {
                    state.update(cx, |state, cx| {
                        state.open = false;
                        state.menu = None;
                        cx.notify();
                    });
                    window.refresh();
                } else {
                    open_menu(window, cx);
                }
            }
        };
        let element = self.element.take().expect("trigger must be set");
        // The trigger is focusable (tab stop); gpui's default focused
        // element behavior dispatches a click on Enter/Space (KeyUp), so the
        // keyboard path and the mouse path converge on `toggle` below.
        let element = element
            .track_focus(&state.read_with(cx, |state, _| state.focus_handle.clone()))
            .on_click({
                let toggle = toggle.clone();
                move |_event: &ClickEvent, window, cx| toggle(window, cx)
            })
            // Right-click opens the menu only when it lands ON the trigger
            // itself (native context-menu semantics). Previously this used
            // `on_mouse_down_out`, so a right-click ANYWHERE else in the
            // window — e.g. on a table row — also opened the dropdown.
            .on_mouse_down(MouseButton::Right, {
                let open_menu = open_menu.clone();
                move |_event: &MouseDownEvent, window, cx| open_menu(window, cx)
            });

        let mut element = element;
        if open && let Some(menu) = menu {
            element = element.child(menu);
        }

        // The trigger is a Stateful element: it needs its GlobalElementId so
        // gpui can key its element state (focus handle, hitbox, click state).
        // Passing `None` dropped that state and the click handler never fired.
        let (layout_id, request_layout) = match Element::id(&element) {
            Some(element_id) => window.with_global_id(element_id, |global_id, window| {
                element.request_layout(Some(global_id), inspector_id, window, cx)
            }),
            None => element.request_layout(None, inspector_id, window, cx),
        };

        // Anchor the menu below the trigger: record the trigger's laid-out
        // bottom-left corner (read by the next frame's request_layout, so
        // the popup lands next to the trigger instead of the window origin).
        let _ = layout_id;

        self.element = Some(element);
        (layout_id, request_layout)
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
        // Pin the popup below the trigger. Prepaint is the right phase:
        // `bounds` are the trigger's laid-out bounds here, and
        // `window.layout_bounds` must NOT be called during request_layout
        // (it returns stale/zero bounds there and corrupts the layout pass).
        // Writing only on change keeps the element-state entity clean so the
        // trigger's Stateful element state (hitbox, focus) survives. When
        // the trigger moves while the menu is open, the live anchor is
        // re-presented on the menu entity (it renders from its own field);
        // a mid-draw notify is dropped by design, but the write lands and
        // the trigger's own layout shift re-renders the menu view through
        // its bounds cache key.
        if let Some(state) = self.anchor_state.borrow().clone() {
            let anchor = bounds.bottom_left();
            let moved = state.read_with(cx, |state, _| state.position != anchor);
            if moved {
                let palette = self.palette;
                state.update(cx, |state, cx| {
                    state.position = anchor;
                    if let Some(menu) = state.menu.clone() {
                        menu.update(cx, |menu, cx| {
                            menu.present(palette, anchor);
                            cx.notify();
                        });
                    }
                });
            }
        }
        let mut element = self.element.take().expect("trigger must be set");
        let prepaint = match Element::id(&element) {
            Some(element_id) => window.with_global_id(element_id, |global_id, window| {
                element.prepaint(Some(global_id), inspector_id, bounds, request, window, cx)
            }),
            None => element.prepaint(None, inspector_id, bounds, request, window, cx),
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
        let mut element = self.element.take().expect("trigger must be set");
        match Element::id(&element) {
            Some(element_id) => window.with_global_id(element_id, |global_id, window| {
                element.paint(
                    Some(global_id),
                    inspector_id,
                    bounds,
                    request,
                    prepaint,
                    window,
                    cx,
                )
            }),
            None => element.paint(None, inspector_id, bounds, request, prepaint, window, cx),
        };
        self.element = Some(element);
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_overlays_dropdown_menu_tests.rs"]
mod tests;
