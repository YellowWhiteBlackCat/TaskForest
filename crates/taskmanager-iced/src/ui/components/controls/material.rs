//! The ONE button composition seam the Iced component family and the older
//! focus-shell adapters spend.
//!
//! Paint reads `theme::control_glass` through the shared `ControlRole` /
//! `ControlState` resolver; geometry is the live theme's `control_extent` and
//! `control_padding_x`, never a second ladder. Status only reweights paint.
//! The native Iced button keeps its own pointer state; the focus shell keeps
//! keyboard activation and the keyboard-only ring.

use iced::Length;
use iced::advanced::text::IntoFragment;
use iced::widget::{button, text};
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::presentation::sizes::ControlRole;

use crate::app::{FocusTarget, Message};
use crate::focus::{FocusableButton, focus_id};
use crate::ui::components::IcedElement;

use super::state::{ButtonTone, ControlPaintState};

/// Build the inner native button every activation shape composes. The caller
/// owns the focus shell, so no widget tree here owns a second focus policy.
/// Horizontal padding comes from the shared `ControlRole` resolver and the
/// vertical gutter from the theme spacing token; callers choose whether their
/// slot spends the role's measured extent or keeps an intrinsic row height.
#[must_use]
pub(crate) fn control_button<'a>(
    theme_snapshot: Theme,
    content: iced::Element<'a, Message, iced::Theme, iced::Renderer>,
    role: ControlRole,
    tone: ButtonTone,
    on_press: Option<Message>,
) -> button::Button<'a, Message, iced::Theme, iced::Renderer> {
    let padding_x = crate::theme::control_padding_x(&theme_snapshot, role);
    let mut inner = button(content)
        .padding([f32::from(tokens::SPACE_4), padding_x])
        .style(move |_, status| {
            super::state::button_style(
                &theme_snapshot,
                role,
                tone,
                ControlPaintState::from_button(status),
            )
        });
    if let Some(message) = on_press {
        inner = inner.on_press(message);
    }
    inner
}

/// Compose the shared focus shell around a FULL measured control button. The
/// activation family spends the resolved `ControlRole::Button` extent.
#[must_use]
pub(crate) fn focusable_control_button<'a>(
    theme_snapshot: &'a Theme,
    target: FocusTarget,
    label: impl IntoFragment<'a>,
    on_press: Message,
    tone: ButtonTone,
    enabled: bool,
) -> IcedElement<'a> {
    let extent = crate::theme::control_extent(theme_snapshot, ControlRole::Button);
    let inner = control_button(
        *theme_snapshot,
        text(label).size(f32::from(tokens::FONT_13)).into(),
        ControlRole::Button,
        tone,
        enabled.then_some(on_press.clone()),
    )
    .height(Length::Fixed(extent));
    focus_shell(
        theme_snapshot,
        target,
        inner.into(),
        on_press,
        tone,
        enabled,
    )
}

/// Compose the shared focus shell around an INTRINSIC control button.
///
/// Dense legacy adapters (the Performance graph-point pills, for example)
/// live inside tight slot budgets that already measured their full row
/// footprint. Keeping their native text height preserves that arithmetic;
/// the material, radius and focus policy remain the shared control family's.
#[must_use]
pub(crate) fn focusable_control_button_intrinsic<'a>(
    theme_snapshot: &'a Theme,
    target: FocusTarget,
    label: impl IntoFragment<'a>,
    on_press: Message,
    tone: ButtonTone,
    enabled: bool,
) -> IcedElement<'a> {
    let inner = control_button(
        *theme_snapshot,
        text(label).size(f32::from(tokens::FONT_13)).into(),
        ControlRole::Button,
        tone,
        enabled.then_some(on_press.clone()),
    );
    focus_shell(
        theme_snapshot,
        target,
        inner.into(),
        on_press,
        tone,
        enabled,
    )
}

fn focus_shell<'a>(
    theme_snapshot: &'a Theme,
    target: FocusTarget,
    content: iced::Element<'a, Message, iced::Theme, iced::Renderer>,
    on_press: Message,
    tone: ButtonTone,
    enabled: bool,
) -> IcedElement<'a> {
    FocusableButton::new(
        focus_id(target),
        content,
        on_press,
        target,
        crate::theme::focus_ring_color(theme_snapshot, tone == ButtonTone::Danger),
        crate::theme::scaled_radius(theme_snapshot, theme_snapshot.palette().control_radius),
        false,
    )
    .with_enabled(enabled)
    .into()
}

/// The owned-theme variant: a lazy body must own every captured style input
/// for its `'static` widget tree, so the theme snapshot travels with it.
#[must_use]
pub(crate) fn focusable_control_button_owned(
    theme_snapshot: Theme,
    target: FocusTarget,
    label: String,
    on_press: Message,
    tone: ButtonTone,
    enabled: bool,
) -> IcedElement<'static> {
    let inner = control_button(
        theme_snapshot,
        text(label).size(f32::from(tokens::FONT_13)).into(),
        ControlRole::Button,
        tone,
        enabled.then_some(on_press.clone()),
    );
    FocusableButton::new(
        focus_id(target),
        inner.into(),
        on_press,
        target,
        crate::theme::focus_ring_color(&theme_snapshot, tone == ButtonTone::Danger),
        crate::theme::scaled_radius(&theme_snapshot, theme_snapshot.palette().control_radius),
        false,
    )
    .with_enabled(enabled)
    .into()
}

/// A shared control button that carries a semantic icon before its label.
/// The glyph comes from the shared icon registry; the icon role's own
/// geometry keeps the hit target square.
#[must_use]
pub(crate) fn focusable_icon_label_button_intrinsic<'a>(
    theme_snapshot: &'a Theme,
    target: FocusTarget,
    icon_id: IconId,
    label: impl IntoFragment<'a>,
    on_press: Message,
    tone: ButtonTone,
    enabled: bool,
) -> IcedElement<'a> {
    let glyph = crate::icons::icon(theme_snapshot, icon_id, f32::from(tokens::ICON_INLINE));
    let content = iced::widget::row![glyph, text(label).size(f32::from(tokens::FONT_13))]
        .spacing(f32::from(tokens::SPACE_8))
        .align_y(iced::Alignment::Center);
    let inner = control_button(
        *theme_snapshot,
        content.into(),
        ControlRole::Button,
        tone,
        enabled.then_some(on_press.clone()),
    );
    FocusableButton::new(
        focus_id(target),
        inner.into(),
        on_press,
        target,
        crate::theme::focus_ring_color(theme_snapshot, tone == ButtonTone::Danger),
        crate::theme::scaled_radius(theme_snapshot, theme_snapshot.palette().control_radius),
        false,
    )
    .with_enabled(enabled)
    .into()
}
