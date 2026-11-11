//! Toast: transient status message with auto-dismiss and typed kinds
//! (absorption: gc notification semantics, our own component).

use std::rc::Rc;
use std::time::Duration;

use crate::OptCallback;
use crate::icons_binding::icon;
use crate::primitives::card_surface::{CardSurface, GlassSurfaceRole};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnimationExt, App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement,
    Styled, Task, Window, div, px,
};
use taskmanager_theme::{Palette, tokens};
use taskmanager_ui_contract::IconId;
use zhuge_ui_tokens::{StatusColors, StatusKind};

/// A product semantic icon for this kind.
fn status_icon(kind: StatusKind) -> IconId {
    match kind {
        StatusKind::Info | StatusKind::Success => IconId::Health,
        StatusKind::Warning | StatusKind::Danger => IconId::Alert,
    }
}

/// Typed toast event payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastEvent {
    /// The toast's auto-dismiss timer fired.
    Dismissed { id: u64 },
}

/// Toast state: the id, message, kind and auto-dismiss bookkeeping live here.
pub struct ToastState {
    id: u64,
    message: SharedString,
    kind: StatusKind,
    dismiss_focus: FocusHandle,
    auto_dismiss: Option<Duration>,
    _task: Option<Task<()>>,
}

impl ToastState {
    /// Build a toast state; when `auto_dismiss` is set, a timer dismisses it.
    pub fn new(
        id: u64,
        message: impl Into<SharedString>,
        kind: StatusKind,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            id,
            message: message.into(),
            kind,
            dismiss_focus: cx.focus_handle().tab_stop(true),
            auto_dismiss: None,
            _task: None,
        }
    }

    /// The toast id.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The message.
    pub fn message(&self) -> SharedString {
        self.message.clone()
    }

    /// The kind.
    pub fn kind(&self) -> StatusKind {
        self.kind
    }

    /// Arm the auto-dismiss timer. Call once from the host after creation.
    pub fn arm_auto_dismiss(&mut self, duration: Duration, cx: &mut Context<Self>) {
        self.auto_dismiss = Some(duration);
        let this = cx.entity();
        self._task = Some(cx.spawn(async move |_this, cx| {
            cx.background_executor().timer(duration).await;
            this.update(cx, |toast, cx| {
                cx.emit(ToastEvent::Dismissed { id: toast.id });
                cx.notify();
            });
        }));
    }

    /// Whether auto-dismiss is armed.
    pub fn auto_dismiss(&self) -> Option<Duration> {
        self.auto_dismiss
    }
}

impl EventEmitter<ToastEvent> for ToastState {}

/// Builder for one rendered toast card.
#[derive(IntoElement)]
pub struct Toast {
    state: Entity<ToastState>,
    palette: Palette,
    on_dismiss: OptCallback,
    single_line: bool,
    indicator: bool,
}

impl Toast {
    /// Build a toast card for `state`.
    pub fn new(state: Entity<ToastState>, palette: Palette) -> Self {
        Self {
            state,
            palette,
            on_dismiss: None,
            single_line: false,
            indicator: false,
        }
    }

    /// Keep the receipt on one bounded line for compact chrome slots.
    #[must_use]
    pub fn single_line(mut self) -> Self {
        self.single_line = true;
        self
    }

    /// Render only the semantic icon when the host slot cannot carry text.
    #[must_use]
    pub fn indicator(mut self, indicator: bool) -> Self {
        self.indicator = indicator;
        self
    }

    /// Dismiss handler (fired from the close button / auto-dismiss).
    #[must_use]
    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Toast {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (message, kind, dismiss_focus) = self.state.read_with(cx, |state, _| {
            (state.message(), state.kind(), state.dismiss_focus.clone())
        });
        let palette = self.palette;
        let accent = StatusColors {
            accent: palette.accent,
            success: palette.success,
            warning: palette.warning,
            danger: palette.danger,
        }
        .resolve(kind);
        let on_dismiss = self.on_dismiss.clone();
        let single_line = self.single_line;
        let indicator = self.indicator;

        // Mission-Center-style entrance: the toast fades in and rises 4/14 REM as
        // it mounts (a fresh toast always replays the animation).
        CardSurface::unpadded_for_role(palette, GlassSurfaceRole::Toast)
            .radius(palette.panel_radius)
            .render()
            .debug_selector(|| "tm-toast-card".to_string())
            .flex()
            .flex_row()
            .items_center()
            .min_w(px(0.0))
            .gap(crate::theme_binding::rem_length(tokens::SPACE_10))
            .px(crate::theme_binding::rem_length(tokens::SPACE_12))
            .py(crate::theme_binding::rem_length(tokens::SPACE_10))
            .when(single_line, |card| {
                card.py(crate::theme_binding::rem_length(tokens::SPACE_4))
            })
            .text_sm()
            .text_color(crate::theme_binding::hsla(palette.fg))
            .child(
                icon(status_icon(kind))
                    .size(crate::theme_binding::rem_value(16.0))
                    .text_color(crate::theme_binding::hsla(accent)),
            )
            .when(!indicator, |card| {
                let text = div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .when(single_line, |text| text.truncate())
                    .child(message);
                card.child(text).when_some(on_dismiss, |el, on_dismiss| {
                    el.child(
                        div()
                            .id("tm-toast-dismiss")
                            .debug_selector(|| "tm-toast-dismiss".into())
                            .flex_shrink_0()
                            .track_focus(&dismiss_focus)
                            .focus(|style| {
                                style.border_color(crate::theme_binding::hsla(palette.ring))
                            })
                            .cursor_pointer()
                            .text_color(crate::theme_binding::hsla(palette.fg_muted))
                            .on_key_down({
                                let on_dismiss = on_dismiss.clone();
                                move |event: &KeyDownEvent, window, cx| {
                                    let stroke = &event.keystroke;
                                    if !stroke.modifiers.modified()
                                        && matches!(stroke.key.as_str(), "enter" | "space")
                                    {
                                        cx.stop_propagation();
                                        on_dismiss(window, cx);
                                    }
                                }
                            })
                            .on_click(move |_event: &ClickEvent, window, cx| {
                                on_dismiss(window, cx);
                            })
                            .child("×"),
                    )
                })
            })
            // Mission-Center-style entrance: the toast fades in and rises 4px
            // as it mounts (a fresh toast always replays the animation).
            .with_animation(
                "toast-entrance",
                crate::theme_binding::appear(),
                |el, delta| {
                    el.opacity(delta)
                        .mt(crate::theme_binding::rem_value((1.0 - delta) * 4.0))
                },
            )
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_overlays_toast_tests.rs"]
mod tests;
