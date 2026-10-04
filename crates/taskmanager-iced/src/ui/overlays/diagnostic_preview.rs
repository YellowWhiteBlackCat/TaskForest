//! Review the frozen sanitized plan before explicit background publication.

use iced::widget::{column, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::diagnostics::{
    diagnostic_failure_message, diagnostic_preview_text,
};
use taskmanager_theme::tokens;

use super::bounded_modal_overlay;
use crate::app::{FocusTarget, Message};
use crate::focus;

pub(crate) fn diagnostic_bundle_overlay<'a>(
    app: &'a crate::IcedApp,
    state: &'a DiagnosticBundleUiState,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let theme = app.theme();
    let mut body = column![].spacing(f32::from(tokens::SPACE_8));
    let contents = match state {
        DiagnosticBundleUiState::Preview(plan) => diagnostic_preview_text(plan.preview()),
        DiagnosticBundleUiState::Writing(preview) => {
            body = body.push(text(t("diagnostics.writing")));
            diagnostic_preview_text(preview)
        }
        DiagnosticBundleUiState::Complete(path) => {
            t("diagnostics.complete").replace("{path}", &path.display().to_string())
        }
        DiagnosticBundleUiState::Failed(error) => diagnostic_failure_message(error),
    };
    body = body.push(
        scrollable(text(contents).size(f32::from(tokens::FONT_BODY)))
            .height(Length::Fill)
            .width(Length::Fill),
    );
    let action = match state {
        DiagnosticBundleUiState::Preview(_) => Some((
            t("diagnostics.export"),
            Message::ConfirmDiagnosticsExport,
            FocusTarget::DiagnosticConfirm,
        )),
        DiagnosticBundleUiState::Failed(_) => Some((
            t("first_run.retry"),
            Message::RetryDiagnostics,
            FocusTarget::DiagnosticRetry,
        )),
        DiagnosticBundleUiState::Writing(_) | DiagnosticBundleUiState::Complete(_) => None,
    };
    let mut actions = row![];
    if let Some((label, message, target)) = action {
        actions = actions.push(focus::dynamic_button(
            theme,
            target,
            label.to_owned(),
            message,
            false,
        ));
    }
    bounded_modal_overlay(app, t("diagnostics.title"), body.into(), actions.into())
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/overlays/diagnostic_tests.rs"]
mod tests;
