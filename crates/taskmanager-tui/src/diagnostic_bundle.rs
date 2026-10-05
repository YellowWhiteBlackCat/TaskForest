//! Terminal diagnostic review and background publication over the shared port.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_app_host::DiagnosticBundleClient;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::i18n::t;
use taskmanager_application::{DiagnosticBundleSession, DiagnosticBundleTarget};
use taskmanager_shell::presentation::diagnostics::{
    diagnostic_failure_message, diagnostic_preview_text,
};
use taskmanager_ui_contract::IconId;

use crate::ui::containers::Modal;
use crate::ui::process_details::{clamped_scroll, wrapped_content_height};
use crate::{TuiApp, TuiSurface, TuiTheme};

#[derive(Clone, Debug)]
pub(crate) struct DiagnosticBundleTargetView {
    pub(crate) state: DiagnosticBundleUiState,
    pub(crate) scroll: usize,
}

#[derive(Debug, Default)]
pub(crate) enum TuiDiagnosticRuntime {
    #[default]
    Unavailable,
    Active(DiagnosticBundleSession<DiagnosticBundleClient>),
}

impl TuiDiagnosticRuntime {
    fn active_mut(&mut self) -> Option<&mut DiagnosticBundleSession<DiagnosticBundleClient>> {
        match self {
            Self::Unavailable => None,
            Self::Active(session) => Some(session),
        }
    }

    pub(crate) fn close(&mut self) {
        if let Some(session) = self.active_mut() {
            session.close();
        }
    }
}

impl TuiApp {
    pub(crate) fn install_diagnostic_bundle_client(&mut self, client: DiagnosticBundleClient) {
        self.diagnostics = TuiDiagnosticRuntime::Active(DiagnosticBundleSession::new(client));
    }

    pub(crate) fn open_diagnostic_bundle(&mut self) {
        self.diagnostics.close();
        let state = DiagnosticBundleUiState::prepared(
            self.projection()
                .prepare_diagnostic_bundle(Some(&self.config_draft)),
        );
        self.show_diagnostic_bundle(state);
    }

    pub(crate) fn show_diagnostic_bundle(&mut self, state: DiagnosticBundleUiState) {
        self.open_local_surface(TuiSurface::DiagnosticBundle(DiagnosticBundleTargetView {
            state,
            scroll: 0,
        }));
    }

    pub(crate) fn confirm_diagnostic_bundle(&mut self) {
        let filename = format!(
            "taskmanager-diagnostics-{}.json",
            self.projection()
                .snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.timestamp_ms)
        );
        let target = self.export_dir.as_ref().map_or_else(
            || DiagnosticBundleTarget::current_directory(filename.clone()),
            |directory| DiagnosticBundleTarget::path(directory.join(&filename)),
        );
        let mut view = match self.local_surface() {
            Some(TuiSurface::DiagnosticBundle(view)) => view.clone(),
            _ => return,
        };
        if view.state.confirm(self.diagnostics.active_mut(), target)
            && let Some(TuiSurface::DiagnosticBundle(current)) = self.local_surface_mut()
        {
            *current = view;
        }
    }

    pub(crate) fn drain_diagnostic_bundle_completions(&mut self) -> bool {
        let completions = self
            .diagnostics
            .active_mut()
            .map_or_else(Vec::new, DiagnosticBundleSession::drain);
        let mut changed = false;
        for completion in completions {
            if let Some(TuiSurface::DiagnosticBundle(view)) = self.local_surface_mut() {
                changed |= view.state.complete(completion);
            }
        }
        changed
    }
}

pub(crate) fn render_diagnostic_bundle_at(
    frame: &mut Frame<'_>,
    view: &DiagnosticBundleTargetView,
    theme: TuiTheme,
    area: Rect,
) {
    let inner = Modal::new(theme, IconId::Settings, t("diagnostics.title")).render(frame, area);
    let hint = match &view.state {
        DiagnosticBundleUiState::Preview(_) => format!(
            "Enter {} · Esc {}",
            t("diagnostics.export"),
            t("common.cancel")
        ),
        DiagnosticBundleUiState::Failed(_) => {
            format!("Enter {} · Esc {}", t("first_run.retry"), t("common.close"))
        }
        DiagnosticBundleUiState::Writing(_) | DiagnosticBundleUiState::Complete(_) => {
            format!("Esc {}", t("common.close"))
        }
    };
    let footer = Paragraph::new(hint).wrap(Wrap { trim: true });
    let footer_height = u16::try_from(footer.line_count(inner.width).saturating_add(1))
        .unwrap_or(u16::MAX)
        .min(inner.height);
    let [body, actions] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(footer_height)]).areas(inner);
    let text = match &view.state {
        DiagnosticBundleUiState::Preview(plan) => diagnostic_preview_text(plan.preview()),
        DiagnosticBundleUiState::Writing(preview) => format!(
            "{}\n{}",
            t("diagnostics.writing"),
            diagnostic_preview_text(preview)
        ),
        DiagnosticBundleUiState::Complete(path) => {
            t("diagnostics.complete").replace("{path}", &path.display().to_string())
        }
        DiagnosticBundleUiState::Failed(error) => diagnostic_failure_message(error),
    };
    let lines: Vec<_> = text.lines().map(ratatui::text::Line::from).collect();
    let (scroll, _) = clamped_scroll(
        wrapped_content_height(&lines, body.width),
        body.height.saturating_sub(1),
        view.scroll,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        body,
    );
    frame.render_widget(footer, actions);
}

#[cfg(test)]
#[path = "../tests/headless/diagnostic_bundle_tests.rs"]
mod tests;
