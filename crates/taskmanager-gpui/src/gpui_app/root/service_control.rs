//! Shared confirmation flow for destructive service / startup control.
//!
//! The process-termination gate (`root/termination.rs`) captures an immutable
//! identity at request time and renders a typed confirmation dialog whose
//! Cancel/X/scrim/Escape paths only clear pending state — the confirm button is
//! the sole submit path. This module mirrors that pattern for the actions that
//! can lock the user out of the session (stopping or disabling NetworkManager,
//! dbus, sshd, pipewire, …) and for startup enable/disable toggles, which
//! previously executed immediately on click.
//!
//! `RootView` remains the single owner of the pending UI state
//! ([`RootView::service_control_confirmation`]); this module owns the intent
//! type, the confirm/cancel methods, and the dialog renderer.

use crate::gpui_app::elements;
use gpui::{
    AnyElement, App, Context, Entity, IntoElement, ParentElement, Styled, Window, div, px, relative,
};
use taskmanager_application::i18n;
use taskmanager_application::{
    ConfirmationKind, PendingConfirmation, ServiceControlTarget, StartupControlRequest,
    SurfaceDismissReason, SurfaceKind,
};
use taskmanager_core::core::services::ServiceAction;
use taskmanager_core::core::startup::StartupEntry;
use taskmanager_core::core::target::ServiceId;
use taskmanager_theme::Theme;

use super::RootView;
use taskmanager_theme::tokens;

/// Pending service / startup control intent awaiting explicit confirmation.
///
/// Captured at request time (not re-derived at confirm time) so a live list
/// refresh can never silently change what the confirmation represents — the same
/// freeze-at-request discipline of the shared confirmation payloads applies to
/// process identities, service ids, and startup entries alike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ServiceControlConfirmation {
    /// A gated service lifecycle action: Stop, Restart, or Disable. Start and
    /// Enable stay immediate (constructive, low blast radius). The display name
    /// is frozen alongside the provider-issued id so the dialog never shows a
    /// stale or reused row's label.
    Service {
        service_id: ServiceId,
        display_name: String,
        action: ServiceAction,
    },
    /// A startup-entry enable / disable toggle. Both directions route through
    /// the gate because either changes boot-time configuration. The immutable
    /// entry snapshot keeps the source-specific identity out of the frontend.
    Startup { entry: StartupEntry, enabled: bool },
}

impl ServiceControlConfirmation {
    /// High-blast-radius actions render in the destructive accent so the dialog
    /// reads as a warning, matching the process confirmation renderer.
    fn is_high_risk(&self) -> bool {
        matches!(
            self,
            Self::Service {
                action: ServiceAction::Stop | ServiceAction::Restart,
                ..
            }
        )
    }

    fn dialog_title(&self) -> &'static str {
        match self {
            Self::Service {
                action: ServiceAction::Stop,
                ..
            } => i18n::t("svc.confirm_stop_title"),
            Self::Service {
                action: ServiceAction::Restart,
                ..
            } => i18n::t("svc.confirm_restart_title"),
            Self::Service {
                action: ServiceAction::Disable,
                ..
            } => i18n::t("svc.confirm_disable_title"),
            // Service Start/Enable are never gated; Startup uses its own title.
            Self::Service { .. } => i18n::t("svc.confirm_stop_title"),
            Self::Startup { .. } => i18n::t("startup.confirm_toggle_title"),
        }
    }

    fn dialog_message(&self) -> String {
        match self {
            Self::Service {
                display_name,
                action,
                ..
            } => {
                let template = match action {
                    ServiceAction::Stop => i18n::t("svc.confirm_stop_message"),
                    ServiceAction::Restart => i18n::t("svc.confirm_restart_message"),
                    ServiceAction::Disable => i18n::t("svc.confirm_disable_message"),
                    // Unreachable for gated actions; keep a sane fallback.
                    ServiceAction::Start | ServiceAction::Enable => {
                        i18n::t("svc.confirm_stop_message")
                    }
                };
                template.replace("{name}", display_name)
            }
            Self::Startup { entry, enabled } => {
                let template = if *enabled {
                    i18n::t("startup.confirm_enable_message")
                } else {
                    i18n::t("startup.confirm_disable_message")
                };
                template.replace("{name}", &entry.name)
            }
        }
    }

    fn confirm_label(&self) -> &'static str {
        match self {
            Self::Service { action, .. } => match action {
                ServiceAction::Stop => i18n::t("svc.stop"),
                ServiceAction::Restart => i18n::t("svc.restart"),
                ServiceAction::Disable => i18n::t("common.disable"),
                ServiceAction::Start => i18n::t("svc.start"),
                ServiceAction::Enable => i18n::t("common.enable"),
            },
            Self::Startup { enabled, .. } => {
                if *enabled {
                    i18n::t("common.enable")
                } else {
                    i18n::t("common.disable")
                }
            }
        }
    }
}

/// The service lifecycle actions that demand a confirmation dialog. Deliberately
/// a strict subset of [`ServiceAction`]: Start and Enable are constructive and
/// stay immediate, while Stop / Restart / Disable can lock the session.
pub(crate) fn requires_service_confirmation(action: ServiceAction) -> bool {
    matches!(
        action,
        ServiceAction::Stop | ServiceAction::Restart | ServiceAction::Disable
    )
}

impl RootView {
    /// Open the shared service/startup confirmation dialog for a gated service
    /// action (Stop / Restart / Disable). Constructive Start / Enable ignore the
    /// gate and submit immediately. Merely setting this state performs no native
    /// work; confirmation consumes it, Cancel/Escape simply clear it.
    pub fn request_service_control_confirmation(
        &mut self,
        service_id: ServiceId,
        action: ServiceAction,
    ) {
        if !requires_service_confirmation(action) {
            self.request_service_action(service_id, action);
            return;
        }
        self.arm_confirmation(PendingConfirmation::ServiceControl(ServiceControlTarget {
            service_id,
            action,
        }));
    }

    /// Open the shared confirmation dialog for a startup enable / disable
    /// toggle. The entry snapshot is frozen so the dialog cannot drift to a
    /// different row during confirmation.
    pub fn request_startup_control_confirmation(&mut self, entry: StartupEntry, enabled: bool) {
        let request_id = self.shell.begin_startup_control();
        self.arm_confirmation(PendingConfirmation::StartupControl(StartupControlRequest {
            request_id,
            entry,
            enabled,
        }));
    }

    /// Dismiss the pending confirmation without submitting any native work.
    /// Cancel, X, scrim, and Escape all funnel through here.
    pub fn cancel_service_control_confirmation(&mut self) {
        let Some(kind) = self.shell.interaction.confirmation_kind() else {
            return;
        };
        if matches!(
            kind,
            ConfirmationKind::ServiceControl | ConfirmationKind::StartupControl
        ) {
            self.dismiss_shared_surface(
                SurfaceKind::Confirmation(kind),
                SurfaceDismissReason::Cancel,
            );
        }
    }

    /// Consume and execute the pending intent via the existing platform-neutral
    /// submit paths (`RootView::request_service_action` /
    /// `RootView::request_startup_enabled`). Returns false when nothing was
    /// pending. This is the sole submit path; Cancel/Escape never reach it.
    pub fn confirm_service_control_confirmation(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(kind) = self.shell.interaction.confirmation_kind() else {
            return false;
        };
        if !matches!(
            kind,
            ConfirmationKind::ServiceControl | ConfirmationKind::StartupControl
        ) {
            return false;
        }
        self.confirm_confirmation(kind)
            .is_some_and(|effect| self.dispatch_confirmed_effect(effect, cx))
    }
}

pub(super) fn confirmation_dialog(view: &RootView) -> Option<ServiceControlConfirmation> {
    match view.pending_confirmation()? {
        PendingConfirmation::ServiceControl(target) => {
            let display_name = view
                .services()
                .iter()
                .find(|service| service.id == target.service_id)
                .map(|service| service.name.clone())
                .unwrap_or_else(|| target.service_id.as_str().to_string());
            Some(ServiceControlConfirmation::Service {
                service_id: target.service_id.clone(),
                display_name,
                action: target.action,
            })
        }
        PendingConfirmation::StartupControl(request) => Some(ServiceControlConfirmation::Startup {
            entry: request.entry.clone(),
            enabled: request.enabled,
        }),
        PendingConfirmation::EndTask(_)
        | PendingConfirmation::ProcessBatch(_)
        | PendingConfirmation::SessionControl(_)
        | PendingConfirmation::SmartSelfTest(_) => None,
    }
}

/// Build the complete service/startup control confirmation dialog. Sibling of
/// the process confirmation renderer. Closing via X / scrim and the Cancel
/// button only clear pending state; the confirm button is the sole UI path to
/// [`RootView::confirm_service_control_confirmation`].
pub(super) fn render_service_control_confirmation_dialog(
    theme: &Theme,
    intent: ServiceControlConfirmation,
    entity: Entity<RootView>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let title = intent.dialog_title();
    let message = intent.dialog_message();
    let confirm_label = intent.confirm_label();
    let is_high_risk = intent.is_high_risk();

    let close_entity = entity.clone();
    let on_close = move |_window: &mut Window, cx: &mut App| {
        close_entity.update(cx, |view, cx| {
            view.cancel_service_control_confirmation();
            cx.notify();
        });
    };
    let cancel_entity = entity.clone();
    let confirm_entity = entity;
    let content: AnyElement = div()
        .w(px(420.0))
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_14,
        ))
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                .line_height(relative(1.45))
                .text_color(taskmanager_ui::theme_binding::hsla(if is_high_risk {
                    theme.danger
                } else {
                    theme.fg
                }))
                .child(message),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .child(elements::pill(
                    theme,
                    "service-control-cancel",
                    i18n::t("common.cancel"),
                    false,
                    false,
                    move |_window, cx| {
                        cancel_entity.update(cx, |view, cx| {
                            view.cancel_service_control_confirmation();
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                ))
                .child(elements::pill(
                    theme,
                    "service-control-confirm",
                    confirm_label,
                    true,
                    false,
                    move |_window, cx| {
                        confirm_entity.update(cx, |view, cx| {
                            view.confirm_service_control_confirmation(cx);
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                )),
        )
        .into_any_element();

    elements::dialog_overlay(theme, window, cx, title, on_close, content).into_any_element()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_service_control_tests.rs"]
mod tests;
