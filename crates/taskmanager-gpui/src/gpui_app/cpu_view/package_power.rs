//! Package-power section of the CPU details panel (the RAPL request lane).
//!
//! Per-package watts can only come from the privileged RAPL helper
//! (`telemetry.cpu.package_power`, ADR-023 permission-model Boundary 2). The
//! section is an honest projection of the application-owned request session:
//! every non-ready variant renders a typed placeholder or the typed failure
//! reason, never a fabricated zero-watt row.
//!
//! # Escalation discipline
//!
//! The OS-native prompt is strictly user-initiated: the ONLY entry is the
//! "Authorize package power" affordance rendered while the projection is
//! [`PackagePowerModel::AuthorizationRequired`]. One click submits exactly one
//! request (begin → platform submit → accept/reject, mirroring
//! `submit_gpu_engine_rows_refresh`); there is no auto-poll chain, and the
//! handler re-checks the projection so a stale click can never prompt.

use gpui::{
    AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div,
};
use taskmanager_application::{RaplPowerRequest, RaplPowerState, i18n, request_submission_failure};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::RaplPowerSnapshot;
use taskmanager_platform_contract::{CapabilityId, CapabilityStatus, SubmissionErrorKind};

use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::platform_submission_time_ms;
use taskmanager_theme::Color;
use taskmanager_theme::{Theme, tokens};

/// Render-entry inputs for the section: the shared session state plus the
/// runtime capability catalog entry for the lane.
pub(crate) struct PackagePowerInputs<'a> {
    pub state: &'a RaplPowerState,
    pub capability: Option<CapabilityStatus>,
}

/// Pure projection of the package-power lane for the details panel.
#[derive(Debug, PartialEq)]
pub(crate) enum PackagePowerModel {
    /// No live session and no registered lane on this host: the section
    /// renders nothing at all.
    Hidden,
    /// Real per-package rows `(label, watts)` from an accepted payload. A
    /// measured zero stays a row; only a *missing* reading is absent.
    Packages(Vec<(String, String)>),
    /// A request is in flight and no accepted payload exists yet.
    Measuring,
    /// The lane is escalation-backed: render the typed hint plus the
    /// authorize affordance. No number may render in this state.
    AuthorizationRequired,
    /// A typed failure; the value is the localized message key.
    Unavailable(&'static str),
}

#[must_use]
pub(crate) fn package_power_model(inputs: &PackagePowerInputs<'_>) -> PackagePowerModel {
    match inputs.state {
        RaplPowerState::Ready(ready) => PackagePowerModel::Packages(watt_rows(&ready.snapshot)),
        RaplPowerState::Loading {
            last_good: Some(ready),
            ..
        } => PackagePowerModel::Packages(watt_rows(&ready.snapshot)),
        RaplPowerState::Loading {
            last_good: None, ..
        } => PackagePowerModel::Measuring,
        RaplPowerState::Failed(failed) => model_from_failure(failure_kind(&failed.failure)),
        RaplPowerState::Closed => match inputs.capability {
            // The runtime catalog proves an escalation-backed lane exists:
            // offer the one explicit authorization entry.
            Some(CapabilityStatus::Available | CapabilityStatus::PermissionRequired) => {
                PackagePowerModel::AuthorizationRequired
            }
            Some(CapabilityStatus::MissingDependency) => {
                PackagePowerModel::Unavailable("cpu.package_power_helper")
            }
            Some(CapabilityStatus::Degraded(kind)) => model_from_failure(kind),
            Some(CapabilityStatus::Unsupported)
            | Some(CapabilityStatus::TemporarilyUnavailable)
            | Some(CapabilityStatus::Stale)
            | None => PackagePowerModel::Hidden,
        },
    }
}

/// One label/value row per real package reading, watts at one decimal — the
/// same spelling as the live CPU power readout (`{value:.1} W`).
fn watt_rows(snapshot: &RaplPowerSnapshot) -> Vec<(String, String)> {
    snapshot
        .packages
        .iter()
        .map(|row| (row.name.clone(), format!("{:.1} W", row.power_w)))
        .collect()
}

/// Both failure spellings carry one `FailureKind`; the provider's detail
/// string is host-specific and never parsed here.
fn failure_kind(failure: &taskmanager_application::RaplPowerRequestFailure) -> FailureKind {
    match failure {
        taskmanager_application::RaplPowerRequestFailure::Submission(kind) => *kind,
        taskmanager_application::RaplPowerRequestFailure::Provider(failed) => failed.kind,
    }
}

const fn model_from_failure(kind: FailureKind) -> PackagePowerModel {
    match kind {
        FailureKind::RequiresEscalation => PackagePowerModel::AuthorizationRequired,
        FailureKind::PermissionDenied => PackagePowerModel::Unavailable("cpu.package_power_denied"),
        FailureKind::MissingDependency => {
            PackagePowerModel::Unavailable("cpu.package_power_helper")
        }
        FailureKind::Unsupported => PackagePowerModel::Unavailable("cpu.package_power_unsupported"),
        FailureKind::TimedOut
        | FailureKind::TemporarilyUnavailable
        | FailureKind::IdentityChanged
        | FailureKind::Rejected
        | FailureKind::ProviderFault => {
            PackagePowerModel::Unavailable("cpu.package_power_unavailable")
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RootView glue — the ONLY submission entry, gated on the projection.
// ─────────────────────────────────────────────────────────────────────────────

impl RootView {
    /// The user clicked the authorize affordance. This is the single explicit
    /// trigger for the RAPL lane's OS-native prompt; never auto-invoked.
    pub(crate) fn authorize_package_power(&mut self, cx: &mut Context<Self>) {
        let inputs = PackagePowerInputs {
            state: self.shell.rapl_power_state(),
            capability: self
                .projection()
                .capability_status(&CapabilityId::TELEMETRY_CPU_PACKAGE_POWER),
        };
        if !matches!(
            package_power_model(&inputs),
            PackagePowerModel::AuthorizationRequired
        ) {
            return;
        }
        self.submit_package_power_request();
        cx.notify();
    }

    /// Submit one package-power read. Beginning the attempt before touching
    /// the platform makes replacement and synchronous rejection obey the same
    /// identity rules as asynchronous terminals (`submit_gpu_engine_rows_refresh`).
    pub(crate) fn submit_package_power_request(&mut self) -> bool {
        let attempt = self.shell.begin_rapl_power_request();
        let result = self.platform.as_mut().map_or_else(
            || Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_rapl_power(RaplPowerRequest::Refresh, platform_submission_time_ms())
                    .map_err(|error| error.kind)
            },
        );
        match result {
            Ok(request_id) => self.shell.accept_rapl_power_request(attempt, request_id),
            Err(kind) => {
                self.shell
                    .reject_rapl_power_request(attempt, request_submission_failure(kind));
                false
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rendering
// ─────────────────────────────────────────────────────────────────────────────

/// The package-power subsection of the CPU details panel: a dim heading plus
/// the honest body for the current projection. `Hidden` never reaches here
/// (the caller omits the whole section).
pub(super) fn render_package_power_section(
    theme: &Theme,
    model: &PackagePowerModel,
    cx: &mut Context<RootView>,
) -> Div {
    let heading = div()
        .text_size(tokens::FONT_12)
        .font_weight(tokens::FONT_WEIGHT_BOLD.into())
        .text_color(theme.fg_dim)
        .child(i18n::t("cpu.package_power"));
    let mut col = div()
        .debug_selector(|| "tm-cpu-package-power".to_string())
        .flex()
        .flex_col()
        .gap(tokens::SPACE_5)
        .w_full()
        .child(heading);
    match model {
        PackagePowerModel::Packages(rows) => {
            if rows.is_empty() {
                // The helper ran and honestly reported no packages: a typed
                // empty message, not a fabricated row.
                col = col.child(dim_text(theme, i18n::t("cpu.package_power_none")));
            } else {
                for (label, value) in rows {
                    col = col.child(super::details_panel::kv_row(theme, label, value));
                }
            }
        }
        PackagePowerModel::Measuring => {
            col = col.child(dim_text(theme, i18n::t("cpu.package_power_measuring")));
        }
        PackagePowerModel::AuthorizationRequired => {
            col = col
                .child(dim_text(theme, i18n::t("cpu.package_power_requires_auth")))
                .child(action_button(
                    theme,
                    i18n::t("cpu.package_power_authorize"),
                    theme.accent,
                    "tm-cpu-package-power-authorize",
                    cx.listener(move |view, _ev, _win, cx| {
                        view.authorize_package_power(cx);
                    }),
                ));
        }
        PackagePowerModel::Unavailable(key) => {
            col = col.child(dim_text(theme, i18n::t(key)));
        }
        PackagePowerModel::Hidden => {}
    }
    col
}

fn dim_text(theme: &Theme, text: &str) -> Div {
    div()
        .text_size(tokens::FONT_12)
        .text_color(theme.fg_dim)
        .child(text.to_owned())
}

/// Keyboard-focusable, clickable text button in the shared affordance style
/// (accent label, focus ring, pointer cursor — the GPU engines panel's
/// `action_button` idiom). `selector` is a test-support geometry breakpoint.
fn action_button(
    theme: &Theme,
    label: &str,
    color: Color,
    selector: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let btn = div()
        .id("cpu-package-power-action")
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(theme))
        .cursor_pointer()
        .on_click(on_click)
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(color)
                .child(label.to_owned()),
        );
    btn.debug_selector(move || selector.to_string())
        .into_any_element()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_cpu_view_package_power_tests.rs"]
mod tests;
