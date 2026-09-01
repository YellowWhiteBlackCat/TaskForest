//! Structured clipboard export and redacted system diagnostics for Iced.
//!
//! Provides formatting helpers to export processes as TSV/JSON, and
//! system information as redacted Markdown suitable for bug reports.
//!
//! JSON is produced by `serde_json`, so a command line carrying quotes,
//! newlines or other control characters still yields valid JSON. Redaction is
//! delegated to the audited core diagnostics contract
//! ([`DiagnosticBundlePlan::prepare`]) — this module spells no redaction rule
//! of its own, and the summary core returns travels with the report instead of
//! being dropped.

use taskmanager_core::core::diagnostics::RedactionSummary;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::{
    DiagnosticBundleError, DiagnosticBundleErrorKind, DiagnosticBundlePlan, DiagnosticSource,
};

use taskmanager_shell::presentation::{bytes, duration, missing_value};

/// The logical source name the diagnostics report travels under. A constant
/// because the core bundle validates names and rejects paths.
const DIAGNOSTICS_SOURCE_NAME: &str = "system-diagnostics.md";

/// Format a single process item as tab-separated values (TSV).
#[must_use]
pub fn process_to_tsv(process: &ProcessItem) -> String {
    let user = process.current_user().unwrap_or_else(missing_value);
    let cpu = process
        .current_cpu_percentage()
        .map_or_else(missing_value, |value| format!("{value:.1}%"));
    let memory = process
        .current_memory_bytes()
        .map_or_else(missing_value, bytes);
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        process.pid,
        process.name,
        cpu,
        memory,
        if user.is_empty() { "—" } else { &user },
        process.status,
    )
}

/// One JSON string literal, escaped by serde (quotes, backslashes, and every
/// control character) instead of a hand-rolled rule.
fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

/// Format a process item as a clean JSON object string.
///
/// The field set (`pid`, `name`, `cpu_usage`, `memory_bytes`, `user`,
/// `status`, `cmdline`), their order, and the numeric spellings are the
/// published ones; every string literal is escaped by serde, so a command line
/// carrying quotes, newlines or other control characters still yields legal
/// JSON.
#[must_use]
pub fn process_to_json(process: &ProcessItem) -> String {
    let user = process
        .current_user()
        .map_or_else(|| "null".to_owned(), |user| json_string(&user));
    let cmdline = if process.cmdline.is_empty() {
        "null".to_owned()
    } else {
        json_string(&process.cmdline)
    };
    let cpu = process
        .current_cpu_percentage()
        .map_or_else(|| "null".to_owned(), |value| format!("{value:.1}"));
    let memory = process
        .current_memory_bytes()
        .map_or_else(|| "null".to_owned(), |value| value.to_string());
    format!(
        "{{\n  \"pid\": {},\n  \"name\": {},\n  \"cpu_usage\": {},\n  \"memory_bytes\": {},\n  \"user\": {},\n  \"status\": {},\n  \"cmdline\": {}\n}}",
        process.pid,
        json_string(&process.name),
        cpu,
        memory,
        user,
        json_string(&process.status),
        cmdline
    )
}

/// The diagnostics report after core's audited redaction contract ran over it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedactedDiagnosticsReport {
    /// Clipboard-ready Markdown with every private occurrence replaced.
    pub markdown: String,
    /// What core removed, so the export path accounts for its redactions.
    pub redactions: RedactionSummary,
}

/// Redact `text` with the core diagnostics contract and report what it removed.
///
/// The plan keeps its sanitized sources private and exposes the already-cleaned
/// text through one read-only core accessor. No redaction rule is re-implemented
/// here, and a plan whose named source cannot be recovered fails the export
/// instead of leaking the input.
pub fn redact_with_summary(
    text: &str,
    usernames: impl IntoIterator<Item = String>,
) -> Result<RedactedDiagnosticsReport, DiagnosticBundleError> {
    let plan = DiagnosticBundlePlan::prepare(
        vec![DiagnosticSource {
            name: DIAGNOSTICS_SOURCE_NAME.to_string(),
            contents: text.to_string(),
        }],
        usernames,
    )?;
    let redactions = plan.preview().redactions;
    let contents = plan
        .sanitized_contents(DIAGNOSTICS_SOURCE_NAME)
        .ok_or_else(|| {
            DiagnosticBundleError::with_detail(
                DiagnosticBundleErrorKind::Encode,
                "sanitized diagnostics source missing from the plan",
            )
        })?
        .to_owned();
    Ok(RedactedDiagnosticsReport {
        markdown: contents,
        redactions,
    })
}

/// Generate a comprehensive Markdown system diagnostics bundle report with
/// private material removed by the core redaction contract.
///
/// `usernames` are the account labels observed on this host; core replaces
/// them (and every filesystem path and IP address) before the text reaches the
/// clipboard.
pub fn system_diagnostics_markdown(
    hardware: Option<&HardwareInfo>,
    snapshot: Option<&SystemSnapshot>,
    usernames: impl IntoIterator<Item = String>,
) -> Result<String, DiagnosticBundleError> {
    redact_with_summary(&diagnostics_markdown_body(hardware, snapshot), usernames)
        .map(|report| report.markdown)
}

/// The unredacted report body. Private material stays here only until
/// [`system_diagnostics_markdown`] hands the whole text to core.
fn diagnostics_markdown_body(
    hardware: Option<&HardwareInfo>,
    snapshot: Option<&SystemSnapshot>,
) -> String {
    let mut out = String::with_capacity(2048);
    out.push_str("### TaskForest System Diagnostics Report\n\n");
    out.push_str("```markdown\n");

    if let Some(hw) = hardware {
        let os_str = format!(
            "{} {}",
            hw.os_name.as_deref().unwrap_or("Unknown OS"),
            hw.os_version.as_deref().unwrap_or("")
        );
        out.push_str(&format!("OS: {}\n", os_str.trim()));
        out.push_str(&format!(
            "Kernel: {}\n",
            hw.kernel_version.as_deref().unwrap_or("Unknown")
        ));
        out.push_str(&format!(
            "Hostname: {}\n",
            hw.hostname.as_deref().unwrap_or("—")
        ));
        out.push_str(&format!(
            "CPU: {}\n",
            hw.cpu_brand.as_deref().unwrap_or("Unknown")
        ));
        out.push_str(&format!(
            "Cores: {}\n",
            hw.cpu_cores
                .map(|c| c.to_string())
                .unwrap_or_else(missing_value)
        ));
    }

    if let Some(snap) = snapshot {
        out.push_str(&format!("Uptime: {}\n", duration(snap.uptime_secs)));
        out.push_str(&format!("Process Count: {}\n", snap.processes));
        let cpu = snap
            .cpu
            .current_global_usage_pct()
            .filter(|value| value.is_finite())
            .map_or_else(missing_value, |value| format!("{value:.1}%"));
        out.push_str(&format!("CPU Total Usage: {cpu}\n"));
        let memory = match (
            snap.memory.current_used_bytes(),
            snap.memory.current_total_bytes(),
        ) {
            (Some(used), Some(total)) => format!("{} / {}", bytes(used), bytes(total)),
            _ => missing_value(),
        };
        out.push_str(&format!("Memory: {memory}\n"));
        if !snap.gpu.is_empty() {
            out.push_str(&format!("GPU Count: {}\n", snap.gpu.len()));
            for (idx, gpu) in snap.gpu.iter().enumerate() {
                // The driver name and its version are independent facts: an
                // adapter may prove a release string without a kernel driver
                // label, so the version is appended only when proven.
                let mut gpu_facts = format!("Driver: {}", gpu.driver.as_deref().unwrap_or("—"));
                if let Some(version) = gpu
                    .driver_version
                    .as_deref()
                    .filter(|value| !value.is_empty())
                {
                    gpu_facts.push_str(&format!(", Version: {version}"));
                }
                out.push_str(&format!("  [GPU {}] {} ({gpu_facts})\n", idx, gpu.brand));
            }
        }
        if !snap.disks.is_empty() {
            out.push_str(&format!("Disks Count: {}\n", snap.disks.len()));
            for (idx, disk) in snap.disks.iter().enumerate() {
                out.push_str(&format!(
                    "  [Disk {}] {} ({})\n",
                    idx, disk.name, disk.model
                ));
            }
        }
    } else {
        out.push_str("System Snapshot: Unavailable\n");
    }

    out.push_str("```\n");
    out
}

#[cfg(test)]
#[path = "../tests/gui/export_tests.rs"]
mod tests;
