//! Deterministic context-menu capture coordination.
//!
//! A real context menu cannot be opened by mutating view state: its only
//! production entry is the row's right-click listener. This module injects the
//! same pointer events a user would send, then certifies the menu only after
//! the host has mounted its `PopupMenuState` and the popup survived two
//! rendered frames.

use gpui::{
    Bounds, Context, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, PlatformInput, Point,
    Window, point, px, size,
};
use taskmanager_shell::ProcessRowId;

use super::{RootView, TopPage};

pub(super) fn schedule_process_context_menu_capture(
    view: &mut RootView,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    if !view.capture_evidence.process_context_menu_active() {
        return;
    }
    if view.capture_evidence.process_context_menu_requested() {
        view.page = TopPage::Apps;
        view.capture_evidence.begin_process_context_menu_capture();
        cx.notify();
        return;
    }
    if !view
        .capture_evidence
        .process_context_menu_pointer_requested()
    {
        return;
    }
    let target = view.processes_projection().0.iter().position(|row| {
        row.selection_key
            .and_then(ProcessRowId::process_identity)
            .is_some()
    });
    let Some(target) = target else {
        return;
    };
    // The virtualized list publishes `last_item_size.item` as the padded
    // viewport, not the uniform row extent. The per-row height is the content
    // height divided by the item count; `bounds_for_item` and `top_item` are
    // only populated by `Div` scroll handles, so `uniform_list` rows are
    // derived from the real scroll offset instead.
    let item_count = view.processes_projection().0.len();
    let row_geometry = {
        let state = view.processes_scroll.vertical.0.borrow();
        let list_bounds = state.base_handle.bounds();
        state.last_item_size.map(|item_size| {
            let row_height = item_size.contents.height / item_count.max(1) as f32;
            let scroll_offset = state.base_handle.offset();
            let row_top = list_bounds.origin.y + row_height * target as f32 + scroll_offset.y;
            (
                list_bounds,
                Bounds {
                    origin: point(list_bounds.origin.x, row_top),
                    size: size(list_bounds.size.width, row_height),
                },
            )
        })
    };
    let Some((list_bounds, target_row_bounds)) = row_geometry else {
        return;
    };
    // A compact window shows only a few rows, so the first identity row can
    // sit below the painted band. Scroll it into view first; anchoring on an
    // unpainted row would hit empty space and never open the menu. The check
    // uses the real offset, so it settles once the row is painted.
    if !target_row_bounds.intersects(&list_bounds) {
        view.processes_scroll
            .vertical
            .scroll_to_item(target, gpui::ScrollStrategy::Nearest);
        cx.notify();
        return;
    }
    let anchor = view
        .capture_evidence
        .process_context_menu_pointer_anchor()
        .or_else(|| {
            // The list is horizontally virtualized to its intrinsic content
            // width, so a wide row can extend past a compact window. Anchor on
            // the part of the row the window actually shows; a row center
            // outside the viewport would be rejected by the guard below.
            let visible =
                target_row_bounds.intersect(&Bounds::new(Point::default(), window.viewport_size()));
            (!visible.is_empty()).then(|| visible.center())
        });
    let Some(anchor) = anchor else {
        return;
    };
    if anchor.x <= px(0.0)
        || anchor.y <= px(0.0)
        || anchor.x >= window.viewport_size().width
        || anchor.y >= window.viewport_size().height
    {
        return;
    }
    if !view.capture_evidence.arm_context_menu_pointer(anchor) {
        return;
    }
    // The pointer must be dispatched after `Render::render` releases the
    // `RootView` lease: the row hover listener re-enters `RootView`, and a
    // synchronous dispatch here would panic with a double lease.
    // `window.on_next_frame` runs outside the view lease; `Context::on_next_frame`
    // or `defer_in` would re-lease `RootView` before dispatching.
    window.on_next_frame(move |window, cx| {
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: anchor,
                pressed_button: None,
                modifiers: Modifiers::none(),
            }),
            cx,
        );
    });
    let view_handle = cx.entity().downgrade();
    window.on_next_frame(move |window, cx| {
        let awaiting_menu = view_handle
            .read_with(cx, |view, _| {
                view.capture_evidence.process_context_menu_awaiting_menu()
            })
            .unwrap_or(false);
        if !awaiting_menu {
            return;
        }
        let anchor = view_handle
            .read_with(cx, |view, _| {
                view.capture_evidence.process_context_menu_pointer_anchor()
            })
            .ok()
            .flatten();
        let Some(anchor) = anchor else {
            return;
        };
        // Dispatch outside the `RootView` lease: the row's production
        // right-click listener re-enters `RootView` to select the row and
        // report the open transition.
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Right,
                position: anchor,
                modifiers: Modifiers::none(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
    });
    cx.notify();
}

pub(super) fn schedule_process_context_menu_presentation(
    view: &mut RootView,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    if !view
        .capture_evidence
        .process_context_menu_awaiting_presentation()
        || !view.capture_evidence.note_context_menu_presentation_armed()
    {
        return;
    }
    let root = cx.entity().downgrade();
    window.on_next_frame(move |window, cx| {
        let _ = root.update(cx, |_view, cx| cx.notify());
        window.on_next_frame(move |_window, cx| {
            let _ = root.update(cx, |view, _cx| {
                let menu_open = view.capture_evidence.process_context_menu_is_open();
                view.capture_evidence.mark_context_menu_presented(menu_open);
            });
        });
    });
}
