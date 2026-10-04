//! Optional setup entry and keyboard intents over the application controller.

use crate::{TuiApp, TuiSurface};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use taskmanager_application::first_run::{FirstRunCompletion, FirstRunPhase};
use taskmanager_application::i18n::t;
use taskmanager_application::{PlatformClient, PlatformEffect, SetupScriptRequest, UrlOpenRequest};
use taskmanager_assets::product::REPOSITORY_URL;
use taskmanager_core::core::setup::SetupScriptAction;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, QuitReason};

#[derive(Clone, Debug, Default)]
pub(crate) struct FirstRunTargetView {
    pub(crate) scroll: usize,
}

impl TuiApp {
    pub(crate) fn open_first_run(&mut self) {
        if self.first_run.view().info.is_some() {
            self.open_local_surface(TuiSurface::FirstRun(FirstRunTargetView::default()));
        }
    }
    pub(crate) fn apply_first_run_completion(&mut self, outcome: FirstRunCompletion) {
        if outcome == FirstRunCompletion::Restart {
            self.shell.request_quit(QuitReason::Restart);
        }
        if outcome != FirstRunCompletion::Unchanged && self.first_run.view().info.is_none() {
            self.dismiss_local_surface_kind(crate::TuiSurfaceKind::FirstRun);
        }
    }
    pub(crate) fn submit_first_run_action(
        &mut self,
        action: SetupScriptAction,
        platform: Option<&mut PlatformClient>,
        now_ms: u64,
    ) {
        if self.local_surface_kind() == Some(crate::TuiSurfaceKind::FirstRun) {
            self.first_run.request(action, platform, now_ms);
        }
    }
    pub(crate) fn first_run_copy_payload(&self, field: char) -> Option<String> {
        let info = self.first_run.view().info.as_ref()?;
        match field {
            '1' => Some(info.path.display().to_string()),
            '2' => Some(info.run_command.clone()),
            '3' => Some(info.revert_command.clone()),
            _ => None,
        }
    }
}

pub(crate) fn handle_key(app: &mut TuiApp, key: KeyEvent) -> Option<PlatformEffect> {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.close_local_overlays(),
        KeyCode::Char('1' | '2' | '3') => {
            if let KeyCode::Char(field) = key.code
                && let Some(payload) = app.first_run_copy_payload(field)
            {
                let result = crate::clipboard::write_clipboard(&mut std::io::stdout(), &payload);
                let (severity, message) = match result {
                    Ok(()) => (FeedbackSeverity::Success, t("common.copied").to_owned()),
                    Err(error) => (FeedbackSeverity::Error, error.to_string()),
                };
                app.report_notice(
                    FeedbackSource::Clipboard,
                    severity,
                    FeedbackLifecycle::UntilReplaced,
                    message,
                );
            }
        }
        KeyCode::Char('d') => {
            return Some(PlatformEffect::OpenUrl(UrlOpenRequest {
                url: REPOSITORY_URL.into(),
            }));
        }
        KeyCode::Up
        | KeyCode::Down
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Home
        | KeyCode::End => {
            if let Some(TuiSurface::FirstRun(view)) = app.local_surface_mut() {
                view.scroll = match key.code {
                    KeyCode::Up => view.scroll.saturating_sub(1),
                    KeyCode::Down => view.scroll.saturating_add(1),
                    KeyCode::PageUp => view.scroll.saturating_sub(5),
                    KeyCode::PageDown => view.scroll.saturating_add(5),
                    KeyCode::Home => 0,
                    KeyCode::End => usize::MAX,
                    _ => view.scroll,
                };
            }
        }
        _ => {
            if app.first_run.view().action_pending() {
                return None;
            }
            let action = match key.code {
                KeyCode::Enter | KeyCode::Char('v') => Some(SetupScriptAction::View),
                KeyCode::Char('r') => Some(SetupScriptAction::Run),
                KeyCode::Char('u') => Some(SetupScriptAction::Revert),
                KeyCode::Char('R')
                    if app.first_run.view().phase == FirstRunPhase::RestartRequired =>
                {
                    Some(SetupScriptAction::Restart)
                }
                KeyCode::Char('t')
                    if matches!(app.first_run.view().phase, FirstRunPhase::Failed(_)) =>
                {
                    app.first_run.view().last_action.filter(|action| {
                        matches!(action, SetupScriptAction::Run | SetupScriptAction::Revert)
                    })
                }
                _ => None,
            };
            return action.map(|action| PlatformEffect::SetupScript(SetupScriptRequest { action }));
        }
    }
    None
}

#[cfg(test)]
#[path = "../tests/headless/first_run_tests.rs"]
mod tests;
