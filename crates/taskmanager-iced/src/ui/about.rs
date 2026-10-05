//! Independent application About over shared build metadata.

use super::components::IcedElement;
use super::overlays::bounded_modal_overlay;
use crate::app::{FocusTarget, Message};
use crate::focus;
use iced::Length;
use iced::widget::{column, scrollable, text};
use taskmanager_application::i18n::t;
use taskmanager_assets::product;
use taskmanager_shell::presentation::about::metadata;

pub(super) fn render(app: &crate::IcedApp) -> IcedElement<'_> {
    let metadata = metadata(
        env!("CARGO_PKG_VERSION"),
        product::LICENSE_SPDX,
        product::REPOSITORY_URL,
    );
    let body = scrollable(
        column![
            text(metadata.name).size(22),
            text(metadata.description),
            text(metadata.details_text())
        ]
        .spacing(12)
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .width(Length::Fill)
    .into();
    let actions = vec![
        focus::ghost_button(
            app.theme(),
            FocusTarget::AboutRepository,
            t("about.open_repository"),
            Message::OpenRepository,
        ),
        focus::ghost_button(
            app.theme(),
            FocusTarget::AboutSystemInformation,
            t("about.system_information"),
            Message::OpenSystemInformation,
        ),
        focus::ghost_button(
            app.theme(),
            FocusTarget::AboutCopyDetails,
            t("about.copy_details"),
            Message::CopyAboutDetails,
        ),
        focus::ghost_button(
            app.theme(),
            FocusTarget::Export,
            t("diagnostics.action"),
            Message::GenerateDiagnosticsReport,
        ),
    ];
    bounded_modal_overlay(app, t("about.title"), body, actions)
}

#[cfg(test)]
#[path = "../../tests/gui/ui/about_tests.rs"]
mod tests;
