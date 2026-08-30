//! CLI rendering for the per-feature escalation surface `--memory-smbios`.
//!
//! This mode is the unprivileged end of ADR-023 / permission-model Boundary 2:
//! it does NOT spawn the native telemetry runtime and it does NOT elevate the
//! main app. It calls [`taskmanager_escalation::polkit::invoke_smbios_helper`],
//! which triggers the OS-native `pkexec` + polkit prompt **only when this flag
//! is invoked**, then prints the typed result as JSON to stdout and exits.
//!
//! Honesty contract (the project red line): success prints real slot/module
//! inventory plus the system/board identity facts the same walk read; every
//! failure path prints a typed honest document — the helper's
//! own typed error (`status: error` + contract `kind`), or an escalation-layer
//! `status: unavailable` with a typed reason — and never a fabricated module
//! row. Absent facts inside a success document print as JSON `null`.

#![forbid(unsafe_code)]

use std::io::{self, Write};

use serde_json::json;
use taskmanager_escalation::EscalationDenialReason;
use taskmanager_escalation::polkit::{DmiIdentityFacts, SmbiosHelperOutcome};

/// Run the `--memory-smbios` mode against stdout: invoke the privileged
/// helper through the per-feature gate and print the typed outcome as JSON.
/// Returns `Ok` as long as the document could be written; the OUTCOME is
/// carried in the printed JSON, not in the process exit code.
pub fn run_memory_smbios() -> io::Result<()> {
    let outcome = taskmanager_escalation::polkit::invoke_smbios_helper();
    let document = render_outcome(&outcome);
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(document.as_bytes())?;
    handle.write_all(b"\n")?;
    Ok(())
}

/// Render one typed [`SmbiosHelperOutcome`] as a pretty-printed JSON document.
///
/// Shapes:
/// * `Success`     -> `{"status":"ok","slots_total":..,"slots_used":..,
///                     "modules":[..],"identity":{..}|null}`
/// * `HelperError` -> `{"status":"error","kind":..,"detail":..}`
/// * `Unavailable` -> `{"status":"unavailable","reason":..,"feature":..,"detail":..}`
fn render_outcome(outcome: &SmbiosHelperOutcome) -> String {
    let value = match outcome {
        SmbiosHelperOutcome::Success(success) => {
            let modules = success
                .modules
                .iter()
                .map(|module| {
                    json!({
                        "slot": module.slot,
                        "size_mb": module.size_mb,
                        "speed_mts": module.speed_mts,
                        "configured_speed_mts": module.configured_speed_mts,
                        "manufacturer": module.manufacturer,
                        "serial_number": module.serial_number,
                        "part_number": module.part_number,
                        "form_factor": module.form_factor,
                        "memory_type": module.memory_type,
                        "locator": module.locator,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "status": "ok",
                "slots_total": success.slots_total,
                "slots_used": success.slots_used,
                "modules": modules,
                "identity": identity_document(success.identity.as_ref()),
            })
        }
        SmbiosHelperOutcome::HelperError(error) => json!({
            "status": "error",
            "kind": error.kind.as_contract_str(),
            "detail": error.detail,
        }),
        SmbiosHelperOutcome::Unavailable { reason, detail } => json!({
            "status": "unavailable",
            "reason": denial_reason_str(*reason),
            "feature": "memory_smbios",
            "detail": detail,
        }),
    };
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| {
        "{\"status\":\"unavailable\",\"reason\":\"helper_unavailable\"}".to_owned()
    })
}

/// The success document's `identity` object: `null` when the host carries no
/// type-0/1/2 DMI entries, otherwise the 13 system/board facts, each a string
/// or an honest `null` — never a fabricated value.
fn identity_document(identity: Option<&DmiIdentityFacts>) -> serde_json::Value {
    match identity {
        None => serde_json::Value::Null,
        Some(facts) => json!({
            "bios_vendor": facts.bios_vendor,
            "bios_version": facts.bios_version,
            "bios_date": facts.bios_date,
            "board_manufacturer": facts.board_manufacturer,
            "board_product": facts.board_product,
            "board_serial": facts.board_serial,
            "board_asset_tag": facts.board_asset_tag,
            "system_manufacturer": facts.system_manufacturer,
            "system_product": facts.system_product,
            "system_serial": facts.system_serial,
            "system_uuid": facts.system_uuid,
            "system_sku": facts.system_sku,
            "system_family": facts.system_family,
        }),
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
#[path = "../tests/logic/cli_memory_smbios_tests.rs"]
mod tests;
