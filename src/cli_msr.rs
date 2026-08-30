//! CLI rendering for the per-feature escalation surface `--msr`.
//!
//! This mode is the unprivileged end of ADR-023/048 / permission-model
//! Boundary 2: it does NOT spawn the native telemetry runtime and it does NOT
//! elevate the main app. It calls
//! [`taskmanager_escalation::polkit::invoke_msr_helper`], which triggers the
//! OS-native `pkexec` + polkit prompt **only when this flag is invoked**, then
//! prints the typed result as JSON to stdout and exits.
//!
//! Honesty contract (the project red line): success prints real per-node MSR
//! readouts — a register the CPU does not implement, and the excluded base
//! clock, print as `null`; every failure path prints a typed honest document
//! — the helper's own typed error (`status: error` + contract `kind`), or an
//! escalation-layer `status: unavailable` with a typed reason — and never a
//! fabricated reading.

#![forbid(unsafe_code)]

use std::io::{self, Write};

use serde_json::json;
use taskmanager_escalation::EscalationDenialReason;
use taskmanager_escalation::polkit::MsrHelperOutcome;

/// Run the `--msr` mode against stdout: invoke the privileged helper through
/// the per-feature gate and print the typed outcome as JSON. Returns `Ok` as
/// long as the document could be written; the OUTCOME is carried in the
/// printed JSON, not in the process exit code.
pub fn run_msr() -> io::Result<()> {
    let outcome = taskmanager_escalation::polkit::invoke_msr_helper();
    let document = render_outcome(&outcome);
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(document.as_bytes())?;
    handle.write_all(b"\n")?;
    Ok(())
}

/// Render one typed [`MsrHelperOutcome`] as a pretty-printed JSON document.
///
/// Presentation rounding per physical quantity: the contract carries
/// full-precision floats; the CLI document rounds to each quantity's display
/// resolution (bclk/temperature one decimal, multipliers two, volts three)
/// instead of echoing f32 artifacts.
fn round_quantity(value: f32, decimals: i32) -> f64 {
    // Round in the f64 domain: f32 mantissas turn e.g. 1.21875→1.219 into
    // 1.2189999… when the factor multiply happens in f32.
    let factor = 10f64.powi(decimals);
    ((f64::from(value)) * factor).round() / factor
}

fn rounded(value: Option<f32>, decimals: i32) -> Option<f64> {
    value.map(|value| round_quantity(value, decimals))
}

/// Shapes:
/// * `Success`     -> `{"status":"ok","packages":[{"cpu":..,"bclk_mhz":null|..,..}]}`
/// * `HelperError` -> `{"status":"error","kind":..,"detail":..}`
/// * `Unavailable` -> `{"status":"unavailable","reason":..,"feature":..,"detail":..}`
fn render_outcome(outcome: &MsrHelperOutcome) -> String {
    let value = match outcome {
        MsrHelperOutcome::Success(success) => {
            let packages = success
                .packages
                .iter()
                .map(|package| {
                    json!({
                        "cpu": package.cpu,
                        "bclk_mhz": rounded(package.bclk_mhz, 1),
                        "temperature_c": rounded(package.temperature_c, 1),
                        "multiplier": rounded(package.multiplier, 2),
                        "multiplier_min": rounded(package.multiplier_min, 2),
                        "multiplier_max": rounded(package.multiplier_max, 2),
                        "vcore_v": rounded(package.vcore_v, 3),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "status": "ok",
                "packages": packages,
            })
        }
        MsrHelperOutcome::HelperError(error) => json!({
            "status": "error",
            "kind": error.kind.as_contract_str(),
            "detail": error.detail,
        }),
        MsrHelperOutcome::Unavailable { reason, detail } => json!({
            "status": "unavailable",
            "reason": denial_reason_str(*reason),
            "feature": "cpu_msr",
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
#[path = "../tests/logic/cli_msr_tests.rs"]
mod tests;
