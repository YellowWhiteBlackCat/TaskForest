//! Privacy review dialog and background diagnostic-bundle write state.

use gpui::{
    AnyElement, App, Context, Div, Entity, IntoElement, ParentElement, ScrollHandle, Styled,
    Window, div, px,
};
use taskmanager_app_host::DiagnosticBundleClient;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::{DiagnosticBundleSession, DiagnosticBundleTarget};
use taskmanager_ui::theme_binding::absolute;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::fill;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::font_weight;
use taskmanager_ui::theme_binding::hsla;
use taskmanager_ui::theme_binding::length;

use crate::gpui_app::elements;
use crate::gpui_app::theme::mono_font_with_fallback;
use taskmanager_application::i18n;
use taskmanager_core::core::diagnostics::DiagnosticPreview;
use taskmanager_shell::presentation::diagnostics::{
    diagnostic_failure_message, diagnostic_redaction_summary,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::layout::{BoundedScrollRailSpec, bounded_scroll_region_with_rail};

use super::RootView;

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
        if let Some(session) = self.diagnostic_bundle_runtime.active_mut() {
            session.close();
        }
        let state = DiagnosticBundleUiState::prepared(
            self.projection()
                .prepare_diagnostic_bundle(Some(&super::persistence::config_from_view(self))),
        );
        self.open_window_surface(super::window_surface::WindowSurface::DiagnosticBundle(
            state,
        ));
    }

    pub(super) fn close_diagnostic_session(&mut self) {
        if let Some(session) = self.diagnostic_bundle_runtime.active_mut() {
            session.close();
        }
    }

    pub fn close_diagnostic_bundle(&mut self) {
        self.close_diagnostic_session();
        self.dismiss_window_surface(
            super::WindowSurfaceKind::DiagnosticBundle,
            super::WindowSurfaceDismissReason::Cancel,
        );
    }

    fn confirm_diagnostic_bundle(&mut self) {
        let target = DiagnosticBundleTarget::current_directory(format!(
            "taskmanager-diagnostics-{}.json",
            self.system_snapshot().timestamp_ms
        ));
        let mut state = self.diagnostic_bundle_state().cloned();
        if let Some(state) = state.as_mut() {
            state.confirm(self.diagnostic_bundle_runtime.active_mut(), target);
        }
        if let (Some(current), Some(state)) = (self.diagnostic_bundle_state_mut(), state) {
            *current = state;
        }
    }

    pub(crate) fn poll_diagnostic_bundle_result(&mut self) {
        let result = self
            .diagnostic_bundle_runtime
            .active_mut()
            .and_then(|session| session.drain().into_iter().next());
        if let Some(result) = result
            && let Some(state) = self.diagnostic_bundle_state_mut()
        {
            state.complete(result);
        }
    }

    pub(crate) fn show_diagnostic_bundle_state(&mut self, state: DiagnosticBundleUiState) {
        self.open_window_surface(super::window_surface::WindowSurface::DiagnosticBundle(
            state,
        ));
    }
}

fn preview_panel(theme: &Theme, preview: &DiagnosticPreview, scroll: ScrollHandle) -> Div {
    let summary = diagnostic_redaction_summary(preview);
    div()
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_8))
        .child(
            div()
                .text_size(font_size(tokens::FONT_12))
                .text_color(hsla(theme.fg))
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
                .gap(definite_length(tokens::SPACE_8))
                .children(preview.files.iter().map(|file| {
                    div()
                        .p(definite_length(tokens::SPACE_8))
                        .rounded(absolute(tokens::control_radius(theme)))
                        .bg(fill(theme.sidebar_card_bg))
                        .child(
                            div()
                                .text_size(font_size(tokens::FONT_12))
                                .font_weight(font_weight(tokens::FONT_WEIGHT_SEMIBOLD))
                                .child(format!("{} · {} B", file.name, file.bytes)),
                        )
                        .child(
                            div()
                                .mt(length(tokens::SPACE_4))
                                .text_size(font_size(tokens::FONT_10))
                                .font(mono_font_with_fallback(theme))
                                .text_color(hsla(theme.fg_dim))
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
                .text_size(font_size(tokens::FONT_12))
                .text_color(hsla(theme.fg_dim))
                .child(i18n::t("diagnostics.writing")),
        ),
        DiagnosticBundleUiState::Complete(path) => div()
            .text_size(font_size(tokens::FONT_12))
            .text_color(hsla(theme.disk))
            .child(i18n::t("diagnostics.complete").replace("{path}", &path.display().to_string())),
        DiagnosticBundleUiState::Failed(error) => div()
            .text_size(font_size(tokens::FONT_12))
            .text_color(hsla(theme.gpu))
            .child(diagnostic_failure_message(error)),
    };
    let dialog_width = (f32::from(window.viewport_size().width) - 80.0).clamp(320.0, 580.0);
    let content_width = (dialog_width - 50.0).max(270.0);
    let mut content = div()
        .w(px(content_width))
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_12))
        .child(body);
    let close_button = entity.clone();
    let actions = div()
        .flex()
        .justify_end()
        .gap(definite_length(tokens::SPACE_8))
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
    } else if matches!(state, DiagnosticBundleUiState::Failed(_)) {
        let retry = entity;
        actions.child(elements::pill(
            theme,
            "diagnostic-retry",
            i18n::t("first_run.retry"),
            true,
            false,
            move |_window, cx| {
                retry.update(cx, |view, cx| {
                    if matches!(
                        view.diagnostic_bundle_state(),
                        Some(DiagnosticBundleUiState::Failed(_))
                    ) {
                        view.open_diagnostic_preview();
                        cx.notify();
                    }
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
