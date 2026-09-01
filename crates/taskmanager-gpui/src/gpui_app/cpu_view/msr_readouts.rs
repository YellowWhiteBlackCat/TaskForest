//! MSR-readout section of the CPU details panel (the CpuMsr request lane).
//!
//! Package temperature, performance multipliers, and Vcore can only come from
//! the privileged MSR helper (`telemetry.cpu.msr`, ADR-023/048
//! permission-model Boundary 2). The section is an honest projection of the
//! application-owned request session: every non-ready variant renders a typed
//! placeholder or the typed failure reason, never a fabricated register
//! value, and a register the CPU does not implement renders no row at all
//! rather than a dash slot.
//!
//! # Escalation discipline
//!
//! The OS-native prompt is strictly user-initiated through the central Settings
//! permission center. One click submits exactly one request (begin → platform
//! submit → accept/reject, mirroring `authorize_package_power`); there is no
//! auto-poll chain, and the handler re-checks the projection so a stale click
//! can never prompt.

use gpui::{Context, Div, InteractiveElement, ParentElement, Styled, div};
use taskmanager_application::{
    MsrReadoutRequest, MsrReadoutState, i18n, request_submission_failure,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::MsrReadoutSnapshot;
use taskmanager_platform_contract::{CapabilityId, CapabilityStatus, SubmissionErrorKind};

use crate::gpui_app::root::RootView;
use crate::gpui_app::root::platform_submission_time_ms;
use taskmanager_theme::{Theme, tokens};

const MAX_VISIBLE_MSR_ROWS: usize = 5;

/// Render-entry inputs for the section: the shared session state plus the
/// runtime capability catalog entry for the lane.
pub(crate) struct MsrReadoutsInputs<'a> {
    pub state: &'a MsrReadoutState,
    pub capability: Option<CapabilityStatus>,
}

/// Pure projection of the MSR-readout lane for the details panel.
#[derive(Debug, PartialEq)]
pub(crate) enum MsrReadoutsModel {
    /// No live session and no registered lane on this host: the section
    /// renders nothing at all.
    Hidden,
    /// Real per-node fact rows `(label, value)` from an accepted payload. A
    /// register the CPU does not implement is an absent row — only a real
    /// measured value ever renders.
    Rows(Vec<(String, String)>),
    /// A request is in flight and no accepted payload exists yet.
    Measuring,
    /// The lane is escalation-backed: the central Settings permission center
    /// renders the typed hint plus the authorize affordance. No number may
    /// render in this state.
    AuthorizationRequired,
    /// A typed failure; the value is the localized message key.
    Unavailable(&'static str),
}

#[must_use]
pub(crate) fn msr_readouts_model(inputs: &MsrReadoutsInputs<'_>) -> MsrReadoutsModel {
    match inputs.state {
        MsrReadoutState::Ready(ready) => MsrReadoutsModel::Rows(fact_rows(&ready.snapshot)),
        MsrReadoutState::Loading {
            last_good: Some(ready),
            ..
        } => MsrReadoutsModel::Rows(fact_rows(&ready.snapshot)),
        MsrReadoutState::Loading {
            last_good: None, ..
        } => MsrReadoutsModel::Measuring,
        MsrReadoutState::Failed(failed) => model_from_failure(failure_kind(&failed.failure)),
        MsrReadoutState::Closed => match inputs.capability {
            // The runtime catalog proves an escalation-backed lane exists:
            // offer the one explicit authorization entry.
            Some(CapabilityStatus::Available | CapabilityStatus::PermissionRequired) => {
                MsrReadoutsModel::AuthorizationRequired
            }
            Some(CapabilityStatus::MissingDependency) => {
                MsrReadoutsModel::Unavailable("cpu.msr_readouts_helper")
            }
            Some(CapabilityStatus::Degraded(kind)) => model_from_failure(kind),
            Some(CapabilityStatus::Unsupported)
            | Some(CapabilityStatus::TemporarilyUnavailable)
            | Some(CapabilityStatus::Stale)
            | None => MsrReadoutsModel::Hidden,
        },
    }
}

/// One label/value row per real register fact, node by node — temperature at
/// one decimal °C (the live readout's shared spelling), multipliers as
/// `×NN.N` (the details panel's spec-row spelling), volts at three decimals.
/// A register the CPU does not implement stays `None` and renders no row.
fn fact_rows(snapshot: &MsrReadoutSnapshot) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for readout in &snapshot.packages {
        let node = format!("CPU {}", readout.cpu);
        if let Some(temperature) = readout.temperature_c {
            rows.push((
                node_label(&node, "cpu.msr_temperature"),
                format!("{temperature:.1} °C"),
            ));
        }
        if let Some(multiplier) = readout.multiplier {
            rows.push((
                node_label(&node, "cpu.msr_multiplier"),
                format!("\u{00d7}{multiplier:.1}"),
            ));
        }
        if let Some(minimum) = readout.multiplier_min {
            rows.push((
                node_label(&node, "cpu.msr_multiplier_min"),
                format!("\u{00d7}{minimum:.1}"),
            ));
        }
        if let Some(maximum) = readout.multiplier_max {
            rows.push((
                node_label(&node, "cpu.msr_multiplier_max"),
                format!("\u{00d7}{maximum:.1}"),
            ));
        }
        if let Some(vcore) = readout.vcore_v {
            rows.push((node_label(&node, "cpu.msr_vcore"), format!("{vcore:.3} V")));
        }
    }
    rows
}

fn node_label(node: &str, key: &'static str) -> String {
    format!("{node} \u{00b7} {}", i18n::t(key))
}

/// Both failure spellings carry one `FailureKind`; the provider's detail
/// string is host-specific and never parsed here.
fn failure_kind(failure: &taskmanager_application::MsrReadoutRequestFailure) -> FailureKind {
    match failure {
        taskmanager_application::MsrReadoutRequestFailure::Submission(kind) => *kind,
        taskmanager_application::MsrReadoutRequestFailure::Provider(failed) => failed.kind,
    }
}

const fn model_from_failure(kind: FailureKind) -> MsrReadoutsModel {
    match kind {
        FailureKind::RequiresEscalation => MsrReadoutsModel::AuthorizationRequired,
        FailureKind::PermissionDenied => MsrReadoutsModel::Unavailable("cpu.msr_readouts_denied"),
        FailureKind::MissingDependency => MsrReadoutsModel::Unavailable("cpu.msr_readouts_helper"),
        FailureKind::Unsupported => MsrReadoutsModel::Unavailable("cpu.msr_readouts_unsupported"),
        FailureKind::TimedOut
        | FailureKind::TemporarilyUnavailable
        | FailureKind::IdentityChanged
        | FailureKind::Rejected
        | FailureKind::ProviderFault => {
            MsrReadoutsModel::Unavailable("cpu.msr_readouts_unavailable")
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RootView glue — the ONLY submission entry, gated on the projection.
// ─────────────────────────────────────────────────────────────────────────────

impl RootView {
    /// The user clicked the authorize affordance. This is the single explicit
    /// trigger for the MSR lane's OS-native prompt; never auto-invoked.
    pub(crate) fn authorize_msr_readouts(&mut self, cx: &mut Context<Self>) {
        let inputs = MsrReadoutsInputs {
            state: self.shell.msr_readout_state(),
            capability: self
                .projection()
                .capability_status(&CapabilityId::TELEMETRY_CPU_MSR),
        };
        if !matches!(
            msr_readouts_model(&inputs),
            MsrReadoutsModel::AuthorizationRequired
                | MsrReadoutsModel::Unavailable("cpu.msr_readouts_denied")
        ) {
            return;
        }
        self.submit_msr_readout_request();
        cx.notify();
    }

    /// Submit one MSR readout. Beginning the attempt before touching the
    /// platform makes replacement and synchronous rejection obey the same
    /// identity rules as asynchronous terminals (`submit_package_power_request`).
    pub(crate) fn submit_msr_readout_request(&mut self) -> bool {
        let attempt = self.shell.begin_msr_readout_request();
        let result = self.platform.as_mut().map_or_else(
            || Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_msr_readout(MsrReadoutRequest::Refresh, platform_submission_time_ms())
                    .map_err(|error| error.kind)
            },
        );
        match result {
            Ok(request_id) => self.shell.accept_msr_readout_request(attempt, request_id),
            Err(kind) => {
                self.shell
                    .reject_msr_readout_request(attempt, request_submission_failure(kind));
                false
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rendering
// ─────────────────────────────────────────────────────────────────────────────

/// The MSR-readout subsection of the CPU details panel: a dim heading plus
/// the honest body for the current projection. `Hidden` never reaches here
/// (the caller omits the whole section).
pub(super) fn render_msr_readouts_section(theme: &Theme, model: &MsrReadoutsModel) -> Div {
    let MsrReadoutsModel::Rows(rows) = model else {
        return div();
    };
    let heading = div()
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
        .font_weight(taskmanager_ui::theme_binding::font_weight(
            tokens::FONT_WEIGHT_BOLD,
        ))
        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
        .child(i18n::t("cpu.msr_readouts"));
    let mut col = div()
        .debug_selector(|| "tm-cpu-msr-readouts".to_string())
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_5,
        ))
        .w_full()
        .child(heading);
    if rows.is_empty() {
        // The helper ran and honestly reported no nodes: a typed empty
        // message, not a fabricated row.
        col = col.child(dim_text(theme, i18n::t("cpu.msr_readouts_none")));
    } else {
        let visible = rows.len().min(MAX_VISIBLE_MSR_ROWS);
        for (label, value) in rows.iter().take(visible) {
            col = col.child(super::details_panel::kv_row(theme, label, value));
        }
        if rows.len() > visible {
            col = col.child(dim_text(
                theme,
                &i18n::t("common.more_rows")
                    .replace("{count}", &(rows.len() - visible).to_string()),
            ));
        }
    }
    col
}

fn dim_text(theme: &Theme, text: &str) -> Div {
    div()
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
        .child(text.to_owned())
}

// Page-local authorization controls intentionally do not exist here. The
// central Settings permission center owns the request trigger; this renderer
// only receives accepted `Rows` projections.

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_cpu_view_msr_readouts_tests.rs"]
mod tests;
