//! Toast: transient status message with auto-dismiss and typed kinds
//! (absorption: gc notification semantics, our own component).

use std::rc::Rc;
use std::time::Duration;

use crate::OptCallback;
use crate::icons_binding::icon;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnimationExt, App, ClickEvent, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Task, Window, div,
    px,
};
use taskmanager_theme::tokens;
use taskmanager_theme::{Color, Palette};
use taskmanager_ui_contract::IconId;

/// Toast severity; each maps to a palette semantic color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    /// `palette.accent` (default).
    Info,
    /// `palette.success`.
    Success,
    /// `palette.warning`.
    Warning,
    /// `palette.danger`.
    Danger,
}

impl ToastKind {
    /// The palette color for this kind.
    pub fn color(self, palette: &Palette) -> Color {
        match self {
            ToastKind::Info => palette.accent,
            ToastKind::Success => palette.success,
            ToastKind::Warning => palette.warning,
            ToastKind::Danger => palette.danger,
        }
    }

    /// A semantic icon for this kind.
    pub fn icon(self) -> IconId {
        match self {
            ToastKind::Info => IconId::Health,
            ToastKind::Success => IconId::Health,
            ToastKind::Warning => IconId::Alert,
            ToastKind::Danger => IconId::Alert,
        }
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
    kind: ToastKind,
    auto_dismiss: Option<Duration>,
    _task: Option<Task<()>>,
}

impl ToastState {
    /// Build a toast state; when `auto_dismiss` is set, a timer dismisses it.
    pub fn new(
        id: u64,
        message: impl Into<SharedString>,
        kind: ToastKind,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            id,
            message: message.into(),
            kind,
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
    pub fn kind(&self) -> ToastKind {
        self.kind
    }

    /// Arm the auto-dismiss timer. Call once from the host after creation.
    pub fn arm_auto_dismiss(&mut self, duration: Duration, cx: &mut Context<Self>) {
        self.auto_dismiss = Some(duration);
        let this = cx.entity();
        self._task = Some(cx.spawn(async move |_this, cx| {
            gpui::Timer::after(duration).await;
            let _ = this.update(cx, |toast, cx| {
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
pub struct Toast {
    state: Entity<ToastState>,
    palette: Palette,
    on_dismiss: OptCallback,
}

impl Toast {
    /// Build a toast card for `state`.
    pub fn new(state: Entity<ToastState>, palette: Palette) -> Self {
        Self {
            state,
            palette,
            on_dismiss: None,
        }
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
        let (message, kind) = self
            .state
            .read_with(cx, |state, _| (state.message(), state.kind()));
        let palette = self.palette;
        let accent = kind.color(&palette);
        let on_dismiss = self.on_dismiss.clone();

        // Mission-Center-style entrance: the toast fades in and rises 4px as
        // it mounts (a fresh toast always replays the animation).
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(crate::theme_binding::definite_length(tokens::SPACE_10))
            .px(crate::theme_binding::definite_length(tokens::SPACE_12))
            .py(crate::theme_binding::definite_length(tokens::SPACE_10))
            .rounded(crate::theme_binding::absolute(palette.panel_radius))
            .bg(crate::theme_binding::fill(palette.surface))
            .border_1()
            .border_color(crate::theme_binding::hsla(palette.border))
            .shadow_md()
            .text_sm()
            .text_color(crate::theme_binding::hsla(palette.fg))
            .child(
                icon(kind.icon())
                    .size(px(16.0))
                    .text_color(crate::theme_binding::hsla(accent)),
            )
            .child(div().child(message))
            .when_some(on_dismiss, |el, on_dismiss| {
                el.child(
                    div()
                        .id("tm-toast-dismiss")
                        .cursor_pointer()
                        .text_color(crate::theme_binding::hsla(palette.fg_muted))
                        .on_click(move |_event: &ClickEvent, window, cx| {
                            on_dismiss(window, cx);
                        })
                        .child("×"),
                )
            })
            // Mission-Center-style entrance: the toast fades in and rises 4px
            // as it mounts (a fresh toast always replays the animation).
            .with_animation(
                "toast-entrance",
                crate::theme_binding::appear(),
                |el, delta| el.opacity(delta).mt(px((1.0 - delta) * 4.0)),
            )
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_overlays_toast_tests.rs"]
mod tests;
