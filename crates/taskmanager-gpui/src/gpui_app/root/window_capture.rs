//! GPUI adapter for the application-correlated current-window PNG session.

#[cfg(target_os = "linux")]
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use taskmanager_app_host::WindowCaptureClient;
use taskmanager_application::window_capture::{WindowCaptureSession, WindowCaptureState};
#[cfg(target_os = "linux")]
use taskmanager_application::window_capture::{WindowCaptureSubmitError, WindowCaptureTarget};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};
#[cfg(target_os = "linux")]
use taskmanager_ui_contract::IconId;
use tracing::{info, warn};

#[cfg(target_os = "linux")]
use crate::gpui_app::elements;

#[cfg(target_os = "linux")]
use super::Hover;
use super::RootView;

#[derive(Debug, Default)]
pub(crate) enum WindowCaptureRuntime {
    #[default]
    Unavailable,
    Active(WindowCaptureSession<WindowCaptureClient>),
}

impl WindowCaptureRuntime {
    pub(crate) fn install(&mut self, client: WindowCaptureClient) {
        *self = Self::Active(WindowCaptureSession::new(client));
    }

    fn active_mut(&mut self) -> Option<&mut WindowCaptureSession<WindowCaptureClient>> {
        match self {
            Self::Unavailable => None,
            Self::Active(session) => Some(session),
        }
    }
}

/// Linux/Wayland one-shot capture affordance. The click only submits a typed
/// application request; the app-host owns the native provider and PNG commit.
#[cfg(target_os = "linux")]
pub(crate) fn current_window_capture_btn(
    t: &taskmanager_theme::Theme,
    hovered: Option<&Hover>,
    icon_only: bool,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let is_hov = hovered == Some(&Hover::Static("window-capture-btn"));
    let click_entity = cx.entity();
    let hover_entity = click_entity.clone();
    let on_click = move |_window: &mut gpui::Window, cx: &mut gpui::App| {
        click_entity.update(cx, |view, cx| {
            view.request_current_window_capture();
            cx.notify();
        });
    };
    let on_hover = move |is_hov: &bool, _window: &mut gpui::Window, cx: &mut gpui::App| {
        hover_entity.update(cx, |view, cx| {
            view.set_hover(
                if *is_hov {
                    Some(Hover::Static("window-capture-btn"))
                } else {
                    None
                },
                cx,
            );
        });
    };

    if icon_only {
        // A compact rail has only 54px including its border and padding. The
        // labeled Pill is replaced by the same 16px icon target used by the
        // other rail controls; its stable hover identity resolves the
        // localized tooltip in root/chrome.rs.
        return div()
            .id("window-capture-btn")
            .debug_selector(|| "window-capture-btn".to_string())
            .on_click(move |_ev, window, cx| on_click(window, cx))
            .on_hover(move |is_hov, window, cx| on_hover(is_hov, window, cx))
            .focusable()
            .tab_stop(true)
            .focus(elements::focus_ring(t))
            .px(taskmanager_ui::theme_binding::definite_length(
                taskmanager_theme::tokens::SPACE_8,
            ))
            .py(taskmanager_ui::theme_binding::definite_length(
                taskmanager_theme::tokens::SPACE_6,
            ))
            .rounded(taskmanager_ui::theme_binding::absolute(
                taskmanager_theme::tokens::control_radius(t),
            ))
            .bg(taskmanager_ui::theme_binding::fill(if is_hov {
                t.accent.with_alpha(0.12)
            } else {
                taskmanager_theme::Color::TRANSPARENT
            }))
            .flex()
            .items_center()
            .justify_center()
            .child(taskmanager_ui::icons_binding::icon(IconId::Export).size(px(16.0)))
            .into_any_element();
    }

    elements::Pill::new(
        "window-capture-btn",
        taskmanager_application::i18n::t("window_capture.capture"),
        on_click,
        on_hover,
    )
    .semantic_icon(IconId::Export)
    .hovered(is_hov)
    .render(t)
    .into_any_element()
}

impl RootView {
    #[cfg(target_os = "linux")]
    pub(crate) fn request_current_window_capture(&mut self) -> bool {
        let target = self
            .capture_evidence
            .window_capture_output()
            .map(WindowCaptureTarget::path)
            .unwrap_or_else(|| WindowCaptureTarget::current_directory("taskforest-window.png"));
        let Some(session) = self.window_capture.active_mut() else {
            self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                taskmanager_application::i18n::t("window_capture.unavailable"),
            );
            return false;
        };
        match session.submit(target) {
            Ok(request) => {
                info!(
                    target: "taskmanager.window_capture",
                    request = request.get(),
                    "current-window PNG capture queued"
                );
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::UntilReplaced,
                    taskmanager_application::i18n::t("window_capture.queued"),
                );
                true
            }
            Err(WindowCaptureSubmitError::Busy(_)) => {
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Warning,
                    FeedbackLifecycle::SHORT,
                    taskmanager_application::i18n::t("window_capture.busy"),
                );
                false
            }
            Err(WindowCaptureSubmitError::RequestSpaceExhausted) => {
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    taskmanager_application::i18n::t("window_capture.unavailable"),
                );
                false
            }
            Err(WindowCaptureSubmitError::Rejected(error)) => {
                warn!(
                    target: "taskmanager.window_capture",
                    kind = error.kind().code(),
                    detail = error.detail(),
                    "current-window PNG capture rejected"
                );
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    taskmanager_application::i18n::t("window_capture.failed")
                        .replace("{}", error.detail()),
                );
                false
            }
        }
    }

    pub(crate) fn drain_window_capture_completions(&mut self) -> bool {
        let Some(session) = self.window_capture.active_mut() else {
            return false;
        };
        if session.drain() == 0 {
            return false;
        }
        let state = session.state().clone();
        match state {
            WindowCaptureState::Ready {
                request,
                destination,
                width,
                height,
                backend,
            } => {
                info!(
                    target: "taskmanager.window_capture",
                    request = request.get(),
                    destination = destination.as_ref(),
                    width,
                    height,
                    backend = backend.code(),
                    "current-window PNG capture completed"
                );
                let message = taskmanager_application::i18n::t("window_capture.success")
                    .replacen("{}", destination.as_ref(), 1)
                    .replacen("{}", &width.to_string(), 1)
                    .replacen("{}", &height.to_string(), 1);
                self.capture_evidence.mark_window_capture_ready();
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    message,
                );
            }
            WindowCaptureState::Failed { request, error } => {
                warn!(
                    target: "taskmanager.window_capture",
                    request = request.get(),
                    kind = error.kind().code(),
                    detail = error.detail(),
                    "current-window PNG capture failed"
                );
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    taskmanager_application::i18n::t("window_capture.failed")
                        .replace("{}", error.detail()),
                );
            }
            WindowCaptureState::Closed
            | WindowCaptureState::Queued(_)
            | WindowCaptureState::Running(_) => return false,
        }
        true
    }
}
