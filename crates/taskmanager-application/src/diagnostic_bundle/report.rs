//! Markdown diagnostic report formatting.

use super::DiagnosticBundle;

/// Generate a human-readable Markdown diagnostic report.
#[must_use]
pub fn format_markdown_report(bundle: &DiagnosticBundle) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str("# TaskForest Diagnostic Report\n\n");
    out.push_str(&format!("- Schema Version: {}\n", bundle.version));
    out.push_str(&format!("- Timestamp (ms): {}\n\n", bundle.timestamp_ms));

    out.push_str("## System Overview\n");
    let os = format!(
        "{} {}",
        bundle
            .system_overview
            .os_name
            .as_deref()
            .unwrap_or("Unknown OS"),
        bundle.system_overview.os_version.as_deref().unwrap_or("")
    );
    out.push_str(&format!("- OS: {}\n", os.trim()));
    out.push_str(&format!(
        "- Kernel: {}\n",
        bundle
            .system_overview
            .kernel_version
            .as_deref()
            .unwrap_or("Unknown")
    ));
    out.push_str(&format!(
        "- Hostname: {}\n",
        bundle.system_overview.hostname.as_deref().unwrap_or("—")
    ));
    out.push_str(&format!(
        "- Architecture: {}\n",
        bundle
            .system_overview
            .architecture
            .as_deref()
            .unwrap_or("Unknown")
    ));
    out.push_str(&format!(
        "- CPU: {}\n",
        bundle
            .system_overview
            .cpu_brand
            .as_deref()
            .unwrap_or("Unknown")
    ));
    if let Some(cores) = bundle.system_overview.cpu_cores {
        out.push_str(&format!("- CPU Cores: {cores}\n"));
    }
    if let Some(pct) = bundle.system_overview.cpu_usage_pct {
        out.push_str(&format!("- CPU Total Usage: {pct:.1}%\n"));
    }
    if let Some(total) = bundle.system_overview.memory_total_bytes {
        let used_str = bundle
            .system_overview
            .memory_used_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "—".to_owned());
        out.push_str(&format!(
            "- Memory: {} / {}\n",
            used_str,
            format_bytes(total)
        ));
    }
    out.push_str(&format!(
        "- Uptime: {}s\n",
        bundle.system_overview.uptime_secs
    ));
    if let Some(load) = &bundle.system_overview.load_average {
        out.push_str(&format!(
            "- Load Average (1m, 5m, 15m): {:.2}, {:.2}, {:.2}\n",
            load.one_minute, load.five_minutes, load.fifteen_minutes
        ));
    }
    out.push_str(&format!(
        "- Hardware: {} GPUs, {} Disks, {} Networks\n\n",
        bundle.system_overview.gpu_count,
        bundle.system_overview.disks_count,
        bundle.system_overview.networks_count
    ));

    out.push_str("## Platform Capabilities\n");
    let available_count = bundle
        .platform_capabilities
        .capabilities
        .iter()
        .filter(|c| c.status == "available")
        .count();
    out.push_str(&format!(
        "- Total Capabilities: {} (Available: {})\n",
        bundle.platform_capabilities.capabilities.len(),
        available_count
    ));
    for cap in &bundle.platform_capabilities.capabilities {
        let prov = if cap.providers.is_empty() {
            "none"
        } else {
            &cap.providers.join(", ")
        };
        out.push_str(&format!(
            "  - {}: {} (providers: {})\n",
            cap.id, cap.status, prov
        ));
    }
    out.push('\n');

    out.push_str("## Process Summary\n");
    out.push_str(&format!(
        "- Total Processes: {}\n",
        bundle.process_summary.total_processes
    ));
    if let Some(threads) = bundle.process_summary.total_threads {
        out.push_str(&format!("- Total Threads: {threads}\n"));
    }
    out.push_str(&format!(
        "- Distinct Users: {}\n",
        bundle.process_summary.distinct_users
    ));
    out.push_str(&format!(
        "- Anomalies Detected: {}\n",
        bundle.process_summary.anomalies_count
    ));
    out.push_str("- Status Breakdown:\n");
    for (status, count) in &bundle.process_summary.status_counts {
        out.push_str(&format!("  - {status}: {count}\n"));
    }
    if !bundle.process_summary.top_cpu.is_empty() {
        out.push_str("- Top CPU Processes:\n");
        for top in &bundle.process_summary.top_cpu {
            let cpu_str = top
                .cpu_pct
                .map_or_else(|| "—".to_owned(), |c| format!("{c:.1}%"));
            let user_str = top.user.as_deref().unwrap_or("—");
            out.push_str(&format!(
                "  - [{}] {} (user: {user_str}, cpu: {cpu_str})\n",
                top.pid, top.name
            ));
        }
    }
    if !bundle.process_summary.top_memory.is_empty() {
        out.push_str("- Top Memory Processes:\n");
        for top in &bundle.process_summary.top_memory {
            let mem_str = top
                .memory_bytes
                .map_or_else(|| "—".to_owned(), format_bytes);
            let user_str = top.user.as_deref().unwrap_or("—");
            out.push_str(&format!(
                "  - [{}] {} (user: {user_str}, memory: {mem_str})\n",
                top.pid, top.name
            ));
        }
    }
    out.push('\n');

    out.push_str("## Telemetry Health\n");
    out.push_str("- Domains:\n");
    for domain in &bundle.telemetry_health.domains {
        let reason = domain
            .failure_reason
            .as_deref()
            .map_or(String::new(), |r| format!(" ({r})"));
        out.push_str(&format!(
            "  - {}: {}{}\n",
            domain.domain, domain.state, reason
        ));
    }
    if !bundle.telemetry_health.sources.is_empty() {
        out.push_str("- Sources:\n");
        for src in &bundle.telemetry_health.sources {
            out.push_str(&format!(
                "  - {}: {} (items: {})\n",
                src.provider, src.outcome, src.item_count
            ));
        }
    }
    if !bundle.telemetry_health.providers.is_empty() {
        out.push_str("- Providers:\n");
        for prov in &bundle.telemetry_health.providers {
            out.push_str(&format!("  - {}: {}\n", prov.provider, prov.status));
        }
    }

    out
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
