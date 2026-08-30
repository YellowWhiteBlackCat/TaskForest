//! CLI rendering for the per-feature escalation surface `--package-power`.
//!
//! This mode is the unprivileged end of ADR-023 / permission-model Boundary 2:
//! it does NOT spawn the native telemetry runtime and it does NOT elevate the
//! main app. It calls [`taskmanager_escalation::polkit::invoke_rapl_helper`],
//! which triggers the OS-native `pkexec` + polkit prompt **only when this flag
//! is invoked**, then prints the typed result as JSON to stdout and exits.
//!
//! Honesty contract (the project red line): success prints real per-package
//! watt readings; every failure path prints a typed honest document — the
//! helper's own typed error (`status: error` + contract `kind`), or an
//! escalation-layer `status: unavailable` with a typed reason — and never a
//! fabricated watt figure.

#![forbid(unsafe_code)]

use std::io::{self, Write};

use serde_json::json;
use taskmanager_escalation::EscalationDenialReason;
use taskmanager_escalation::polkit::RaplHelperOutcome;

/// Presentation rounding: the contract carries full-precision floats; the CLI
/// document rounds each reading to its display resolution (watts to one
/// decimal) instead of echoing f32 artifacts like 6.0503997802734375.
fn round_watts(value: f32) -> f64 {
    // Round in the f64 domain so the JSON number prints at display
    // resolution instead of an f32 artifact.
    (f64::from(value) * 10.0).round() / 10.0
}

/// Run the `--package-power` mode against stdout: invoke the privileged
/// helper through the per-feature gate and print the typed outcome as JSON.
/// Returns `Ok` as long as the document could be written; the OUTCOME is
/// carried in the printed JSON, not in the process exit code.
pub fn run_package_power() -> io::Result<()> {
    let outcome = taskmanager_escalation::polkit::invoke_rapl_helper();
    let document = render_outcome(&outcome);
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(document.as_bytes())?;
    handle.write_all(b"\n")?;
    Ok(())
}

/// Render one typed [`RaplHelperOutcome`] as a pretty-printed JSON document.
///
/// Shapes:
/// * `Success`     -> `{"status":"ok","sample_ms":..,"packages":[..]}`
/// * `HelperError` -> `{"status":"error","kind":..,"detail":..}`
/// * `Unavailable` -> `{"status":"unavailable","reason":..,"feature":..,"detail":..}`
fn render_outcome(outcome: &RaplHelperOutcome) -> String {
    let value = match outcome {
        RaplHelperOutcome::Success(success) => {
            let packages = success
                .packages
                .iter()
                .map(|package| {
                    json!({
                        "name": package.name,
                        "power_w": round_watts(package.power_w),
                        "energy_delta_uj": package.energy_delta_uj,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "status": "ok",
                "sample_ms": success.sample_ms,
                "packages": packages,
            })
        }
        RaplHelperOutcome::HelperError(error) => json!({
            "status": "error",
            "kind": error.kind.as_contract_str(),
            "detail": error.detail,
        }),
        RaplHelperOutcome::Unavailable { reason, detail } => json!({
            "status": "unavailable",
            "reason": denial_reason_str(*reason),
            "feature": "package_power_rapl",
            "detail": detail,
        }),
    };
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| {
        "{\"status\":\"unavailable\",\"reason\":\"helper_unavailable\"}".to_owned()
    })
}

/// Snake-case JSON label for an [`EscalationDenialReason`].
fn denial_reason_str(reason: EscalationDenialReason) -> &'static str {
    match reason {
        EscalationDenialReason::Unsupported => "unsupported",
        EscalationDenialReason::PermissionDenied => "permission_denied",
        EscalationDenialReason::AuthorizationUnavailable => "authorization_unavailable",
        EscalationDenialReason::HelperUnavailable => "helper_unavailable",
        EscalationDenialReason::HelperProtocolViolation => "helper_protocol_violation",
    }
}

#[cfg(test)]
#[path = "../tests/logic/cli_package_power_tests.rs"]
mod tests;
