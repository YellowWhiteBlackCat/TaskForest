//! Popup menu component: anchored positioning, outside-click dismiss,
//! keyboard navigation (absorption §3.5). Single-level menus for P3/P4
//! (submenus are compile-time excluded: `MenuItem` has no submenu variant,
//! matching the "no scrollable+submenu combination" rule by construction).
//!
//! The menu is a self-rendering view: hosts call [`PopupMenuState::open`]
//! and mount the returned entity as a child element. Rendering, anchoring,
//! focus, and dismissal are owned here, so item hover/selection notifies
//! invalidate this view only and unrelated host re-renders reuse the
//! cached element tree instead of rebuilding the menu.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, ClickEvent, Context, DismissEvent, ElementId, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, MouseDownEvent,
    ParentElement, Pixels, Point, Render, ScrollHandle, SharedString, StatefulInteractiveElement,
    Styled, Window, actions, anchored, deferred, div, px,
};
use taskmanager_icons::icon;
use taskmanager_theme::{Palette, Theme};
use taskmanager_ui_contract::IconId;

use crate::primitives::scrollbar::{SCROLLBAR_WIDTH, Scrollbar, ScrollbarShow};

/// The popup key context (navigation bindings live under it).
pub const POPUP_CONTEXT: &str = "TaskManagerPopup";

actions!(popup, [PopupUp, PopupDown, PopupConfirm, PopupCancel]);

/// Register the popup navigation keymap.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", PopupUp, Some(POPUP_CONTEXT)),
        KeyBinding::new("down", PopupDown, Some(POPUP_CONTEXT)),
        KeyBinding::new("enter", PopupConfirm, Some(POPUP_CONTEXT)),
        KeyBinding::new("escape", PopupCancel, Some(POPUP_CONTEXT)),
    ]);
}

/// A typed menu entry (no submenu variant: illegal states are unrepresentable).
#[derive(Clone)]
pub enum MenuEntry {
    /// A 1px separator line.
    Separator,
    /// A non-interactive label row.
    Label(SharedString),
    /// An interactive item.
    Item(MenuItem),
}

/// One interactive menu item.
#[derive(Clone)]
pub struct MenuItem {
    /// Optional leading icon.
    pub icon: Option<IconId>,
    /// The item label.
    pub label: SharedString,
    /// Disabled items are not selectable/clickable.
    pub disabled: bool,
    /// Checked items draw a check mark on the right.
    pub checked: bool,
    /// Activated handler (runs on click/Enter).
    pub action: OptCallback,
}

impl MenuItem {
    /// Build a simple enabled item.
    pub fn new(
        label: impl Into<SharedString>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            icon: None,
            label: label.into(),
            disabled: false,
            checked: false,
            action: Some(Rc::new(action)),
        }
    }

    /// Set the leading icon.
    #[must_use]
    pub fn icon(mut self, icon: IconId) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Mark the item disabled.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Mark the item checked.
    #[must_use]
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

/// Popup menu component: items + selection cursor + presentation (palette,
/// anchor position, min width). `action_context` records the trigger's focus
/// handle so dismissal restores focus (absorption 3.3-B).
pub struct PopupMenuState {
    focus_handle: FocusHandle,
    pub scroll_handle: ScrollHandle,
    items: Vec<MenuEntry>,
    selected: Option<usize>,
    action_context: Option<FocusHandle>,
    palette: Palette,
    position: Point<Pixels>,
    min_width: f32,
}

impl PopupMenuState {
    /// Build a menu entity. Presentation fields hold placeholders until
    /// [`Self::open`] / [`Self::present`] install the live palette and
    /// anchor; a host must never mount a bare `new` entity.
    pub fn new(items: Vec<MenuEntry>, cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_stop(true),
            scroll_handle: ScrollHandle::default(),
            items,
            selected: None,
            action_context: None,
            palette: Theme::default().palette(),
            position: Point::new(Pixels::ZERO, Pixels::ZERO),
            min_width: 120.0,
        }
    }

    /// Open the component: build the state via `build`, install the live
    /// presentation and focus anchor, focus the menu, and return the entity
    /// for the host to mount as a child element. Hosts subscribe to
    /// [`DismissEvent`] themselves to clear their open-menu field.
    pub fn open(
        build: impl FnOnce(PopupMenuState, &mut App) -> PopupMenuState,
        anchor: Option<FocusHandle>,
        palette: Palette,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let mut state = build(PopupMenuState::new(Vec::new(), cx), cx);
        state.action_context = anchor;
        state.mount(palette, position, window, cx)
    }

    /// Mount an already-built state (for hosts that build the menu with a
    /// typed context, e.g. a table delegate): install the live presentation,
    /// create the entity, focus it, and return it for mounting as a child
    /// element.
    pub fn mount(
        mut self,
        palette: Palette,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        self.palette = palette;
        self.position = position;
        let menu = cx.new(|_| self);
        menu.read_with(cx, |menu, _| menu.focus_handle.clone())
            .focus(window);
        menu
    }

    /// Re-install the presentation on an existing entity (e.g. a dropdown
    /// re-anchoring its cached menu after the trigger moved).
    pub fn present(&mut self, palette: Palette, position: Point<Pixels>) {
        self.palette = palette;
        self.position = position;
    }

    /// Whether the menu has no interactive items.
    pub fn is_empty(&self) -> bool {
        !self.items.iter().any(|entry| match entry {
            MenuEntry::Item(item) => !item.disabled,
            _ => false,
        })
    }

    /// The currently selected item index.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The menu items (read-only).
    pub fn items(&self) -> &[MenuEntry] {
        &self.items
    }

    /// The trigger's focus handle, restored on dismiss.
    pub fn set_action_context(&mut self, handle: FocusHandle) {
        self.action_context = Some(handle);
    }

    /// The trigger's focus handle.
    pub fn action_context(&self) -> Option<&FocusHandle> {
        self.action_context.as_ref()
    }

    /// Whether the given item index is selectable.
    pub fn is_selectable(&self, ix: usize) -> bool {
        matches!(&self.items.get(ix), Some(MenuEntry::Item(item)) if !item.disabled)
    }

    /// Move the selection cursor one step toward `direction` (skip disabled
    /// items and separators, wrap around; absorption 3.3-C).
    pub fn move_selection(&mut self, direction: SelectionDirection, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            return;
        }
        let count = self.items.len();
        let Some(mut ix) = self.selected else {
            // First activation: land on the nearest selectable in the given
            // direction without an extra step.
            let base = match direction {
                SelectionDirection::Down => 0,
                SelectionDirection::Up => count - 1,
            };
            self.select_nearest(base, direction, cx);
            return;
        };
        for _ in 0..count {
            ix = match direction {
                SelectionDirection::Down => (ix + 1) % count,
                SelectionDirection::Up => (ix + count - 1) % count,
            };
            if self.is_selectable(ix) {
                self.selected = Some(ix);
                cx.notify();
                return;
            }
        }
        self.selected = None;
        cx.notify();
    }

    /// Select the nearest selectable item scanning from `base` in `direction`.
    fn select_nearest(
        &mut self,
        base: usize,
        direction: SelectionDirection,
        cx: &mut Context<Self>,
    ) {
        let count = self.items.len();
        for step in 0..count {
            let ix = match direction {
                SelectionDirection::Down => (base + step) % count,
                SelectionDirection::Up => (base + count - step) % count,
            };
            if self.is_selectable(ix) {
                self.selected = Some(ix);
                cx.notify();
                return;
            }
        }
        self.selected = None;
        cx.notify();
    }

    /// Activate the selected item: run its action and dismiss.
    pub fn confirm_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.selected else {
            return;
        };
        let action = match &self.items.get(ix) {
            Some(MenuEntry::Item(item)) if !item.disabled => item.action.clone(),
            _ => None,
        };
        if let Some(action) = action {
            action(window, cx);
        }
    }

    /// Dismiss the menu and restore the trigger focus (absorption 3.3-B).
    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = &self.action_context {
            handle.focus(window);
        }
        cx.emit(DismissEvent);
    }
}

/// Selection traversal direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionDirection {
    /// Toward the end of the list.
    Down,
    /// Toward the start of the list.
    Up,
}

impl EventEmitter<DismissEvent> for PopupMenuState {}
impl Focusable for PopupMenuState {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// The rendered popup: anchored at the open position, snapping to the
/// window edge, keyboard-navigable, dismisses on outside clicks / Escape.
impl Render for PopupMenuState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = cx.entity();
        let id = ElementId::named_usize(
            "tm-popup",
            state.entity_id().as_non_zero_u64().get() as usize,
        );
        let items_id = ElementId::named_usize(
            "tm-popup-items",
            state.entity_id().as_non_zero_u64().get() as usize,
        );
        let focus_handle = self.focus_handle.clone();
        let scroll_handle = self.scroll_handle.clone();
        let palette = self.palette;
        let selected = self.selected;

        let menu_body = div()
            .id(items_id)
            .debug_selector(|| "tm-popup-items".into())
            .key_context(POPUP_CONTEXT)
            .track_focus(&focus_handle)
            .min_w(px(self.min_width))
            .max_h(px(360.0))
            .overflow_y_scroll()
            .track_scroll(&scroll_handle)
            .py(tokens::SPACE_4)
            .flex_col()
            // Outside-click dismiss.
            .on_mouse_down_out({
                let state = state.clone();
                move |_event: &MouseDownEvent, window, cx| {
                    state.update(cx, |state, cx| state.dismiss(window, cx));
                }
            })
            // Keyboard navigation.
            .on_action({
                let state = state.clone();
                move |_: &PopupUp, _window, cx| {
                    state.update(cx, |state, cx| {
                        state.move_selection(SelectionDirection::Up, cx)
                    });
                }
            })
            .on_action({
                let state = state.clone();
                move |_: &PopupDown, _window, cx| {
                    state.update(cx, |state, cx| {
                        state.move_selection(SelectionDirection::Down, cx)
                    });
                }
            })
            .on_action({
                let state = state.clone();
                move |_: &PopupConfirm, window, cx| {
                    state.update(cx, |state, cx| {
                        state.confirm_selected(window, cx);
                        // Desktop convention: choosing with Enter closes the
                        // menu (the mouse path already confirms + dismisses).
                        state.dismiss(window, cx);
                    });
                }
            })
            .on_action({
                let state = state.clone();
                move |_: &PopupCancel, window, cx| {
                    state.update(cx, |state, cx| state.dismiss(window, cx));
                }
            });

        let mut body = menu_body;
        for (ix, entry) in self.items.iter().enumerate() {
            body = match entry {
                MenuEntry::Separator => {
                    body.child(div().h_px().my(tokens::SPACE_4).bg(palette.border))
                }
                MenuEntry::Label(label) => body.child(
                    div()
                        .px(tokens::SPACE_12)
                        .py(tokens::SPACE_6)
                        .text_sm()
                        .text_color(palette.fg_muted)
                        .child(label.clone()),
                ),
                MenuEntry::Item(item) => {
                    let is_selected = selected == Some(ix);
                    let is_disabled = item.disabled;
                    let state = state.clone();
                    body.child(
                        div()
                            .id(ElementId::NamedInteger("tm-popup-item".into(), ix as u64))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(tokens::SPACE_8)
                            .px(tokens::SPACE_12)
                            .py(tokens::SPACE_6)
                            .h(px(26.0))
                            .text_sm()
                            .text_color(if is_disabled {
                                palette.fg_muted
                            } else {
                                palette.fg
                            })
                            .when(is_selected, |el| el.bg(hover_fill(palette.surface)))
                            .on_hover({
                                let state = state.clone();
                                move |hovering: &bool, _window, cx| {
                                    if *hovering {
                                        state.update(cx, |state, cx| {
                                            // Idempotent hover: a no-op move
                                            // across the same item must not
                                            // notify (redraw storm guard).
                                            if state.selected != Some(ix) {
                                                state.selected = Some(ix);
                                                cx.notify();
                                            }
                                        });
                                    }
                                }
                            })
                            .on_click({
                                let state = state.clone();
                                move |_event: &ClickEvent, window, cx| {
                                    if is_disabled {
                                        return;
                                    }
                                    state.update(cx, |state, cx| {
                                        state.selected = Some(ix);
                                        state.confirm_selected(window, cx);
                                        state.dismiss(window, cx);
                                    });
                                }
                            })
                            .child(div().w(px(16.0)).flex_shrink_0().child(match item.icon {
                                Some(icon_id) => div().child(icon(icon_id).size(px(14.0))),
                                None => div(),
                            }))
                            .child(div().flex_grow().child(item.label.clone()))
                            .child(div().w(px(14.0)).flex_shrink_0().child(if item.checked {
                                div().child(
                                    icon(IconId::EndTask)
                                        .size(px(12.0))
                                        .text_color(palette.accent),
                                )
                            } else {
                                div()
                            })),
                    )
                }
            };
        }

        let container = div()
            .id(id)
            .debug_selector(|| "tm-popup".into())
            .relative()
            .min_w(px(self.min_width))
            .rounded(palette.panel_radius)
            .bg(palette.surface)
            .border_1()
            .border_color(palette.border)
            .shadow_md()
            .child(body)
            .child(
                div()
                    .occlude()
                    .absolute()
                    .top(tokens::SPACE_4)
                    .right_0()
                    .bottom(tokens::SPACE_4)
                    .w(px(SCROLLBAR_WIDTH))
                    .child(
                        Scrollbar::vertical("tm-popup-scrollbar", Rc::new(scroll_handle), palette)
                            .show(ScrollbarShow::Scrolling),
                    ),
            );

        deferred(
            anchored()
                .position(self.position)
                .snap_to_window_with_margin(px(8.0))
                .child(container),
        )
        .with_priority(1)
    }
}

use crate::OptCallback;
use crate::styled::hover_fill;
use taskmanager_theme::tokens;

#[cfg(test)]
#[path = "../../tests/gui/ui_overlays_popup_tests.rs"]
mod tests;
