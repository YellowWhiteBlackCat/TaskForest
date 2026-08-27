//! Slider: drag + keyboard arrows/Home/End, value clamped to [min, max]
//! (own implementation; state entity is the single source of truth).

use std::rc::Rc;

use crate::OptCallback1;
use gpui::{
    App, Bounds, Context, ElementId, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels,
    Point, RenderOnce, Styled, Window, canvas, div, px, relative,
};
use taskmanager_theme::Palette;

/// Typed slider event payload.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderEvent {
    /// The value changed (drag or keyboard).
    Changed { value: f32 },
    /// A drag gesture ended.
    DragEnded { value: f32 },
}

/// Slider state: canonical value + bounds. All mutations clamp.
pub struct SliderState {
    focus_handle: FocusHandle,
    value: f32,
    min: f32,
    max: f32,
    /// Keyboard step override; `None` falls back to `(max-min)/20`.
    step: Option<f32>,
}

impl SliderState {
    /// Create a slider state.
    pub fn new(min: f32, max: f32, cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_stop(true),
            value: min,
            min,
            max,
            step: None,
        }
    }

    /// Override the keyboard step size (default `(max-min)/20`).
    pub fn set_step(&mut self, step: f32, cx: &mut Context<Self>) {
        if step > 0.0 && self.step != Some(step) {
            self.step = Some(step);
            cx.notify();
        }
    }

    /// The keyboard step size.
    pub fn step_size(&self) -> f32 {
        self.step
            .unwrap_or_else(|| (self.max - self.min).max(1e-6) / 20.0)
    }

    /// The focus handle backing this slider.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Current clamped value.
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Bounds.
    pub fn min(&self) -> f32 {
        self.min
    }

    /// Bounds.
    pub fn max(&self) -> f32 {
        self.max
    }

    /// Set the value (clamped to [min, max]) and notify.
    pub fn set_value(&mut self, value: f32, cx: &mut Context<Self>) {
        let clamped = value.clamp(self.min, self.max);
        if (self.value - clamped).abs() > f32::EPSILON {
            self.value = clamped;
            cx.notify();
        }
    }

    /// Step the value by `delta` (clamped) and notify.
    pub fn step(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.set_value(self.value + delta, cx);
    }
}

/// Builder for one rendered slider.
#[derive(IntoElement)]
pub struct Slider {
    state: Entity<SliderState>,
    palette: Palette,
    on_change: OptCallback1<f32>,
    on_drag_end: OptCallback1<f32>,
}

impl Slider {
    /// Build a slider bound to `state`.
    pub fn new(state: Entity<SliderState>, palette: Palette) -> Self {
        Self {
            state,
            palette,
            on_change: None,
            on_drag_end: None,
        }
    }

    /// Change handler (fires on every value change).
    #[must_use]
    pub fn on_change(mut self, handler: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    /// Drag-end handler.
    #[must_use]
    pub fn on_drag_end(mut self, handler: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_drag_end = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Slider {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (value, min, max, step, focus_handle) = self.state.read_with(cx, |state, _| {
            (
                state.value,
                state.min,
                state.max,
                state.step_size(),
                state.focus_handle.clone(),
            )
        });
        let palette = self.palette;
        let span = (max - min).max(1e-6);
        let frac = ((value - min) / span).clamp(0.0, 1.0);
        let on_change = self.on_change.clone();
        let on_drag_end = self.on_drag_end.clone();
        let state = self.state.clone();

        let id = ElementId::named_usize(
            "tm-slider",
            self.state.entity_id().as_non_zero_u64().get() as usize,
        );

        let thumb = div()
            .size(px(16.0))
            .rounded_full()
            .bg(palette.accent)
            .border(px(2.0))
            .border_color(palette.surface)
            .mx(px(-8.0));

        // Keyboard: arrows step by `step`, Home/End jump to the ends.
        let keyboard = {
            let state = state.clone();
            let on_change = on_change.clone();
            move |window: &mut Window, cx: &mut App, delta: f32| {
                let new_value = state.update(cx, |state, cx| {
                    state.step(delta, cx);
                    state.value()
                });
                if let Some(on_change) = &on_change {
                    on_change(new_value, window, cx);
                }
            }
        };

        // Pointer -> value: the track's laid-out bounds are captured each
        // frame by this canvas backfill (same pattern as gpui_app elements).
        let bounds = Rc::new(std::cell::RefCell::new(None::<Bounds<Pixels>>));
        let bounds_for_paint = bounds.clone();

        div()
            .id(id)
            .debug_selector(|| "tm-slider".into())
            .track_focus(&focus_handle)
            .flex()
            .items_center()
            .w_full()
            .h(px(28.0))
            .cursor_pointer()
            .focus(|style| style.border_color(palette.ring))
            .on_key_down({
                let state = state.clone();
                let on_change = on_change.clone();
                move |event: &KeyDownEvent, window, cx| {
                    let key = event.keystroke.key.as_str();
                    if event.keystroke.modifiers.modified() {
                        return;
                    }
                    match key {
                        "left" | "down" => {
                            cx.stop_propagation();
                            keyboard(window, cx, -step);
                        }
                        "right" | "up" => {
                            cx.stop_propagation();
                            keyboard(window, cx, step);
                        }
                        "home" => {
                            cx.stop_propagation();
                            let new_value = state.update(cx, |state, cx| {
                                state.set_value(min, cx);
                                state.value()
                            });
                            if let Some(on_change) = &on_change {
                                on_change(new_value, window, cx);
                            }
                        }
                        "end" => {
                            cx.stop_propagation();
                            let new_value = state.update(cx, |state, cx| {
                                state.set_value(max, cx);
                                state.value()
                            });
                            if let Some(on_change) = &on_change {
                                on_change(new_value, window, cx);
                            }
                        }
                        _ => {}
                    }
                }
            })
            .child(
                div()
                    .w_full()
                    .h(px(4.0))
                    .rounded(palette.xsmall_radius)
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        div()
                            .flex_basis(relative(frac))
                            .flex_shrink_0()
                            .h_full()
                            .rounded(palette.xsmall_radius)
                            .bg(palette.accent),
                    )
                    .child(thumb)
                    .child(
                        div()
                            .flex_basis(relative(1.0 - frac))
                            .flex_shrink_0()
                            .h_full()
                            .rounded(palette.xsmall_radius)
                            .bg(palette.border),
                    ),
            )
            .child(
                canvas(|_, _, _| (), {
                    let bounds = bounds_for_paint.clone();
                    move |bnd, _, _, _| {
                        *bounds.borrow_mut() = Some(bnd);
                    }
                })
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_mouse_down(MouseButton::Left, {
                let state = state.clone();
                let on_change = on_change.clone();
                let bounds = bounds.clone();
                move |event: &MouseDownEvent, window, cx| {
                    let new_value = state.update(cx, |state, cx| {
                        let value = bounds
                            .borrow()
                            .map(|bounds| pointer_value(event.position, &bounds, min, span))
                            .unwrap_or(min);
                        state.set_value(value, cx);
                        state.value()
                    });
                    if let Some(on_change) = &on_change {
                        on_change(new_value, window, cx);
                    }
                }
            })
            .on_mouse_move({
                let state = state.clone();
                let on_change = on_change.clone();
                let bounds = bounds.clone();
                move |event: &MouseMoveEvent, window, cx| {
                    if !event.dragging() {
                        return;
                    }
                    let new_value = state.update(cx, |state, cx| {
                        let value = bounds
                            .borrow()
                            .map(|bounds| pointer_value(event.position, &bounds, min, span))
                            .unwrap_or(min);
                        state.set_value(value, cx);
                        state.value()
                    });
                    if let Some(on_change) = &on_change {
                        on_change(new_value, window, cx);
                    }
                }
            })
            .on_mouse_up_out(MouseButton::Left, {
                let state = state.clone();
                let on_drag_end = on_drag_end.clone();
                move |_event: &MouseUpEvent, window, cx| {
                    let value = state.read_with(cx, |state, _| state.value());
                    if let Some(on_drag_end) = &on_drag_end {
                        on_drag_end(value, window, cx);
                    }
                }
            })
    }
}

/// Pointer position -> value, clamped to the track bounds.
fn pointer_value(pos: Point<Pixels>, bounds: &Bounds<Pixels>, min: f32, span: f32) -> f32 {
    let w = f32::from(bounds.size.width).max(1.0);
    let rel = ((f32::from(pos.x) - f32::from(bounds.origin.x)) / w).clamp(0.0, 1.0);
    min + span * rel
}

#[cfg(test)]
#[path = "../../tests/gui/ui_inputs_slider_tests.rs"]
mod tests;
