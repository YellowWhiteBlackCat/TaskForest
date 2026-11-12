//! The non-modal feedback card: the Iced port of GPUI's toast surface.
//!
//! The shell owns the one notice fact (copy, severity and lifetime). This
//! component spends the shared Card glass, shared status palette and shared
//! `content_button` focus adapter. It never focuses itself on mount and its
//! dismissal is side-effect free.

use std::rc::Rc;

use iced::widget::{container, row, text};
use iced::{Alignment, Background, Length, Padding};
use taskmanager_application::i18n::t;
use taskmanager_icons::status_icon;
use taskmanager_shell::FeedbackPresentation;
use taskmanager_theme::{GlassSurfaceRole, Theme, tokens};

use crate::app::{FocusTarget, Message};
use crate::ui::components::{ContentButtonOptions, IcedElement, content_button};

const MAX_CARD_WIDTH: f32 = 420.0;
const ICON_EXTENT: f32 = 16.0;

#[must_use]
pub(crate) fn toast<'a>(
    theme_snapshot: &'a Theme,
    notice: FeedbackPresentation<'a>,
    viewport_width: f32,
) -> IcedElement<'a> {
    let palette = theme_snapshot.palette();
    let status_colors = crate::theme_binding::status_colors(theme_snapshot);
    let status = notice.status_kind();
    let accent = status_colors.resolve(status);
    let icon = crate::icons::icon_tinted(status_icon(status), ICON_EXTENT, accent);
    let message = text(notice.text().to_owned())
        .size(f32::from(tokens::FONT_12))
        .width(Length::Fill)
        .wrapping(iced::widget::text::Wrapping::Glyph);
    let dismiss = content_button(ContentButtonOptions {
        content: text(t("common.close"))
            .size(f32::from(tokens::FONT_11))
            .into(),
        style: Rc::new(move |_, status| {
            let hovered = matches!(status, iced::widget::button::Status::Hovered);
            iced::widget::button::Style {
                background: hovered
                    .then(|| Background::Color(crate::theme_binding::color(palette.border))),
                text_color: crate::theme_binding::color(palette.fg_muted),
                ..iced::widget::button::Style::default()
            }
        }),
        padding: Padding::from([0.0, f32::from(tokens::SPACE_6)]),
        width: None,
        height: None,
        on_press: Some(Message::DismissToast),
        target: Some(FocusTarget::ToastDismiss),
        focus_color: crate::theme::focus_ring_color(theme_snapshot, false),
        focus_radius: f32::from(palette.control_radius),
        activate_on_pointer: true,
        enabled: true,
        clip: false,
    });
    let card = container(
        row![icon, message, dismiss]
            .spacing(f32::from(tokens::SPACE_8))
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .clip(true)
    .style(move |_| {
        let mut style = crate::theme::glass_surface_style(theme_snapshot, GlassSurfaceRole::Toast);
        style.snap = true;
        style
    })
    .padding(Padding::from([
        f32::from(tokens::SPACE_4),
        f32::from(tokens::SPACE_12),
    ]))
    .width(Length::Fixed(card_width(viewport_width)));
    container(card)
        .width(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .padding(Padding {
            top: f32::from(tokens::SPACE_12),
            right: f32::from(tokens::SPACE_16),
            bottom: 0.0,
            left: f32::from(tokens::SPACE_16),
        })
        .into()
}

/// Reserve the toast's own top slot before the page body.
///
/// Non-modal feedback must not block or cover interactive page controls. The
/// body receives the frame's remaining vertical extent below the notice, so
/// search/filter/action rows stay reachable while the shell owns the toast
/// fact and dismissal.
#[must_use]
pub(crate) fn toast_body<'a>(
    theme_snapshot: &'a Theme,
    notice: FeedbackPresentation<'a>,
    viewport_width: f32,
    body: IcedElement<'a>,
) -> IcedElement<'a> {
    iced::widget::column![toast(theme_snapshot, notice, viewport_width), body]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn card_width(viewport_width: f32) -> f32 {
    let side_inset = 2.0 * f32::from(tokens::SPACE_16);
    (viewport_width - side_inset).clamp(0.0, MAX_CARD_WIDTH)
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/toast_tests.rs"]
mod tests;
