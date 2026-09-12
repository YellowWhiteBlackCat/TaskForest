//! Terminal diagnostic report generation and export for TUI.
//!
//! Generates a structured system diagnostics summary (hardware, telemetry snapshot,
//! subsystems, resource usage, and active alerts) with core-audited redaction of
//! sensitive paths, usernames, and IP addresses. Exports to a specified path or
//! defaults to `/tmp/taskforest-diagnostic.txt` with status feedback rendered in the footer.

use std::path::{Path, PathBuf};

use taskmanager_application::i18n::t;
use taskmanager_core::core::diagnostics::{
    DiagnosticBundlePlan, DiagnosticSource, RedactionSummary,
};
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_shell::presentation::{bytes, duration, missing_value};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use crate::TuiApp;

/// The logical source name under which the diagnostics text is submitted to core.
const DIAGNOSTIC_SOURCE_NAME: &str = "taskforest-diagnostic.txt";

/// Default diagnostic report export filename.
pub const DEFAULT_DIAGNOSTIC_FILENAME: &str = "taskforest-diagnostic.txt";

/// Redact text using the core diagnostics contract.
fn redact_diagnostic_text(
    text: &str,
    usernames: impl IntoIterator<Item = String>,
) -> (String, RedactionSummary) {
    match DiagnosticBundlePlan::prepare(
        vec![DiagnosticSource {
            name: DIAGNOSTIC_SOURCE_NAME.to_string(),
            contents: text.to_string(),
        }],
        usernames,
    ) {
        Ok(plan) => {
            let summary = plan.preview().redactions;
            let sanitized = plan
                .sanitized_contents(DIAGNOSTIC_SOURCE_NAME)
                .unwrap_or(text)
                .to_string();
            (sanitized, summary)
        }
        Err(_) => (text.to_string(), RedactionSummary::default()),
    }
}

/// Format a comprehensive, privacy-safe diagnostic summary report.
#[must_use]
pub fn format_diagnostic_summary(
    hardware: Option<&HardwareInfo>,
    snapshot: Option<&SystemSnapshot>,
    usernames: impl IntoIterator<Item = String>,
) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str(
        "================================================================================\n",
    );
    out.push_str("TaskForest Terminal Diagnostic Report\n");
    out.push_str(
        "================================================================================\n\n",
    );

    out.push_str("## System Information\n");
    if let Some(hw) = hardware {
        let os = format!(
            "{} {}",
            hw.os_name.as_deref().unwrap_or("Unknown OS"),
            hw.os_version.as_deref().unwrap_or("")
        );
        out.push_str(&format!("OS: {}\n", os.trim()));
        out.push_str(&format!(
            "Kernel: {}\n",
            hw.kernel_version.as_deref().unwrap_or("Unknown")
        ));
        out.push_str(&format!(
            "Hostname: {}\n",
            hw.hostname.as_deref().unwrap_or("—")
        ));
        out.push_str(&format!(
            "CPU Brand: {}\n",
            hw.cpu_brand.as_deref().unwrap_or("Unknown")
        ));
        out.push_str(&format!(
            "CPU Cores: {}\n",
            hw.cpu_cores.map_or_else(missing_value, |c| c.to_string())
        ));
    } else {
        out.push_str("Hardware: Unavailable\n");
    }

    if let Some(snap) = snapshot {
        out.push_str(&format!("Uptime: {}\n", duration(snap.uptime_secs)));
        out.push_str(&format!("Processes: {}\n", snap.processes));
        if let Some(threads) = snap.threads {
            out.push_str(&format!("Threads: {threads}\n"));
        }
        let cpu_usage = snap
            .cpu
            .current_global_usage_pct()
            .filter(|v| v.is_finite())
            .map_or_else(missing_value, |v| format!("{v:.1}%"));
        out.push_str(&format!("CPU Total Usage: {cpu_usage}\n"));
        let memory = match (
            snap.memory.current_used_bytes(),
            snap.memory.current_total_bytes(),
        ) {
            (Some(used), Some(total)) => format!("{} / {}", bytes(used), bytes(total)),
            _ => missing_value(),
        };
        out.push_str(&format!("Memory: {memory}\n"));
        let swap = match (
            snap.memory.current_swap_used_bytes(),
            snap.memory.current_swap_total_bytes(),
        ) {
            (Some(used), Some(total)) => format!("{} / {}", bytes(used), bytes(total)),
            _ => missing_value(),
        };
        out.push_str(&format!("Swap: {swap}\n"));
        if let Some(load) = &snap.load_average {
            out.push_str(&format!(
                "Load Average: {:.2}, {:.2}, {:.2}\n",
                load.one_minute, load.five_minutes, load.fifteen_minutes
            ));
        }

        if !snap.disks.is_empty() {
            out.push_str("\n## Storage Devices\n");
            for (idx, disk) in snap.disks.iter().enumerate() {
                let cap = disk
                    .current_capacity_bytes()
                    .map_or_else(missing_value, bytes);
                out.push_str(&format!(
                    "[{idx}] {} (Model: {}, Cap: {}, SMART: {:?})\n",
                    disk.name,
                    if disk.model.is_empty() {
                        "—"
                    } else {
                        &disk.model
                    },
                    cap,
                    disk.smart_availability,
                ));
            }
        }

        if !snap.networks.is_empty() {
            out.push_str("\n## Network Interfaces\n");
            for (idx, nic) in snap.networks.iter().enumerate() {
                let speed = nic
                    .current_link_speed_mbps()
                    .map_or_else(missing_value, |s| format!("{s} Mbps"));
                out.push_str(&format!(
                    "[{idx}] {} (MAC: {}, Speed: {})\n",
                    nic.interface_name,
                    nic.mac_addr.as_deref().unwrap_or("—"),
                    speed,
                ));
            }
        }

        if !snap.gpu.is_empty() {
            out.push_str("\n## GPU Accelerators\n");
            for (idx, gpu) in snap.gpu.iter().enumerate() {
                let vram = gpu
                    .current_memory_total_bytes()
                    .map_or_else(missing_value, bytes);
                out.push_str(&format!(
                    "[{idx}] {} (Driver: {}, VRAM: {})\n",
                    gpu.brand,
                    gpu.driver.as_deref().unwrap_or("—"),
                    vram,
                ));
            }
        }
    } else {
        out.push_str("Snapshot: Unavailable\n");
    }

    let (redacted, summary) = redact_diagnostic_text(&out, usernames);
    let mut final_report = redacted;
    final_report.push_str("\n## Redaction Summary\n");
    final_report.push_str(&format!(
        "Total redactions: {} (usernames: {}, paths: {}, IPv4: {}, IPv6: {})\n",
        summary.total(),
        summary.usernames,
        summary.paths,
        summary.ipv4_addresses,
        summary.ipv6_addresses,
    ));
    final_report
}

/// Resolve the default diagnostic export destination path.
/// Prefers `app.export_dir/taskforest-diagnostic.txt` if configured;
/// otherwise defaults to `/tmp/taskforest-diagnostic.txt` (or system temp directory).
#[must_use]
pub fn default_diagnostic_path(export_dir: Option<&Path>) -> PathBuf {
    export_dir.map_or_else(
        || {
            let tmp = PathBuf::from("/tmp");
            if tmp.is_dir() {
                tmp.join(DEFAULT_DIAGNOSTIC_FILENAME)
            } else {
                std::env::temp_dir().join(DEFAULT_DIAGNOSTIC_FILENAME)
            }
        },
        |dir| dir.join(DEFAULT_DIAGNOSTIC_FILENAME),
    )
}

impl TuiApp {
    /// Format the system diagnostics summary report with core redactions applied.
    #[must_use]
    pub fn format_diagnostic_report(&self) -> String {
        let hardware = self.projection().hardware.as_ref();
        let snapshot = self.projection().snapshot.as_ref();
        let usernames: Vec<String> = self
            .projection()
            .processes
            .as_deref()
            .map_or(&[] as &[ProcessItem], |vec| vec.as_slice())
            .iter()
            .filter_map(|p| p.current_user())
            .collect();
        format_diagnostic_summary(hardware, snapshot, usernames)
    }

    /// Export the formatted diagnostic summary to `path` and report footer feedback.
    pub fn export_diagnostic_report_to(
        &mut self,
        path: impl Into<PathBuf>,
    ) -> Result<PathBuf, std::io::Error> {
        let destination = path.into();
        let report = self.format_diagnostic_report();
        match std::fs::write(&destination, report) {
            Ok(()) => {
                let msg =
                    t("diagnostics.complete").replace("{path}", &destination.display().to_string());
                self.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    msg,
                );
                Ok(destination)
            }
            Err(err) => {
                let msg =
                    t("diagnostics.failed_detail").replace("{reason}", t("diagnostics.failure_io"));
                self.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    msg,
                );
                Err(err)
            }
        }
    }

    /// Export the formatted diagnostic summary report to `export_dir/taskforest-diagnostic.txt`
    /// or `/tmp/taskforest-diagnostic.txt`.
    pub fn export_diagnostic_report(&mut self) -> Result<PathBuf, std::io::Error> {
        let destination = default_diagnostic_path(self.export_dir.as_deref());
        self.export_diagnostic_report_to(destination)
    }
}

#[cfg(test)]
#[path = "../tests/headless/diagnostic_report_tests.rs"]
mod tests;
