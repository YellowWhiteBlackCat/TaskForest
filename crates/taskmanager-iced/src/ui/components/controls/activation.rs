//! Interactive activations: buttons, icon buttons, pills, and checkboxes.
//!
//! Native Iced widgets own pointer, caret and toggle events. The shared focus
//! shell owns Tab stops, Enter/Space activation, and the keyboard-only ring;
//! shared `ControlRole` measurements own the extents. Copy arrives already
//! resolved through `taskmanager-application::i18n` by the caller.

use iced::advanced::text::IntoFragment;
use iced::widget::{checkbox, container};
use iced::{Background, Length, Padding};
use std::rc::Rc;
use taskmanager_theme::color::mix;
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::presentation::sizes::ControlRole;

use super::material::focusable_control_button;
use super::state::{ButtonTone, ControlPaintState};
use crate::app::{FocusTarget, Message};
use crate::focus::{FocusableButton, focus_id};
use crate::theme;
use crate::ui::components::IcedElement;

/// A shared-glass, focus-reachable button. Disabled keeps the same hit target
/// and geometry, but drops both the pointer press and the focus stop.
#[must_use]
pub(crate) fn glass_button<'a>(
    theme_snapshot: &'a Theme,
    target: FocusTarget,
    label: impl IntoFragment<'a>,
    on_press: Message,
    tone: ButtonTone,
    enabled: bool,
) -> IcedElement<'a> {
    focusable_control_button(theme_snapshot, target, label, on_press, tone, enabled)
}

/// An icon-only action. The shared `label` is exposed through the same
/// tooltip primitive the rest of the product uses: Iced 0.14 has no widget
/// accessible-name operation, so the localized copy becomes a visible hint
/// instead of being dropped.
#[must_use]
pub(crate) fn icon_button<'a>(
    theme_snapshot: &'a Theme,
    target: FocusTarget,
    icon_id: IconId,
    label: &'a str,
    on_press: Message,
    enabled: bool,
) -> IcedElement<'a> {
    let extent = theme::control_extent(theme_snapshot, ControlRole::IconButton);
    let glyph = crate::icons::icon(theme_snapshot, icon_id, f32::from(tokens::ICON_INLINE));
    let style_theme = *theme_snapshot;
    let inner = super::content_button(super::ContentButtonOptions {
        content: container(glyph)
            .width(Length::Fixed(extent))
            .height(Length::Fixed(extent))
            .center_x(Length::Fixed(extent))
            .center_y(Length::Fixed(extent))
            .into(),
        style: Rc::new(move |_, status| {
            super::state::button_style(
                &style_theme,
                ControlRole::IconButton,
                ButtonTone::Secondary,
                ControlPaintState::from_button(status),
            )
        }),
        padding: Padding::ZERO,
        width: Some(Length::Fixed(extent)),
        height: Some(Length::Fixed(extent)),
        on_press: enabled.then_some(on_press.clone()),
        target: None,
        focus_color: theme::focus_ring_color(theme_snapshot, false),
        focus_radius: theme::scaled_radius(theme_snapshot, theme_snapshot.palette().control_radius),
        activate_on_pointer: false,
        enabled: true,
        clip: false,
    });
    let activation: IcedElement<'a> = FocusableButton::new(
        focus_id(target),
        inner,
        on_press,
        target,
        theme::focus_ring_color(theme_snapshot, false),
        theme::scaled_radius(theme_snapshot, theme_snapshot.palette().control_radius),
        false,
    )
    .with_enabled(enabled)
    .into();
    crate::ui::components::tooltip(theme_snapshot, activation, label)
}

/// A filter/segment pill: one activation family with idle and active ink,
/// painted over shared Button glass and the shared focus ring.
#[must_use]
pub(crate) fn pill<'a>(
    theme_snapshot: &'a Theme,
    target: FocusTarget,
    body: impl IntoFragment<'a>,
    active: bool,
    on_press: Message,
) -> IcedElement<'a> {
    glass_button(
        theme_snapshot,
        target,
        body,
        on_press,
        if active {
            ButtonTone::Primary
        } else {
            ButtonTone::Secondary
        },
        true,
    )
}

/// A binary checkbox: Iced's native pointer toggle receives the NEW boolean,
/// while the focus shell's Enter/Space publishes the same typed transition.
/// Checked state changes ink only; the Toggle-role geometry is invariant.
#[must_use]
pub(crate) fn check_box<'a>(
    theme_snapshot: &'a Theme,
    target: FocusTarget,
    checked: bool,
    label: impl IntoFragment<'a>,
    on_toggle: impl Fn(bool) -> Message + 'a,
    disabled: bool,
) -> IcedElement<'a> {
    let extent = theme::control_extent(theme_snapshot, ControlRole::Toggle);
    let box_size = extent * 0.5;
    let on_toggle = std::rc::Rc::new(on_toggle);
    let confirm = on_toggle(!checked);
    let pointer_toggle = on_toggle.clone();
    let mut control = checkbox(checked)
        .label(label)
        .size(box_size)
        .spacing(f32::from(tokens::SPACE_8))
        .text_size(f32::from(tokens::FONT_13))
        .font(crate::theme_binding::ui_font(theme_snapshot))
        .style(move |_, status| {
            let is_checked = match status {
                checkbox::Status::Active { is_checked }
                | checkbox::Status::Hovered { is_checked }
                | checkbox::Status::Disabled { is_checked } => is_checked,
            };
            checkbox_style(
                theme_snapshot,
                ControlPaintState::from_checkbox(status),
                is_checked,
            )
        });
    if !disabled {
        control = control.on_toggle(move |next| pointer_toggle(next));
    }
    FocusableButton::new(
        focus_id(target),
        control.into(),
        confirm,
        target,
        theme::focus_ring_color(theme_snapshot, false),
        theme::scaled_radius(theme_snapshot, theme_snapshot.palette().control_radius),
        false,
    )
    .with_enabled(!disabled)
    .into()
}

#[must_use]
pub(crate) fn checkbox_style(
    theme: &Theme,
    state: ControlPaintState,
    checked: bool,
) -> checkbox::Style {
    let glass = theme::control_glass(theme, ControlRole::Toggle, state.visual());
    let palette = theme.palette();
    let mut background = glass
        .background
        .unwrap_or_else(|| Background::Color(crate::theme_binding::color(palette.surface)));
    if checked && state != ControlPaintState::Disabled {
        background = Background::Color(checked_glass_background(theme));
    }
    checkbox::Style {
        background,
        icon_color: crate::theme_binding::color(theme.accent_text),
        border: glass.border,
        text_color: glass
            .text_color
            .or_else(|| Some(crate::theme_binding::color(palette.fg))),
    }
}

/// The checked glass uses the theme's own accent mix rather than a hardcoded
/// color. Keeping it beside `checkbox_style` makes the paint seam testable.
#[must_use]
pub(crate) fn checked_glass_background(theme: &Theme) -> iced::Color {
    crate::theme_binding::color(mix(theme.palette().accent, theme.glass_card().fill, 0.18))
}
