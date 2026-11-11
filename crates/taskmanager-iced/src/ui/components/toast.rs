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
use taskmanager_shell::{FeedbackPresentation, FeedbackTone};
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui_contract::IconId;

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
    let accent = crate::theme_binding::color(match notice.tone() {
        FeedbackTone::Neutral => palette.accent,
        FeedbackTone::Positive => palette.success,
        FeedbackTone::Caution => palette.warning,
        FeedbackTone::Danger => palette.danger,
    });
    let icon = crate::icons::icon_tinted(icon_for(notice), ICON_EXTENT, accent);
    let message = text(notice.text().to_owned())
        .size(f32::from(tokens::FONT_12))
        .width(Length::Fill)
        .wrapping(iced::widget::text::Wrapping::None);
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
    .style(move |_| crate::theme::card_style(theme_snapshot))
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

fn icon_for(notice: FeedbackPresentation<'_>) -> IconId {
    match notice.tone() {
        FeedbackTone::Caution | FeedbackTone::Danger => IconId::Alert,
        FeedbackTone::Neutral | FeedbackTone::Positive => IconId::Health,
    }
}

fn card_width(viewport_width: f32) -> f32 {
    let side_inset = 2.0 * f32::from(tokens::SPACE_16);
    (viewport_width - side_inset).clamp(0.0, MAX_CARD_WIDTH)
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/toast_tests.rs"]
mod tests;
