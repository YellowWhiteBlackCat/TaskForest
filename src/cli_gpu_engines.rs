//! CLI rendering for the per-feature escalation surface `--gpu-engines`.
//!
//! This mode is the unprivileged end of ADR-023 / permission-model Boundary 2:
//! it does NOT spawn the native telemetry runtime and it does NOT elevate the
//! main app. It calls [`taskmanager_escalation::polkit::invoke_perf_helper`],
//! which triggers the OS-native `pkexec` + polkit prompt **only when this flag
//! is invoked**, then prints the typed result as JSON to stdout and exits.
//!
//! Honesty contract (the project red line): success prints real engine rows;
//! every failure path prints a typed honest document — the helper's own typed
//! error (`status: error` + contract `kind`), or an escalation-layer
//! `status: unavailable` with a typed reason — and never a fabricated row.
//!
//! Rendering lives here (not in `cli.rs`) to keep `cli.rs` under the workspace
//! 800-line file guard. JSON assembly uses `serde_json::Value` already pulled in
//! by the root crate; the escalation seam stays zero-dependency and owns the
//! parsing, this module only renders its typed outcome.

#![forbid(unsafe_code)]

use std::io::{self, Write};

use serde_json::json;
use taskmanager_escalation::EscalationDenialReason;
use taskmanager_escalation::EscalationFeature;
use taskmanager_escalation::polkit::PerfHelperOutcome;

/// Run the `--gpu-engines` mode against stdout: invoke the privileged helper
/// through the per-feature gate and print the typed outcome as JSON. Returns
/// `Ok` as long as the document could be written; the OUTCOME (success vs a
/// typed denial) is carried in the printed JSON, not in the process exit code,
/// so a denial is honest output rather than a runtime error.
pub fn run_gpu_engines() -> io::Result<()> {
    let outcome = taskmanager_escalation::polkit::invoke_perf_helper();
    let document = render_outcome(&outcome);
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(document.as_bytes())?;
    // Trailing newline so downstream tools (jq, grep) see a clean record.
    handle.write_all(b"\n")?;
    Ok(())
}

/// Render one typed [`PerfHelperOutcome`] as a pretty-printed JSON document.
///
/// Shapes:
/// * `Success`   -> `{"status":"ok","driver":..,"sample_ms":..,"engines":[..]}`
/// * `HelperError` -> `{"status":"error","kind":..,"detail":..}`
/// * `Unavailable` -> `{"status":"unavailable","reason":..,"detail":..}`
fn render_outcome(outcome: &PerfHelperOutcome) -> String {
    let value = match outcome {
        PerfHelperOutcome::Success(success) => {
            let engines = success
                .engines
                .iter()
                .map(|engine| {
                    json!({
                        "name": engine.name,
                        "class": engine.class,
                        "busy_pct": engine.busy_pct as f64,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "status": "ok",
                "driver": success.driver,
                "sample_ms": success.sample_ms,
                "engines": engines,
            })
        }
        PerfHelperOutcome::HelperError(error) => json!({
            "status": "error",
            "kind": error.kind.as_contract_str(),
            "detail": error.detail,
        }),
        PerfHelperOutcome::Unavailable { reason, detail } => json!({
            "status": "unavailable",
            "reason": denial_reason_str(*reason),
            "feature": feature_key(),
            "detail": detail,
        }),
    };
    // The document holds only finite numbers, plain strings, and the fixed
    // status keys, so to_string_pretty is infallible in practice; the fallback
    // keeps this production path panic-free per the panic-surface gate.
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| {
        "{\"status\":\"unavailable\",\"reason\":\"helper_unavailable\"}".to_owned()
    })
}

/// The escalation feature this surface reaches, as a stable JSON key.
fn feature_key() -> &'static str {
    match EscalationFeature::IntelPmu {
        EscalationFeature::IntelPmu => "intel_pmu",
        EscalationFeature::PerProcessNet => "per_process_net",
        EscalationFeature::AtaSmart => "ata_smart",
        EscalationFeature::ForeignProcessControl => "foreign_process_control",
        EscalationFeature::SystemServiceControl => "system_service_control",
        EscalationFeature::MemorySmbios => "memory_smbios",
        EscalationFeature::PackagePowerRapl => "package_power_rapl",
        EscalationFeature::CpuMsr => "cpu_msr",
    }
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
#[path = "../tests/logic/cli_gpu_engines_tests.rs"]
mod tests;
