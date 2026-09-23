//! GPU and capability rows for process-insight previews.

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use taskmanager_application::i18n::t;
use taskmanager_core::core::process_telemetry::{ProcessGpuDevice, ProcessGpuEngineUsage};
use taskmanager_shell::presentation::{bytes, missing_value};

use crate::TuiTheme;

/// Compact GPU device line: `GPU #<id> <util> · VRAM in use <bytes>`.
pub(crate) fn format_gpu_device_row(device: &ProcessGpuDevice) -> String {
    let vram = device.memory_bytes.map_or_else(missing_value, bytes);
    let util = device
        .utilization_pct
        .map_or_else(missing_value, |value| format!("{value:.1}%"));
    format!(
        "{} #{} {} · {} {}",
        t("common.gpu"),
        device.device_id,
        util,
        t("gpu.vram_in_use"),
        vram,
    )
}

/// Format one engine's cumulative busy time as seconds.
pub(crate) fn format_engine_time(nanoseconds: u64) -> String {
    let seconds = nanoseconds as f64 / 1_000_000_000.0;
    format!("{seconds:.1}s")
}

/// Format a cumulative cycle counter (xe fdinfo) as a compact count.
pub(crate) fn format_engine_cycles(cycles: u64) -> String {
    if cycles >= 1_000_000_000 {
        format!("{:.2}G cycles", cycles as f64 / 1_000_000_000.0)
    } else if cycles >= 1_000_000 {
        format!("{:.1}M cycles", cycles as f64 / 1_000_000.0)
    } else {
        format!("{cycles} cycles")
    }
}

/// Compact per-engine line: `name  usage%  cumulative`. The current usage is a
/// typed [`ScalarObservation`] — the cold-start first sample and any counter
/// rollback are a typed gap, rendered as an explicit dash rather than a
/// fabricated `0.0%`. Cumulative DRM busy time or cycles are shown when observed;
/// if neither is present, it renders an explicit dash.
pub(crate) fn format_engine_usage_line(engine: &ProcessGpuEngineUsage) -> String {
    let usage = engine
        .usage_pct
        .current_value()
        .map_or_else(missing_value, |value| format!("{value:.1}%"));
    let cumulative = engine
        .engine_time_ns
        .current_value()
        .map(|value| format_engine_time(*value))
        .or_else(|| {
            engine
                .engine_cycles
                .current_value()
                .map(|value| format_engine_cycles(*value))
        })
        .unwrap_or_else(missing_value);
    format!("{}  {usage}  {cumulative}", engine.name)
}

/// Whether a capability is considered high-privilege / dangerous (e.g. `CAP_SYS_ADMIN`, `CAP_SYS_PTRACE`).
/// Matches case-insensitively and handles both `CAP_` prefixed and bare capability names.
pub(crate) fn is_dangerous_capability(cap: &str) -> bool {
    let name = cap.trim().to_ascii_uppercase();
    let stripped = name.strip_prefix("CAP_").unwrap_or(&name);
    matches!(
        stripped,
        "SYS_ADMIN"
            | "SYS_PTRACE"
            | "SYS_RAWIO"
            | "SYS_MODULE"
            | "SYS_BOOT"
            | "SYS_CHROOT"
            | "NET_ADMIN"
            | "NET_RAW"
            | "DAC_OVERRIDE"
            | "DAC_READ_SEARCH"
            | "FOWNER"
            | "SETUID"
            | "SETGID"
            | "SETPCAP"
            | "BPF"
            | "PERFMON"
    ) || stripped.contains("SYS_ADMIN")
        || stripped.contains("SYS_PTRACE")
}

/// Format one capability entry as a styled terminal line. Dangerous capabilities are highlighted
/// with a visible warning marker `[!] ⚠` in the theme's warning color with bold styling.
pub(crate) fn format_capability_line(cap: &str, theme: TuiTheme) -> ratatui::text::Line<'static> {
    if is_dangerous_capability(cap) {
        ratatui::text::Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "[!] \u{26a0} ",
                Style::new().fg(theme.warn).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                cap.to_string(),
                Style::new().fg(theme.warn).add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        ratatui::text::Line::from(format!("    {cap}"))
    }
}

/// Bounded Capabilities preview: the count title, then up to `limit` capability rows
/// with dangerous capabilities visibly highlighted, and an honest "…" when more remain.
pub(crate) fn capabilities_preview_lines_with_limit(
    capabilities: &[String],
    theme: TuiTheme,
    limit: usize,
) -> Vec<ratatui::text::Line<'static>> {
    let mut out = Vec::new();
    if capabilities.is_empty() {
        return out;
    }
    out.push(ratatui::text::Line::from(format!(
        "  {} {}",
        t("proc_insights.capabilities"),
        capabilities.len()
    )));
    for cap in capabilities.iter().take(limit) {
        out.push(format_capability_line(cap, theme));
    }
    if capabilities.len() > limit {
        out.push(ratatui::text::Line::from(Span::styled(
            "    …",
            Style::new().fg(theme.dim),
        )));
    }
    out
}

use taskmanager_core::core::process_telemetry::{
    OpenFileEntry, ProcessEnvironment, ProcessEnvironmentEntry, ProcessOpenFiles,
};
/// Compact open-file row: `fd [kind] → target`. A descriptor whose readlink failed
/// (`target: None`) surfaces the typed unreadable marker, never a blank or a
/// fabricated path.
pub(crate) fn format_open_file_row(entry: &OpenFileEntry, unreadable: &str) -> String {
    let target = entry
        .target
        .clone()
        .unwrap_or_else(|| unreadable.to_string());
    if entry.deleted {
        format!(
            "{} [{}] → {} [deleted]",
            entry.fd,
            entry.resolved_kind(),
            target
        )
    } else {
        format!("{} [{}] → {}", entry.fd, entry.resolved_kind(), target)
    }
}

/// Bounded Open-files-facet preview: the entry count (plus an "N unreadable"
/// marker when readlink failed for any descriptor), then the first
/// `limit` `fd → target` rows and an honest "…" when more remain.
pub(crate) fn open_files_preview_lines_with_limit(
    open_files: &ProcessOpenFiles,
    theme: TuiTheme,
    limit: usize,
) -> Vec<ratatui::text::Line<'static>> {
    let unreadable_label = t("proc_insights.unreadable");
    let mut out = Vec::new();
    if open_files.entries.is_empty() {
        out.push(ratatui::text::Line::from(Span::styled(
            format!("  {}", t("proc_insights.no_open_files")),
            Style::new().fg(theme.dim),
        )));
        return out;
    }
    let header = if open_files.unreadable_count > 0 {
        format!(
            "  {} {} · {} {}",
            t("proc_insights.open_files"),
            open_files.entries.len(),
            open_files.unreadable_count,
            unreadable_label,
        )
    } else {
        format!(
            "  {} {}",
            t("proc_insights.open_files"),
            open_files.entries.len()
        )
    };
    out.push(ratatui::text::Line::from(header));
    for entry in open_files.entries.iter().take(limit) {
        out.push(ratatui::text::Line::from(format!(
            "    {}",
            format_open_file_row(entry, unreadable_label)
        )));
    }
    if open_files.entries.len() > limit {
        out.push(ratatui::text::Line::from(Span::styled(
            "    …",
            Style::new().fg(theme.dim),
        )));
    }
    out
}

/// Format one environment entry as `key=escaped_value`. Newlines and carriage returns
/// are escaped to keep each entry on a single terminal row.
pub(crate) fn format_env_entry(entry: &ProcessEnvironmentEntry) -> String {
    taskmanager_application::process_details_vm::format_env_entry(entry)
}

/// Bounded Environment-facet preview: the entry count, then the first
/// `limit` `key=value` rows and an honest "…" when more remain.
pub(crate) fn environment_preview_lines_with_limit(
    env: &ProcessEnvironment,
    theme: TuiTheme,
    limit: usize,
) -> Vec<ratatui::text::Line<'static>> {
    let mut out = Vec::new();
    if env.entries.is_empty() {
        out.push(ratatui::text::Line::from(Span::styled(
            format!("  {}", t("prop.environment_empty")),
            Style::new().fg(theme.dim),
        )));
        return out;
    }
    out.push(ratatui::text::Line::from(format!(
        "  {} {}",
        t("prop.environment"),
        env.entries.len()
    )));
    for entry in env.entries.iter().take(limit) {
        out.push(ratatui::text::Line::from(format!(
            "    {}",
            format_env_entry(entry)
        )));
    }
    if env.entries.len() > limit || env.truncated_count > 0 {
        out.push(ratatui::text::Line::from(Span::styled(
            "    …",
            Style::new().fg(theme.dim),
        )));
    }
    out
}
