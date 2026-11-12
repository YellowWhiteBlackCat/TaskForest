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
use iced::{Color, Padding};
use std::rc::Rc;
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::presentation::sizes::ControlRole;

use crate::app::{FocusTarget, Message};
use crate::focus::{FocusableButton, focus_id};
use crate::ui::components::IcedElement;

use super::state::{ButtonTone, button_ink};

type StyleFn<'a> = Rc<dyn Fn(&iced::Theme, button::Status) -> button::Style + 'a>;

/// Composition inputs for one native Iced button. The caller owns the content,
/// exact style resolver and geometry; the adapter owns the single native
/// constructor and, when a focus target is supplied, the shared focus shell.
pub(crate) struct ContentButtonOptions<'a> {
    pub(crate) content: iced::Element<'a, Message, iced::Theme, iced::Renderer>,
    pub(crate) style: StyleFn<'a>,
    pub(crate) padding: Padding,
    pub(crate) width: Option<Length>,
    pub(crate) height: Option<Length>,
    pub(crate) on_press: Option<Message>,
    pub(crate) target: Option<FocusTarget>,
    pub(crate) focus_color: Color,
    pub(crate) focus_radius: f32,
    pub(crate) activate_on_pointer: bool,
    pub(crate) enabled: bool,
    pub(crate) clip: bool,
}

/// The ONE native-button constructor. A target wraps the control in the shared
/// focus shell; without one, the native pointer path alone owns activation
/// (search clear, switch and segmented segments use this branch).
#[must_use]
pub(crate) fn content_button<'a>(options: ContentButtonOptions<'a>) -> IcedElement<'a> {
    let ContentButtonOptions {
        content,
        style,
        padding,
        width,
        height,
        on_press,
        target,
        focus_color,
        focus_radius,
        activate_on_pointer,
        enabled,
        clip,
    } = options;
    let mut inner = button(content)
        .padding(padding)
        .style(move |theme, status| (*style)(theme, status));
    inner = inner.clip(clip);
    if let Some(width) = width {
        inner = inner.width(width);
    }
    if let Some(height) = height {
        inner = inner.height(height);
    }
    if let Some(message) = on_press.clone().filter(|_| enabled) {
        inner = inner.on_press(message);
    }
    match (target, on_press) {
        (Some(target), Some(on_press)) => FocusableButton::new(
            focus_id(target),
            inner.into(),
            on_press,
            target,
            focus_color,
            focus_radius,
            activate_on_pointer,
        )
        .with_enabled(enabled)
        .into(),
        _ => inner.into(),
    }
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
    glass_content_button(
        *theme_snapshot,
        text(label).size(f32::from(tokens::FONT_13)).into(),
        GlassContentButtonRequest {
            role: ControlRole::Button,
            tone,
            target,
            on_press,
            height: Some(Length::Fixed(extent)),
            enabled,
        },
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
    glass_content_button(
        *theme_snapshot,
        text(label).size(f32::from(tokens::FONT_13)).into(),
        GlassContentButtonRequest {
            role: ControlRole::Button,
            tone,
            target,
            on_press,
            height: None,
            enabled,
        },
    )
}

fn glass_content_button<'a>(
    theme_snapshot: Theme,
    content: iced::Element<'a, Message, iced::Theme, iced::Renderer>,
    request: GlassContentButtonRequest,
) -> IcedElement<'a> {
    let GlassContentButtonRequest {
        role,
        tone,
        target,
        on_press,
        height,
        enabled,
    } = request;
    let padding_x = crate::theme::control_padding_x(&theme_snapshot, role);
    let style_theme = theme_snapshot;
    content_button(ContentButtonOptions {
        content,
        style: Rc::new(move |_, status| {
            super::state::button_style(&style_theme, role, tone, button_ink(status))
        }),
        padding: Padding::from([f32::from(tokens::SPACE_4), padding_x]),
        width: None,
        height,
        on_press: Some(on_press),
        target: Some(target),
        focus_color: crate::theme::focus_ring_color(&theme_snapshot, tone == ButtonTone::Danger),
        focus_radius: crate::theme::scaled_radius(
            &theme_snapshot,
            theme_snapshot.palette().control_radius,
        ),
        activate_on_pointer: false,
        enabled,
        clip: false,
    })
}

struct GlassContentButtonRequest {
    role: ControlRole,
    tone: ButtonTone,
    target: FocusTarget,
    on_press: Message,
    height: Option<Length>,
    enabled: bool,
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
    let padding_x = crate::theme::control_padding_x(&theme_snapshot, ControlRole::Button);
    let style_theme = theme_snapshot;
    content_button(ContentButtonOptions {
        content: text(label).size(f32::from(tokens::FONT_13)).into(),
        style: Rc::new(move |_, status| {
            super::state::button_style(&style_theme, ControlRole::Button, tone, button_ink(status))
        }),
        padding: Padding::from([f32::from(tokens::SPACE_4), padding_x]),
        width: None,
        height: None,
        on_press: Some(on_press),
        target: Some(target),
        focus_color: crate::theme::focus_ring_color(&theme_snapshot, tone == ButtonTone::Danger),
        focus_radius: crate::theme::scaled_radius(
            &theme_snapshot,
            theme_snapshot.palette().control_radius,
        ),
        activate_on_pointer: false,
        enabled,
        clip: false,
    })
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
    glass_content_button(
        *theme_snapshot,
        content.into(),
        GlassContentButtonRequest {
            role: ControlRole::Button,
            tone,
            target,
            on_press,
            height: None,
            enabled,
        },
    )
}
