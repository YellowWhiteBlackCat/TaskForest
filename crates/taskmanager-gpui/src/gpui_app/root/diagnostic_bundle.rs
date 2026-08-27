//! Privacy review dialog and background diagnostic-bundle write state.

use gpui::{
    AnyElement, App, Context, Div, Entity, IntoElement, ParentElement, ScrollHandle, Styled,
    Window, div, px,
};
use std::path::PathBuf;
use taskmanager_app_host::DiagnosticBundleClient;
use taskmanager_application::{DiagnosticBundleSession, DiagnosticBundleTarget};

use crate::core::diagnostics::{
    DiagnosticBundleError, DiagnosticBundleErrorKind, DiagnosticBundlePlan, DiagnosticPreview,
    DiagnosticSource,
};
use crate::core::export::snapshot_to_json;
use crate::gpui_app::elements;
use crate::gpui_app::theme::{Theme, mono_font_with_fallback, tokens};
use crate::i18n;
use taskmanager_ui::layout::{BoundedScrollRailSpec, bounded_scroll_region_with_rail};

use super::RootView;

#[derive(Debug, Clone)]
pub enum DiagnosticBundleUiState {
    Preview(DiagnosticBundlePlan),
    Writing(DiagnosticPreview),
    Complete(PathBuf),
    Failed(DiagnosticBundleError),
}

#[derive(Debug, Default)]
pub(crate) enum DiagnosticBundleRuntime {
    #[default]
    Unavailable,
    Active(DiagnosticBundleSession<DiagnosticBundleClient>),
}

impl DiagnosticBundleRuntime {
    pub(crate) fn install(&mut self, client: DiagnosticBundleClient) {
        *self = Self::Active(DiagnosticBundleSession::new(client));
    }

    fn active_mut(&mut self) -> Option<&mut DiagnosticBundleSession<DiagnosticBundleClient>> {
        match self {
            Self::Unavailable => None,
            Self::Active(session) => Some(session),
        }
    }
}

impl RootView {
    /// Build an immutable sanitized plan. The preview never stores raw sources.
    pub fn open_diagnostic_preview(&mut self) {
        let snapshot = self.system_snapshot();
        let processes = self.processes();
        let sources = vec![
            DiagnosticSource {
                name: "snapshot.json".into(),
                contents: snapshot_to_json(snapshot, processes),
            },
            DiagnosticSource {
                name: "services.json".into(),
                contents: serde_json::to_string_pretty(self.services())
                    .unwrap_or_else(|error| format!("serialization error: {error}")),
            },
            DiagnosticSource {
                name: "startup.json".into(),
                contents: serde_json::to_string_pretty(self.startup_entries())
                    .unwrap_or_else(|error| format!("serialization error: {error}")),
            },
        ];
        let usernames = processes
            .iter()
            .map(|process| process.current_user().unwrap_or_default());
        let state = match DiagnosticBundlePlan::prepare(sources, usernames) {
            Ok(plan) => DiagnosticBundleUiState::Preview(plan),
            Err(error) => diagnostic_failure_state(error),
        };
        self.open_window_surface(super::window_surface::WindowSurface::DiagnosticBundle(
            state,
        ));
    }

    pub fn close_diagnostic_bundle(&mut self) {
        if let Some(session) = self.diagnostic_bundle_runtime.active_mut() {
            session.close();
        }
        self.dismiss_window_surface(
            super::WindowSurfaceKind::DiagnosticBundle,
            super::WindowSurfaceDismissReason::Cancel,
        );
    }

    fn confirm_diagnostic_bundle(&mut self) {
        let Some(DiagnosticBundleUiState::Preview(plan)) = self.diagnostic_bundle_state() else {
            return;
        };
        let plan = plan.clone();
        let preview = plan.preview().clone();
        let file_name = format!(
            "taskmanager-diagnostics-{}.json",
            self.system_snapshot().timestamp_ms
        );
        let Some(session) = self.diagnostic_bundle_runtime.active_mut() else {
            if let Some(state) = self.diagnostic_bundle_state_mut() {
                *state = diagnostic_failure_state(DiagnosticBundleError::new(
                    DiagnosticBundleErrorKind::Unavailable,
                ));
            }
            return;
        };
        match session.submit(plan, DiagnosticBundleTarget::current_directory(file_name)) {
            Ok(_) => {
                if let Some(state) = self.diagnostic_bundle_state_mut() {
                    *state = DiagnosticBundleUiState::Writing(preview);
                }
            }
            Err(error) => {
                if let Some(state) = self.diagnostic_bundle_state_mut() {
                    *state = diagnostic_failure_state(error);
                }
            }
        }
    }

    pub(crate) fn poll_diagnostic_bundle_result(&mut self) {
        let result = self
            .diagnostic_bundle_runtime
            .active_mut()
            .and_then(|session| session.drain().into_iter().next());
        if let Some(result) = result {
            let next = match result.result {
                Ok(()) => DiagnosticBundleUiState::Complete(result.destination),
                Err(error) => diagnostic_failure_state(error),
            };
            // A completion belongs only to the still-visible diagnostic
            // workflow. If the user replaced or dismissed it, the stale worker
            // result must not steal the window's current input owner.
            if let Some(state) = self.diagnostic_bundle_state_mut() {
                *state = next;
            }
        }
    }

    pub(crate) fn show_diagnostic_bundle_state(&mut self, state: DiagnosticBundleUiState) {
        self.open_window_surface(super::window_surface::WindowSurface::DiagnosticBundle(
            state,
        ));
    }
}

fn diagnostic_failure_state(error: DiagnosticBundleError) -> DiagnosticBundleUiState {
    tracing::warn!(
        failure_kind = error.kind().stable_code(),
        detail = error.detail().unwrap_or(""),
        "diagnostic bundle operation failed"
    );
    DiagnosticBundleUiState::Failed(error)
}

const fn diagnostic_failure_feedback_key(kind: DiagnosticBundleErrorKind) -> &'static str {
    match kind {
        DiagnosticBundleErrorKind::InvalidSource => "diagnostics.failure_invalid_source",
        DiagnosticBundleErrorKind::Encode => "diagnostics.failure_encode",
        DiagnosticBundleErrorKind::Io => "diagnostics.failure_io",
        DiagnosticBundleErrorKind::Busy => "diagnostics.failure_busy",
        DiagnosticBundleErrorKind::Unavailable => "diagnostics.failure_unavailable",
    }
}

fn diagnostic_failure_message(error: &DiagnosticBundleError) -> String {
    i18n::t("diagnostics.failed_detail").replace(
        "{reason}",
        i18n::t(diagnostic_failure_feedback_key(error.kind())),
    )
}

fn preview_panel(theme: &Theme, preview: &DiagnosticPreview, scroll: ScrollHandle) -> Div {
    let summary = i18n::t("diagnostics.redaction_summary")
        .replace("{total}", &preview.redactions.total().to_string())
        .replace("{users}", &preview.redactions.usernames.to_string())
        .replace("{paths}", &preview.redactions.paths.to_string())
        .replace(
            "{ips}",
            &(preview.redactions.ipv4_addresses + preview.redactions.ipv6_addresses).to_string(),
        );
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg)
                .child(summary),
        )
        .child(bounded_scroll_region_with_rail(
            BoundedScrollRailSpec {
                id: "diagnostic-preview-scroll",
                viewport_selector: "tm-diagnostic-preview-scroll",
                scrollbar_id: "diagnostic-preview-scrollbar",
                scrollbar_selector: "tm-diagnostic-preview-scrollbar",
                track_selector: "tm-diagnostic-preview-scrollbar-track",
                width: None,
                max_height: px(230.0),
                scroll,
                palette: theme.palette(),
            },
            div()
                .flex()
                .flex_col()
                .gap(tokens::SPACE_8)
                .children(preview.files.iter().map(|file| {
                    div()
                        .p(tokens::SPACE_8)
                        .rounded(tokens::control_radius(theme))
                        .bg(theme.sidebar_card_bg)
                        .child(
                            div()
                                .text_size(tokens::FONT_12)
                                .font_weight(tokens::FONT_WEIGHT_SEMIBOLD.into())
                                .child(format!("{} · {} B", file.name, file.bytes)),
                        )
                        .child(
                            div()
                                .mt(tokens::SPACE_4)
                                .text_size(tokens::FONT_10)
                                .font(mono_font_with_fallback(theme))
                                .text_color(theme.fg_dim)
                                .whitespace_normal()
                                .child(file.excerpt.clone()),
                        )
                })),
        ))
}

pub(super) fn render_diagnostic_bundle_dialog(
    theme: &Theme,
    state: DiagnosticBundleUiState,
    entity: Entity<RootView>,
    scroll: ScrollHandle,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let close = entity.clone();
    let on_close = move |_window: &mut Window, cx: &mut App| {
        close.update(cx, |view, cx| {
            view.close_diagnostic_bundle();
            cx.notify();
        });
    };
    let body = match &state {
        DiagnosticBundleUiState::Preview(plan) => preview_panel(theme, plan.preview(), scroll),
        DiagnosticBundleUiState::Writing(preview) => preview_panel(theme, preview, scroll).child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(i18n::t("diagnostics.writing")),
        ),
        DiagnosticBundleUiState::Complete(path) => div()
            .text_size(tokens::FONT_12)
            .text_color(theme.disk)
            .child(i18n::t("diagnostics.complete").replace("{path}", &path.display().to_string())),
        DiagnosticBundleUiState::Failed(error) => div()
            .text_size(tokens::FONT_12)
            .text_color(theme.gpu)
            .child(diagnostic_failure_message(error)),
    };
    let dialog_width = (f32::from(window.viewport_size().width) - 80.0).clamp(320.0, 580.0);
    let content_width = (dialog_width - 50.0).max(270.0);
    let mut content = div()
        .w(px(content_width))
        .flex()
        .flex_col()
        .gap(tokens::SPACE_12)
        .child(body);
    let close_button = entity.clone();
    let actions = div()
        .flex()
        .justify_end()
        .gap(tokens::SPACE_8)
        .child(elements::pill(
            theme,
            "diagnostic-close",
            if matches!(state, DiagnosticBundleUiState::Preview(_)) {
                i18n::t("common.cancel")
            } else {
                i18n::t("common.close")
            },
            false,
            false,
            move |_window, cx| {
                close_button.update(cx, |view, cx| {
                    view.close_diagnostic_bundle();
                    cx.notify();
                });
            },
            |_, _, _| {},
        ));
    let actions = if matches!(state, DiagnosticBundleUiState::Preview(_)) {
        let confirm = entity;
        actions.child(elements::pill(
            theme,
            "diagnostic-confirm",
            i18n::t("diagnostics.export"),
            true,
            false,
            move |_window, cx| {
                confirm.update(cx, |view, cx| {
                    view.confirm_diagnostic_bundle();
                    cx.notify();
                });
            },
            |_, _, _| {},
        ))
    } else {
        actions
    };
    content = content.child(actions);
    elements::dialog_overlay_width(
        theme,
        window,
        cx,
        px(dialog_width),
        i18n::t("diagnostics.title"),
        on_close,
        content,
    )
    .into_any_element()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_diagnostic_bundle_tests.rs"]
mod tests;
