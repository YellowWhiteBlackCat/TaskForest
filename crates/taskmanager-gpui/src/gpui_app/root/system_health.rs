//! Root-owned state, confirmation boundary, and dialog for System > Health.

use crate::gpui_app::elements;
use crate::gpui_app::system_health_view::{
    SmartSelfTestConfirmationRequest, SystemHealthText, localized_text,
};
use gpui::{
    AnyElement, App, Context, Entity, IntoElement, ParentElement, Styled, Window, div, px, relative,
};
use taskmanager_application::i18n;
use taskmanager_application::{
    ConfirmationKind, PendingConfirmation, SmartControlRequest, SmartObservationProjection,
    SurfaceDismissReason, SurfaceKind,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_core::core::{DeviceGeneration, SmartSelfTestReport};
use taskmanager_theme::Theme;

use super::RootView;
use taskmanager_theme::tokens;

impl RootView {
    /// Store a typed confirmation request only. No platform request is queued.
    pub fn request_system_health_self_test_confirmation(
        &mut self,
        request: SmartSelfTestConfirmationRequest,
    ) {
        self.arm_confirmation(PendingConfirmation::SmartSelfTest(SmartSelfTestIntent {
            device_id: request.device_id,
            device_generation: request.device_generation,
            device_key: request.disk_name.into(),
            display_name: request.disk_label,
            kind: request.kind,
        }));
    }

    /// Cancel, close-X, scrim, and Escape never submit a platform operation.
    pub fn cancel_system_health_self_test_confirmation(&mut self) {
        self.dismiss_shared_surface(
            SurfaceKind::Confirmation(ConfirmationKind::SmartSelfTest),
            SurfaceDismissReason::Cancel,
        );
        self.shell.close_smart_self_test();
    }

    /// The sole path that can submit the pending SMART self-test.
    pub fn confirm_system_health_self_test(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(effect) = self.confirm_confirmation(ConfirmationKind::SmartSelfTest) else {
            self.shell.report_notice(
                taskmanager_shell::FeedbackSource::Interaction,
                taskmanager_shell::FeedbackSeverity::Warning,
                taskmanager_shell::FeedbackLifecycle::SHORT,
                i18n::t("health.no_pending"),
            );
            return false;
        };
        self.dispatch_confirmed_effect(effect, cx)
    }

    pub(crate) fn submit_smart_self_test_intent(&mut self, intent: SmartSelfTestIntent) -> bool {
        let attempt = self.shell.begin_smart_self_test(intent.clone());
        let result = self.platform.as_mut().map_or_else(
            || Err(FailureKind::TemporarilyUnavailable),
            |platform| {
                platform
                    .submit_smart_control(
                        SmartControlRequest::StartSelfTest(intent.clone()),
                        super::platform_submission_time_ms(),
                    )
                    .map_err(super::process_control::submission_failure_kind)
            },
        );
        match result {
            Ok(request_id) => self.shell.accept_smart_self_test(attempt, request_id),
            Err(failure) => {
                self.shell.reject_smart_self_test(attempt, failure);
                // Submission rejection has no future terminal. Re-arm the
                // exact immutable intent so the same dialog renders Failed
                // and Confirm is an explicit retry transition.
                self.arm_confirmation(PendingConfirmation::SmartSelfTest(intent));
                false
            }
        }
    }
}

/// Borrow the exact generation-bound report selected by the visible disk.
/// The shell projection remains the sole production owner; GPUI never copies
/// its report and identity into independently writable optional fields.
pub(crate) fn smart_report_for_device<'a>(
    projection: &'a SmartObservationProjection,
    device_id: &str,
    generation: DeviceGeneration,
) -> Option<&'a SmartSelfTestReport> {
    projection
        .observations()
        .iter()
        .find(|observation| {
            observation.device_id.as_str() == device_id
                && observation.device_generation == generation
        })
        .map(|observation| &observation.report)
}

fn self_test_failure_text(error: FailureKind) -> &'static str {
    match error {
        FailureKind::TemporarilyUnavailable | FailureKind::TimedOut => {
            i18n::t("health.request_busy")
        }
        _ => i18n::t("health.worker_stopped"),
    }
}

pub(super) fn render_system_health_confirmation_dialog(
    theme: &Theme,
    request: SmartSelfTestIntent,
    error: Option<FailureKind>,
    entity: Entity<RootView>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let message = i18n::t("health.confirm_body")
        .replace(
            "{kind}",
            &localized_text(SystemHealthText::SmartKind(request.kind)),
        )
        .replace("{disk}", &request.display_name);
    let close = entity.clone();
    let on_close = move |_window: &mut Window, cx: &mut App| {
        close.update(cx, |view, cx| {
            view.cancel_system_health_self_test_confirmation();
            cx.notify();
        });
    };
    let cancel = entity.clone();
    let confirm = entity;
    let mut content = div()
        .w(px(420.0))
        .max_w(relative(1.0))
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(message),
        );
    if let Some(error) = error {
        content = content.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.danger))
                .child(self_test_failure_text(error)),
        );
    }
    let content = content
        .child(
            div()
                .flex()
                .justify_end()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .child(elements::pill(
                    theme,
                    "health-self-test-cancel",
                    i18n::t("common.cancel"),
                    false,
                    false,
                    move |_window, cx| {
                        cancel.update(cx, |view, cx| {
                            view.cancel_system_health_self_test_confirmation();
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                ))
                .child(elements::pill(
                    theme,
                    "health-self-test-confirm",
                    i18n::t("health.confirm"),
                    true,
                    false,
                    move |_window, cx| {
                        confirm.update(cx, |view, cx| {
                            view.confirm_system_health_self_test(cx);
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                )),
        )
        .into_any_element();
    elements::dialog_overlay(
        theme,
        window,
        cx,
        i18n::t("health.confirm_title"),
        on_close,
        content,
    )
    .into_any_element()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_system_health_tests.rs"]
mod tests;
