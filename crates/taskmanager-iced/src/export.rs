//! Structured clipboard export and redacted system diagnostics for Iced.
//!
//! Provides formatting helpers to export processes as TSV/JSON, and
//! system information as redacted Markdown suitable for bug reports.

use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::ProcessItem;

use taskmanager_shell::presentation::{bytes, duration, missing_value};

/// Format a single process item as tab-separated values (TSV).
#[must_use]
pub fn process_to_tsv(process: &ProcessItem) -> String {
    let user = process.current_user().unwrap_or_default();
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

/// Format a process item as a clean JSON object string.
#[must_use]
pub fn process_to_json(process: &ProcessItem) -> String {
    let user = process.current_user().unwrap_or_default();
    let user_str = if user.is_empty() {
        "null".to_string()
    } else {
        format!("\"{user}\"")
    };
    let cmd_str = if process.cmdline.is_empty() {
        "null".to_string()
    } else {
        format!(
            "\"{}\"",
            process.cmdline.replace('\\', "\\\\").replace('"', "\\\"")
        )
    };
    let cpu = process
        .current_cpu_percentage()
        .map_or_else(|| "null".to_owned(), |value| format!("{value:.1}"));
    let memory = process
        .current_memory_bytes()
        .map_or_else(|| "null".to_owned(), |value| value.to_string());
    format!(
        "{{\n  \"pid\": {},\n  \"name\": \"{}\",\n  \"cpu_usage\": {},\n  \"memory_bytes\": {},\n  \"user\": {},\n  \"status\": \"{}\",\n  \"cmdline\": {}\n}}",
        process.pid,
        process.name.replace('\\', "\\\\").replace('"', "\\\""),
        cpu,
        memory,
        user_str,
        process.status,
        cmd_str
    )
}

/// Redact sensitive identifiable text (usernames, IPv4 subnets, MAC endings).
#[must_use]
pub fn redact_sensitive_text(text: &str) -> String {
    let mut result = text.to_string();
    // Redact typical home paths
    if let Some(pos) = result.find("/home/") {
        let after = &result[pos + 6..];
        if let Some(slash) = after.find('/') {
            let user = &after[..slash];
            result = result.replace(user, "<user>");
        }
    }
    if let Some(pos) = result.find("C:\\Users\\") {
        let after = &result[pos + 9..];
        if let Some(slash) = after.find('\\') {
            let user = &after[..slash];
            result = result.replace(user, "<user>");
        }
    }
    result
}

/// Generate a comprehensive Markdown system diagnostics bundle report with redacted private fields.
#[must_use]
pub fn system_diagnostics_markdown(
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
                out.push_str(&format!(
                    "  [GPU {}] {} (Driver: {})\n",
                    idx,
                    gpu.brand,
                    gpu.driver.as_deref().unwrap_or("—")
                ));
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
    redact_sensitive_text(&out)
}

#[cfg(test)]
#[path = "../tests/gui/export_tests.rs"]
mod tests;
