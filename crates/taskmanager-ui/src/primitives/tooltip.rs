//! Tooltip content block + hover/keyboard host (absorption §7.5).
//!
//! `Tooltip` is the styled content block; `TooltipHost` wraps a trigger
//! element and shows the tooltip after a 500ms hover delay, immediately on
//! keyboard focus, and hides on mouse leave. Positioning uses gpui's
//! `anchored` so the popup flips/clamps to the window edge automatically.

use std::rc::Rc;
use std::time::Duration;

use crate::ElementBuilder;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Context, ElementId, FocusHandle, FocusOutEvent, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Pixels, Point, RenderOnce, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, Window, anchored, deferred, div, point,
    px,
};
use taskmanager_theme::Palette;
use taskmanager_theme::tokens;

/// Delay before a hover shows the tooltip.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(500);

/// Tooltip content: static text or a custom element builder.
pub enum TooltipContent {
    /// Plain text label.
    Text(SharedString),
    /// Custom element produced per render.
    Element(ElementBuilder),
}

/// The styled tooltip content block. Hosts position it; this struct only owns
/// presentation.
#[derive(IntoElement)]
pub struct Tooltip {
    palette: Palette,
    content: TooltipContent,
    key_hint: Option<SharedString>,
}

impl Tooltip {
    /// Build a text tooltip.
    pub fn text(text: impl Into<SharedString>, palette: Palette) -> Self {
        Self {
            palette,
            content: TooltipContent::Text(text.into()),
            key_hint: None,
        }
    }

    /// Build a tooltip from a custom element.
    pub fn element<E: IntoElement + 'static>(
        builder: impl Fn(&mut Window, &mut App) -> E + 'static,
        palette: Palette,
    ) -> Self {
        Self {
            palette,
            content: TooltipContent::Element(Rc::new(move |window, cx| {
                builder(window, cx).into_any_element()
            })),
            key_hint: None,
        }
    }

    /// Append a shortcut hint (e.g. "Ctrl+F").
    #[must_use]
    pub fn key_hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.key_hint = Some(hint.into());
        self
    }
}

impl RenderOnce for Tooltip {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let palette = self.palette;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(crate::theme_binding::definite_length(tokens::SPACE_8))
            .px(crate::theme_binding::definite_length(tokens::SPACE_10))
            .py(crate::theme_binding::definite_length(tokens::SPACE_5))
            .rounded(crate::theme_binding::absolute(palette.control_radius))
            .bg(crate::theme_binding::fill(palette.surface))
            .border_1()
            .border_color(crate::theme_binding::hsla(palette.border))
            .text_sm()
            .text_color(crate::theme_binding::hsla(palette.fg))
            .child(match &self.content {
                TooltipContent::Text(text) => div().child(text.clone()),
                TooltipContent::Element(builder) => div().child(builder(_window, _cx)),
            })
            .when_some(self.key_hint, |el, hint| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(crate::theme_binding::hsla(palette.fg_muted))
                        .child(hint),
                )
            })
    }
}

/// Delay/hover state machine for one tooltip host. Pure logic, unit-tested
/// headlessly; the host drives it from pointer/focus events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TooltipVisibility {
    /// Not shown.
    #[default]
    Hidden,
    /// Hover registered; waiting out the 500ms delay.
    Armed,
    /// Shown (hover delay elapsed or keyboard focus).
    Visible,
}

/// Pure transition rules for [`TooltipVisibility`].
#[must_use]
pub fn tooltip_step(
    current: TooltipVisibility,
    hovering: bool,
    focus_within: bool,
    hover_elapsed: Option<Duration>,
) -> TooltipVisibility {
    match current {
        TooltipVisibility::Hidden => {
            if focus_within {
                TooltipVisibility::Visible
            } else if hovering {
                TooltipVisibility::Armed
            } else {
                TooltipVisibility::Hidden
            }
        }
        TooltipVisibility::Armed => {
            if focus_within {
                TooltipVisibility::Visible
            } else if !hovering {
                TooltipVisibility::Hidden
            } else if hover_elapsed.is_some_and(|elapsed| elapsed >= TOOLTIP_DELAY) {
                TooltipVisibility::Visible
            } else {
                TooltipVisibility::Armed
            }
        }
        TooltipVisibility::Visible => {
            if focus_within || hovering {
                TooltipVisibility::Visible
            } else {
                TooltipVisibility::Hidden
            }
        }
    }
}

/// Host state for one tooltip (element state, created per rendered host).
pub struct TooltipHostState {
    visibility: TooltipVisibility,
    hover_since: Option<std::time::Instant>,
    focus_within: bool,
    dismissed: bool,
    focus_handle: FocusHandle,
    _focus_in_subscription: Subscription,
    _focus_out_subscription: Subscription,
    _task: Option<Task<()>>,
}

impl TooltipHostState {
    fn new(
        focus_handle: FocusHandle,
        focus_in_subscription: Subscription,
        focus_out_subscription: Subscription,
    ) -> Self {
        Self {
            visibility: TooltipVisibility::Hidden,
            hover_since: None,
            focus_within: false,
            dismissed: false,
            focus_handle,
            _focus_in_subscription: focus_in_subscription,
            _focus_out_subscription: focus_out_subscription,
            _task: None,
        }
    }

    /// Advance the state machine from a pointer-hover change.
    pub fn on_hover_changed(
        &mut self,
        hovering: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = std::time::Instant::now();
        if hovering {
            if self.hover_since.is_none() {
                self.dismissed = false;
            }
            self.hover_since = Some(now);
        } else {
            self.hover_since = None;
            if !self.focus_within {
                self.dismissed = false;
            }
        }
        self.recompute(window, cx);
    }

    /// Advance the state machine from a focus change.
    pub fn on_focus_changed(
        &mut self,
        focus_within: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.focus_within == focus_within {
            return;
        }
        if focus_within {
            self.dismissed = false;
        }
        self.focus_within = focus_within;
        if !self.focus_within && self.hover_since.is_none() {
            self.dismissed = false;
        }
        self.recompute(window, cx);
    }

    /// Hide the tooltip until its trigger leaves both hover and focus, or a
    /// new pointer/focus entry begins.
    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismissed = true;
        self.visibility = TooltipVisibility::Hidden;
        self._task = None;
        cx.notify();
        window.refresh();
    }

    /// Spawn a one-shot wakeup that re-evaluates the hover delay.
    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visibility = if self.dismissed {
            TooltipVisibility::Hidden
        } else {
            tooltip_step(
                self.visibility,
                self.hover_since.is_some(),
                self.focus_within,
                self.hover_elapsed(),
            )
        };
        if self.visibility == TooltipVisibility::Armed {
            let delay = TOOLTIP_DELAY.saturating_sub(self.hover_elapsed().unwrap_or_default());
            let this = cx.entity();
            self._task = Some(window.spawn(cx, async move |cx| {
                gpui::Timer::after(delay).await;
                this.update(cx, |state, cx| {
                    if !state.dismissed {
                        state.visibility = tooltip_step(
                            state.visibility,
                            state.hover_since.is_some(),
                            state.focus_within,
                            state.hover_elapsed(),
                        );
                        if state.visibility == TooltipVisibility::Visible {
                            cx.notify();
                        }
                    }
                })
                .ok();
            }));
        }
        window.refresh();
    }

    fn hover_elapsed(&self) -> Option<Duration> {
        self.hover_since.map(|since| since.elapsed())
    }
}

/// A wrapper element that shows a tooltip near its trigger on hover (after
/// [`TOOLTIP_DELAY`]) or keyboard focus, positioned with `anchored` (window
/// edge flipping/clamping included). The trigger element is provided by the
/// consumer; the host adds the hover wiring and the popup layer.
#[derive(IntoElement)]
pub struct TooltipHost {
    id: ElementId,
    trigger: Option<AnyElement>,
    tooltip: Option<Tooltip>,
    offset: Point<Pixels>,
}

impl TooltipHost {
    /// Wrap `trigger`; the tooltip appears when hovering it.
    pub fn new(id: impl Into<ElementId>, trigger: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            trigger: Some(trigger.into_any_element()),
            tooltip: None,
            offset: point(px(0.0), px(8.0)),
        }
    }

    /// The tooltip content to show.
    #[must_use]
    pub fn tooltip(mut self, tooltip: Tooltip) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    /// Anchor offset from the trigger's layout position.
    #[must_use]
    pub fn offset(mut self, offset: Point<Pixels>) -> Self {
        self.offset = offset;
        self
    }
}

impl RenderOnce for TooltipHost {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state_key = ElementId::from((self.id.clone(), "state"));
        let state = window.use_keyed_state(state_key, cx, |window, cx| {
            let focus_handle = cx.focus_handle();
            let focus_in_state: gpui::WeakEntity<TooltipHostState> = cx.weak_entity();
            let focus_in_subscription = window.on_focus_in(&focus_handle, cx, move |window, cx| {
                let _ = focus_in_state.update(cx, |state, cx| {
                    state.on_focus_changed(true, window, cx);
                });
            });
            let focus_out_state: gpui::WeakEntity<TooltipHostState> = cx.weak_entity();
            let focus_out_subscription = window.on_focus_out(
                &focus_handle,
                cx,
                move |_event: FocusOutEvent, window, cx| {
                    let _ = focus_out_state.update(cx, |state, cx| {
                        state.on_focus_changed(false, window, cx);
                    });
                },
            );
            TooltipHostState::new(focus_handle, focus_in_subscription, focus_out_subscription)
        });
        let tooltip = self.tooltip.take();
        let offset = self.offset;
        let focus_handle = state.read_with(cx, |state, _| state.focus_handle.clone());
        let focus_within = focus_handle.contains_focused(window, cx);
        state.update(cx, |state, cx| {
            state.on_focus_changed(focus_within, window, cx);
        });

        let trigger = self.trigger.expect("trigger must be set");
        let mut base = div()
            .id(self.id)
            .relative()
            .track_focus(&focus_handle)
            .cursor_default()
            .on_hover({
                let state = state.clone();
                move |hovering: &bool, window, cx| {
                    state.update(cx, |state, cx| {
                        state.on_hover_changed(*hovering, window, cx);
                    });
                }
            })
            .on_click({
                let state = state.clone();
                move |_event, window, cx| {
                    state.update(cx, |state, cx| state.dismiss(window, cx));
                }
            })
            .on_key_down({
                let state = state.clone();
                move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
                        cx.stop_propagation();
                        state.update(cx, |state, cx| state.dismiss(window, cx));
                    }
                }
            })
            .child(trigger);

        let show = state.read_with(cx, |state, _| {
            state.visibility == TooltipVisibility::Visible
        });

        if show && let Some(tooltip) = tooltip {
            base = base.child(deferred(anchored().offset(offset).child(tooltip)).with_priority(1));
        }

        base
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_tooltip_tests.rs"]
mod tests;
