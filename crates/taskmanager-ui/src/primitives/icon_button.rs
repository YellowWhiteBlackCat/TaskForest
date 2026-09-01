//! Icon-only button (M2: state entity + builder + element render).

use std::rc::Rc;

use crate::OptEventCallback;
use crate::icons_binding::icon;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, Context, ElementId, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, RenderOnce, StatefulInteractiveElement, Styled, Window, div, px,
};
use taskmanager_theme::Palette;
use taskmanager_ui_contract::IconId;

use crate::styled::{active_fill, hover_fill};

/// An icon-only round button. Shares the [`Button`](crate::primitives::button)
/// activation model: pointer click and keyboard activation both arrive as
/// `ButtonEvent::Activated`.
pub struct IconButtonState {
    focus_handle: FocusHandle,
    enabled: bool,
}

impl IconButtonState {
    /// Create a new icon button state.
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            enabled: true,
        }
    }

    /// The focus handle backing this button.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Whether the button accepts activation.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Flip the enabled flag and notify observers.
    pub fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.enabled != enabled {
            self.enabled = enabled;
            cx.notify();
        }
    }
}

/// Activation payload (shared with [`crate::primitives::button::ButtonEvent`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconButtonEvent {
    /// The button was activated (click or Enter/Space while focused).
    Activated { keyboard: bool },
}

/// Builder for one rendered icon button.
#[derive(IntoElement)]
pub struct IconButton {
    state: Entity<IconButtonState>,
    palette: Palette,
    icon_id: IconId,
    size: f32,
    on_activate: OptEventCallback<IconButtonEvent>,
}

impl IconButton {
    /// Build an icon button bound to `state`.
    pub fn new(state: Entity<IconButtonState>, icon_id: IconId, palette: Palette) -> Self {
        Self {
            state,
            palette,
            icon_id,
            size: 16.0,
            on_activate: None,
        }
    }

    /// Icon pixel size (default 16).
    #[must_use]
    pub fn icon_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Activate handler (pointer + keyboard).
    #[must_use]
    pub fn on_activate(
        mut self,
        handler: impl Fn(&IconButtonEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_activate = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for IconButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (enabled, focus_handle) = self
            .state
            .read_with(cx, |state, _| (state.enabled, state.focus_handle.clone()));
        let palette = self.palette;
        let on_activate = self.on_activate.clone();
        let id = ElementId::named_usize(
            "tm-icon-button",
            self.state.entity_id().as_non_zero_u64().get() as usize,
        );

        let focus_handle = focus_handle.clone().tab_stop(enabled);

        div()
            .id(id)
            .debug_selector(|| "tm-icon-button".into())
            .track_focus(&focus_handle)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .size(px(28.0))
            .rounded_full()
            .bg(crate::theme_binding::fill(palette.surface))
            .text_color(crate::theme_binding::hsla(palette.fg))
            .hover(|style| style.bg(crate::theme_binding::fill(hover_fill(palette.surface))))
            .active(|style| style.bg(crate::theme_binding::fill(active_fill(palette.surface))))
            .focus(|style| style.border_color(crate::theme_binding::hsla(palette.ring)))
            .when(!enabled, |el| el.opacity(0.5))
            .on_key_down({
                let on_activate = on_activate.clone();
                move |event: &KeyDownEvent, window, cx| {
                    if !enabled {
                        return;
                    }
                    let stroke = &event.keystroke;
                    let unmodified = !stroke.modifiers.modified();
                    if unmodified && matches!(stroke.key.as_str(), "enter" | "space") {
                        cx.stop_propagation();
                        if let Some(on_activate) = &on_activate {
                            on_activate(&IconButtonEvent::Activated { keyboard: true }, window, cx);
                        }
                    }
                }
            })
            .on_click(move |event: &ClickEvent, window, cx| {
                if !enabled {
                    return;
                }
                let keyboard = matches!(event, ClickEvent::Keyboard(_));
                if let Some(on_activate) = &on_activate {
                    on_activate(&IconButtonEvent::Activated { keyboard }, window, cx);
                }
            })
            .child(icon(self.icon_id).size(px(self.size)))
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_icon_button_tests.rs"]
mod tests;
