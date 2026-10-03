//! Diagnostic report preview and error overlays for Iced.

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};
use taskmanager_application::i18n::t;

use crate::app::Message;
use crate::theme;

pub(crate) fn diagnostic_preview_overlay<'a>(
    app: &'a crate::IcedApp,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let title = t("diagnostics.preview_title");
    let report = match crate::export::system_diagnostics_markdown(
        app.shell.projection().hardware.as_ref(),
        app.shell.projection().snapshot.as_ref(),
        Vec::<String>::new(),
    ) {
        Ok(text) => text,
        Err(_) => "Diagnostic report preview unavailable".to_owned(),
    };

    let content = column![
        row![
            text(if title.is_empty() {
                "System Diagnostics Preview"
            } else {
                title
            })
            .size(18),
        ]
        .padding(12),
        container(
            scrollable(
                container(text(report).size(12))
                    .padding(12)
                    .width(Length::Fill),
            )
            .height(Length::Fixed(360.0)),
        )
        .style(move |_| theme::card_style(theme_snapshot)),
        row![
            button(text(t("action.copy")).size(13)).on_press(Message::GenerateDiagnosticsReport),
            button(text(t("action.close")).size(13)).on_press(Message::DismissOverlay),
        ]
        .spacing(12)
        .padding(12)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .max_width(680);

    container(
        container(content)
            .padding(16)
            .style(move |_| theme::card_style(theme_snapshot)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

pub(crate) fn diagnostic_failure_overlay<'a>(
    app: &'a crate::IcedApp,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let content = column![
        text("System Diagnostics Failed").size(18),
        text("Diagnostic report generation failed or timed out.").size(13),
        row![
            button(text(t("action.retry")).size(13)).on_press(Message::GenerateDiagnosticsReport),
            button(text(t("action.close")).size(13)).on_press(Message::DismissOverlay),
        ]
        .spacing(12)
        .padding(12),
    ]
    .spacing(12)
    .max_width(480);

    container(
        container(content)
            .padding(16)
            .style(move |_| theme::card_style(theme_snapshot)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}
