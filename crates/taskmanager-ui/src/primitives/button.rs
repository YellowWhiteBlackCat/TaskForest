//! Typed button primitive (M2: state entity + builder + element render).
//!
//! Variants select a palette-derived fill family; hover/active are derived
//! from the base fill (never hand-picked colors), disabled mutes toward the
//! surface, and the focus ring reads `Palette::ring` (its alpha already
//! encodes the focus-visible decision). Keyboard activation (Enter/Space on a
//! focused tab stop) is dispatched by gpui's interactive element as a
//! [`ClickEvent::Keyboard`], so pointer and keyboard share one handler.

use crate::OptEventCallback;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    App, BoxShadow, ClickEvent, Context, ElementId, Entity, Fill, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement,
    Styled, Window, div, linear_color_stop, linear_gradient, point, px,
};
use taskmanager_icons::icon;

/// The pressed-state shadow: a tighter, fainter drop than the resting state
/// so the button visually sinks under the pointer (Mission-Center-style
/// press feedback). Fixed geometry (the `px` contract), black 8% like the
/// gpui shadow constants.
fn press_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(2.0),
        spread_radius: px(0.0),
        color: gpui::Hsla::black().alpha(0.08),
    }]
}
use taskmanager_theme::Palette;
use taskmanager_ui_contract::IconId;

use crate::styled::{active_fill, disabled_fg, hover_fill};
use taskmanager_theme::color::on_accent;
use taskmanager_theme::tokens;

/// Semantic button variants. The exact colors come from the palette per
/// variant; this enum only selects *which* family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Accent fill, `on_accent` text: the single primary action.
    Primary,
    /// Surface fill + border: neutral/secondary actions.
    Secondary,
    /// Danger fill: destructive actions.
    Danger,
}

/// Interaction state for one button. The owning view holds this entity; the
/// builder is re-created per render from the snapshot fields.
pub struct ButtonState {
    focus_handle: FocusHandle,
    enabled: bool,
}

impl ButtonState {
    /// Create a button state with a fresh focus handle.
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            enabled: true,
        }
    }

    /// The focus handle backing this button (tab stop).
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Whether the button currently accepts activation.
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

/// Typed event payload for button activation (unified pointer + keyboard).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonEvent {
    /// The button was activated (click or Enter/Space while focused).
    Activated {
        /// Whether the activation came from the keyboard.
        keyboard: bool,
    },
}

/// Builder for one rendered button. Handlers are `Rc` so the same callback can
/// be wired into both pointer and keyboard paths.
#[derive(IntoElement)]
pub struct Button {
    state: Entity<ButtonState>,
    palette: Palette,
    variant: ButtonVariant,
    label: Option<SharedString>,
    icon_id: Option<IconId>,
    radius: f32,
    on_activate: OptEventCallback<ButtonEvent>,
}

impl Button {
    /// Build a button bound to `state` with the given palette snapshot.
    pub fn new(state: Entity<ButtonState>, palette: Palette) -> Self {
        Self {
            state,
            palette,
            variant: ButtonVariant::Primary,
            label: None,
            icon_id: None,
            radius: 6.0,
            on_activate: None,
        }
    }

    /// Select the semantic variant family.
    #[must_use]
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set the button label.
    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Place an icon before the label.
    #[must_use]
    pub fn icon(mut self, icon_id: IconId) -> Self {
        self.icon_id = Some(icon_id);
        self
    }

    /// Corner radius override (default 6px).
    #[must_use]
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Activate handler. Fires for pointer click and keyboard activation.
    #[must_use]
    pub fn on_activate(
        mut self,
        handler: impl Fn(&ButtonEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_activate = Some(Rc::new(handler));
        self
    }
}

/// The primary action's fill: a 90° linear gradient from the palette's
/// `gradient_from` (top, brighter) to `gradient_to` (bottom, darker) —
/// Mission Center button styling. Hover/active re-derive both stops via
/// [`primary_state_fill`].
fn primary_fill(palette: &Palette) -> Fill {
    primary_state_fill(palette.gradient_from, palette.gradient_to, |stop| stop)
}

/// Rebuild the primary gradient after mapping each stop through a state
/// transform (`hover_fill` brightens dark accents / deepens light ones,
/// `active_fill` always sinks). Keeping both stops means the gradient shape
/// survives the state change — hover/active stay gradient buttons.
fn primary_state_fill(
    from: taskmanager_theme::Color,
    to: taskmanager_theme::Color,
    map: impl Fn(taskmanager_theme::Color) -> taskmanager_theme::Color,
) -> Fill {
    Fill::from(linear_gradient(
        90.0,
        linear_color_stop(map(from), 0.0),
        linear_color_stop(map(to), 1.0),
    ))
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // The state entity is strongly held by the consumer, so this read is
        // safe; the render is a pure consumer of state (M2).
        let (enabled, focus_handle) = self
            .state
            .read_with(cx, |state, _| (state.enabled, state.focus_handle.clone()));
        let palette = self.palette;

        // Primary fills paint the Mission Center accent gradient
        // (gradient_from → gradient_to, 90° top-light); hover/active lift or
        // sink both stops. Secondary/Danger stay solid fills.
        let (fill, hover, active, text, border) = match self.variant {
            ButtonVariant::Primary => {
                let from = palette.gradient_from;
                let to = palette.gradient_to;
                (
                    primary_fill(&palette),
                    primary_state_fill(from, to, hover_fill),
                    primary_state_fill(from, to, active_fill),
                    on_accent(palette.accent),
                    palette.accent,
                )
            }
            ButtonVariant::Secondary => (
                palette.surface.into(),
                hover_fill(palette.surface).into(),
                active_fill(palette.surface).into(),
                palette.fg,
                palette.border,
            ),
            ButtonVariant::Danger => (
                palette.danger.into(),
                hover_fill(palette.danger).into(),
                active_fill(palette.danger).into(),
                on_accent(palette.danger),
                palette.danger,
            ),
        };

        let on_activate = self.on_activate.clone();
        let id = ElementId::named_usize(
            "tm-button",
            self.state.entity_id().as_non_zero_u64().get() as usize,
        );

        let focus_handle = focus_handle.clone().tab_stop(enabled);

        let element = div()
            .id(id)
            .debug_selector(|| "tm-button".into())
            .track_focus(&focus_handle)
            .cursor_pointer()
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(tokens::SPACE_6)
            .px(tokens::SPACE_12)
            .h(px(28.0))
            .rounded(px(self.radius))
            .bg(fill)
            .text_color(text)
            .text_sm()
            .font_weight(tokens::FONT_WEIGHT_MEDIUM.into())
            .when(self.icon_id.is_some(), |el| el.pl(tokens::SPACE_2))
            // Focus ring: palette.ring is transparent when focus is not
            // keyboard-driven, so this draws only for keyboard focus.
            .focus(|style| style.border_color(palette.ring))
            .when(!enabled, |el| {
                el.opacity(0.5).text_color(disabled_fg(&palette))
            })
            .when(enabled && self.variant != ButtonVariant::Secondary, |el| {
                el.hover(|style| style.bg(hover.clone()))
                    .active(|style| style.bg(active.clone()).shadow(press_shadow()))
            })
            .when(enabled && self.variant == ButtonVariant::Secondary, |el| {
                el.border_1()
                    .border_color(border)
                    .hover(|style| style.bg(hover))
                    .active(|style| style.bg(active).shadow(press_shadow()))
            })
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
                            on_activate(&ButtonEvent::Activated { keyboard: true }, window, cx);
                        }
                    }
                }
            })
            .on_click({
                let on_activate = on_activate.clone();
                move |event: &ClickEvent, window, cx| {
                    if !enabled {
                        return;
                    }
                    let keyboard = matches!(event, ClickEvent::Keyboard(_));
                    if let Some(on_activate) = &on_activate {
                        on_activate(&ButtonEvent::Activated { keyboard }, window, cx);
                    }
                }
            });

        match (self.icon_id, self.label) {
            (Some(icon_id), Some(label)) => {
                element.child(icon(icon_id).size(px(14.0))).child(label)
            }
            (Some(icon_id), None) => element.child(icon(icon_id).size(px(14.0))),
            (None, Some(label)) => element.child(label),
            (None, None) => element,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_button_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_button_press_tests.rs"]
mod press_tests;
