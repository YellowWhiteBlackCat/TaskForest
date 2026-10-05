//! Toggle switch: track + knob, keyboard Tab/Enter/Space, focus ring from
//! `palette.ring` (absorption: own implementation, no gpui-component).

use std::rc::Rc;

use crate::OptCallback1;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, Bounds, ClickEvent, Context, ElementId, Entity, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Pixels, RenderOnce, ScrollHandle,
    StatefulInteractiveElement, Styled, Window, canvas, div, point, px,
};
use taskmanager_theme::Palette;
use taskmanager_theme::tokens;

/// Typed switch event payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchEvent {
    /// The switch toggled to a new state.
    Toggled { on: bool },
}

/// Switch state: the canonical `on` flag lives here so tests assert behavior
/// (not source text) and owners read it without extra plumbing.
pub struct SwitchState {
    focus_handle: FocusHandle,
    on: bool,
    scroll_parent: Option<ScrollHandle>,
    painted_bounds: Option<Bounds<Pixels>>,
}

impl SwitchState {
    /// Create a switch state, initially off.
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            on: false,
            scroll_parent: None,
            painted_bounds: None,
        }
    }

    /// The focus handle backing this switch.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Attach the scroll viewport that owns keyboard visibility of this control.
    pub fn set_scroll_parent(&mut self, parent: ScrollHandle) {
        self.scroll_parent = Some(parent);
    }
    /// Actual prepaint bounds of the mounted control.
    pub fn painted_bounds(&self) -> Option<Bounds<Pixels>> {
        self.painted_bounds
    }

    /// Current switch position.
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Set the switch position directly and notify.
    pub fn set_on(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.on != on {
            self.on = on;
            cx.notify();
        }
    }

    /// Flip the switch and notify.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.on = !self.on;
        cx.notify();
    }
}

/// Builder for one rendered switch. The `on` flag is read from the state each
/// render; `on_change` fires with the new value after the state flips.
#[derive(IntoElement)]
pub struct Switch {
    state: Entity<SwitchState>,
    palette: Palette,
    disabled: bool,
    on_change: OptCallback1<bool>,
}

impl Switch {
    /// Build a switch bound to `state`.
    pub fn new(state: Entity<SwitchState>, palette: Palette) -> Self {
        Self {
            state,
            palette,
            disabled: false,
            on_change: None,
        }
    }

    /// Disable pointer + keyboard interaction.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Change handler: receives the NEW state after toggling.
    #[must_use]
    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (on, focus_handle, disabled) = self.state.read_with(cx, |state, _| {
            (state.on, state.focus_handle.clone(), self.disabled)
        });
        let palette = self.palette;
        let on_change = self.on_change.clone();
        let state = self.state.clone();
        let measured = self.state.clone();
        let measurement = canvas(
            move |bounds, window, cx| {
                measured.update(cx, |state, _cx| {
                    state.painted_bounds = Some(bounds);
                    if state.focus_handle.is_focused(window)
                        && let Some(parent) = &state.scroll_parent
                    {
                        let viewport = parent.bounds();
                        if viewport.size.height > px(0.0) {
                            let delta = if bounds.top() < viewport.top() {
                                viewport.top() - bounds.top()
                            } else if bounds.bottom() > viewport.bottom() {
                                viewport.bottom() - bounds.bottom()
                            } else {
                                px(0.0)
                            };
                            if delta != px(0.0) {
                                parent.set_offset(parent.offset() + point(px(0.0), delta));
                                window.refresh();
                            }
                        }
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();

        // gpui `TabStopMap` reads the TRACKED handle's own `tab_stop` field,
        // not the element-level `.tab_stop()` style (which only updates the
        // focus map). A plain `cx.focus_handle()` is never tab-reachable, so
        // track a clone carrying the real value: `!disabled` both registers
        // the switch in Tab order and removes it when disabled.
        let focus_handle = focus_handle.clone().tab_stop(!disabled);

        let id = ElementId::named_usize(
            "tm-switch",
            self.state.entity_id().as_non_zero_u64().get() as usize,
        );

        // Knob slides between the off and on positions.
        let track = div()
            .w(px(36.0))
            .h(px(20.0))
            .rounded_full()
            .bg(crate::theme_binding::fill(if on {
                palette.accent
            } else {
                palette.border
            }))
            .p(crate::theme_binding::definite_length(tokens::SPACE_2))
            .cursor_pointer();

        let knob = div()
            .size(px(16.0))
            .rounded_full()
            .bg(crate::theme_binding::fill(palette.surface))
            .shadow_sm()
            .when(on, |el| {
                el.ml(crate::theme_binding::length(tokens::SPACE_16))
            });

        div()
            .id(id)
            .relative()
            .child(measurement)
            .debug_selector(|| "tm-switch".into())
            .track_focus(&focus_handle)
            .flex()
            .items_center()
            .when(disabled, |el| el.opacity(0.5))
            // Focus ring: transparent alpha for pointer focus, visible for
            // keyboard focus (palette.ring carries the decision).
            .focus(|style| style.border_color(crate::theme_binding::hsla(palette.ring)))
            .border_1()
            .rounded_full()
            .child(track.child(knob))
            .on_key_down({
                let state = state.clone();
                let on_change = on_change.clone();
                move |event: &KeyDownEvent, window, cx| {
                    if disabled {
                        return;
                    }
                    let stroke = &event.keystroke;
                    let unmodified = !stroke.modifiers.modified();
                    if unmodified && matches!(stroke.key.as_str(), "enter" | "space") {
                        cx.stop_propagation();
                        let new_on = !on;
                        state.update(cx, |state, cx| {
                            state.set_on(new_on, cx);
                        });
                        if let Some(on_change) = &on_change {
                            on_change(new_on, window, cx);
                        }
                    }
                }
            })
            .on_click(move |_event: &ClickEvent, window, cx| {
                if disabled {
                    return;
                }
                let new_on = !on;
                state.update(cx, |state, cx| {
                    state.set_on(new_on, cx);
                });
                if let Some(on_change) = &on_change {
                    on_change(new_on, window, cx);
                }
            })
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_inputs_switch_tests.rs"]
mod tests;
