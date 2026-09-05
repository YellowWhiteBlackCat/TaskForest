//! Bounded `ProcessInsights` card rendering for the selected-process detail
//! panel and the Process Properties modal's Insights tab. Extracted verbatim
//! from `process_details.rs` to keep the renderer under the source line budget.
//! `insights_lines` stays reachable at `super::insights_lines` /
//! `crate::ui::process_details::insights_lines` via a `pub(crate) use` in the
//! parent module.

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use taskmanager_application::{ProcessInsightUnavailable, i18n::t};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process_telemetry::{
    ConnectionEndpoint, ConnectionTransport, LimitValue, OpenFileEntry, ProcessEnvironment,
    ProcessEnvironmentEntry, ProcessGpuDevice, ProcessGpuEngineUsage, ProcessOpenFiles,
    ProcessThreadInfo, ProcessThreads,
};
use taskmanager_shell::presentation::{bytes, missing_value};

use crate::TuiTheme;

/// Whether the network facet for `pid` reports the typed
/// `RequiresEscalation` state — the one facet whose unavailability the
/// per-feature escalation seam (ADR-023) can reach. The `e` trigger key and
/// the rendered authorization hint both gate on this, so the keyboard can
/// never fire an escalation prompt the current projection did not ask for.
pub(crate) fn network_requires_escalation(app: &crate::TuiApp, pid: u32) -> bool {
    use taskmanager_application::ProcessInsightFacetState;
    app.projection()
        .process_insights
        .as_ref()
        .is_some_and(|projection| {
            projection.target.pid == pid
                && matches!(
                    projection.network,
                    ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
                        FailureKind::RequiresEscalation
                    ))
                )
        })
}

/// Bounded insight cards for the selected process, projected from the shared
/// `SystemProjectionStore.process_insights` (last-wins application projection). A missing
/// or mismatched projection renders an honest "collecting" line; every facet
/// distinguishes Pending / Unavailable / Current — never a fabricated idle.
///
/// Shared by the inline selected-process detail panel AND the Process
/// Properties modal's Insights tab (`ui::process_properties`) so the two views
/// never drift apart — one renderer, one honest projection mapping.
pub(crate) fn insights_lines(
    app: &crate::TuiApp,
    theme: TuiTheme,
    pid: u32,
) -> Vec<ratatui::text::Line<'static>> {
    use taskmanager_application::ProcessInsightFacetState;
    let collecting = || {
        ratatui::text::Line::from(Span::styled(
            t("proc_insights.loading"),
            Style::new().fg(theme.dim),
        ))
    };
    let Some(projection) = app.projection().process_insights.as_ref() else {
        return vec![collecting()];
    };
    if projection.target.pid != pid {
        return vec![collecting()];
    }
    let mut lines = Vec::new();
    lines.push(ratatui::text::Line::from(Span::styled(
        t("prop.insights"),
        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
    )));
    // Network connections: count plus the first three endpoints. An
    // escalation-requiring capture renders the typed reason line plus the
    // `e` trigger hint (G-04b) — never the Debug formatting of the reason.
    match &projection.network {
        ProcessInsightFacetState::Pending => lines.push(collecting()),
        ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
            FailureKind::RequiresEscalation,
        )) => {
            lines.push(ratatui::text::Line::from(Span::styled(
                format!("  {}", t("proc_insights.network_requires_escalation")),
                Style::new().fg(theme.warn),
            )));
            lines.push(ratatui::text::Line::from(format!(
                "  e · {} ({})",
                t("proc_insights.enable_network_capture"),
                t("proc_insights.network_escalation_hint"),
            )));
        }
        ProcessInsightFacetState::Unavailable(reason) => {
            lines.push(insight_unavailable(theme, reason))
        }
        ProcessInsightFacetState::Current(snapshot) => {
            let rx = snapshot
                .rx_bytes_per_sec
                .map_or_else(missing_value, |v| format!("{}/s", bytes(v)));
            let tx = snapshot
                .tx_bytes_per_sec
                .map_or_else(missing_value, |v| format!("{}/s", bytes(v)));
            lines.push(ratatui::text::Line::from(format!(
                "  {} RX {} · TX {}",
                t("common.throughput"),
                rx,
                tx
            )));
            lines.push(ratatui::text::Line::from(format!(
                "  {} {}",
                t("proc_insights.connections"),
                snapshot.connections.len()
            )));
            for connection in snapshot.connections.iter().take(3) {
                lines.push(ratatui::text::Line::from(format!(
                    "    {} {} -> {}",
                    transport_text(&connection.transport),
                    endpoint_text(&connection.local),
                    endpoint_text(&connection.remote),
                )));
            }
            if snapshot.connections.len() > 3 {
                lines.push(ratatui::text::Line::from(Span::styled(
                    "    …",
                    Style::new().fg(theme.dim),
                )));
            }
        }
    }
    // GPU: engine time / VRAM per device.
    match &projection.gpu {
        ProcessInsightFacetState::Pending => lines.push(collecting()),
        ProcessInsightFacetState::Unavailable(reason) => {
            lines.push(insight_unavailable(theme, reason))
        }
        ProcessInsightFacetState::Current(snapshot) => {
            for device in snapshot.devices.iter().take(2) {
                lines.push(ratatui::text::Line::from(format!(
                    "  {}",
                    format_gpu_device_row(device)
                )));
            }
            if snapshot.devices.len() > 2 {
                lines.push(ratatui::text::Line::from(Span::styled(
                    "  …",
                    Style::new().fg(theme.dim),
                )));
            }
            // Per-engine breakdown (drm-engine fdinfo, deeper-indented under
            // the device rollup): name + current usage_pct + cumulative time/cycles.
            // An empty engine list renders nothing fabricated — a live, non-GPU process.
            for engine in snapshot.engines.engines.iter().take(GPU_ENGINES_PREVIEW) {
                lines.push(ratatui::text::Line::from(format!(
                    "    {}",
                    format_engine_usage_line(engine)
                )));
            }
            if snapshot.engines.engines.len() > GPU_ENGINES_PREVIEW {
                lines.push(ratatui::text::Line::from(Span::styled(
                    "    …",
                    Style::new().fg(theme.dim),
                )));
            }
        }
    }
    // Resource limits: memory + CPU quota rows.
    match &projection.resources {
        ProcessInsightFacetState::Pending => lines.push(collecting()),
        ProcessInsightFacetState::Unavailable(reason) => {
            lines.push(insight_unavailable(theme, reason))
        }
        ProcessInsightFacetState::Current(snapshot) => {
            lines.push(ratatui::text::Line::from(format!(
                "  {} {}",
                t("proc_insights.resource_limits"),
                limit_value(
                    snapshot.current_memory_limit(),
                    snapshot.current_memory_usage_bytes(),
                ),
            )));
            if let Some(quota) = snapshot.current_cpu_time_quota_micros() {
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("proc_insights.cpu_quota"),
                    limit_value(Some(quota), None),
                )));
            }
            if let Some(pids) = snapshot.current_process_count() {
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("proc_insights.pids"),
                    limit_value(snapshot.current_process_limit(), Some(pids)),
                )));
            }
            if let Some(groups) = snapshot.current_resource_groups()
                && let Some(first) = groups.first()
            {
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("proc_insights.resource_group"),
                    first.native_locator,
                )));
            }
        }
    }
    // Isolation: one honest line.
    match &projection.isolation {
        ProcessInsightFacetState::Pending => lines.push(collecting()),
        ProcessInsightFacetState::Unavailable(reason) => {
            lines.push(insight_unavailable(theme, reason))
        }
        ProcessInsightFacetState::Current(isolation) => {
            let kind_str = isolation
                .kind
                .as_ref()
                .map_or_else(missing_value, |k| format!("{k:?}"));
            lines.push(ratatui::text::Line::from(format!(
                "  {} {}",
                t("proc_insights.isolation"),
                kind_str,
            )));
            let detail = isolation
                .container_id
                .as_deref()
                .map_or_else(missing_value, |id| id.to_string());
            lines.push(ratatui::text::Line::from(format!(
                "  {} {}",
                t("proc_insights.container_id"),
                detail,
            )));
            let sandboxed_str = isolation.sandboxed.map_or_else(missing_value, |s| {
                if s {
                    t("common.yes").to_string()
                } else {
                    t("common.no").to_string()
                }
            });
            lines.push(ratatui::text::Line::from(format!(
                "  {} {}",
                t("proc_insights.sandboxed"),
                sandboxed_str,
            )));
        }
    }
    // Threads: compact column header plus the first N thread rows.
    match &projection.threads {
        ProcessInsightFacetState::Pending => lines.push(collecting()),
        ProcessInsightFacetState::Unavailable(reason) => {
            lines.push(insight_unavailable(theme, reason))
        }
        ProcessInsightFacetState::Current(threads) => {
            lines.extend(thread_preview_lines(threads, theme))
        }
    }
    // Open files: entry count (+ unreadable marker) plus the first N descriptors.
    match &projection.open_files {
        ProcessInsightFacetState::Pending => lines.push(collecting()),
        ProcessInsightFacetState::Unavailable(reason) => {
            lines.push(insight_unavailable(theme, reason))
        }
        ProcessInsightFacetState::Current(open_files) => {
            lines.extend(open_files_preview_lines(open_files, theme))
        }
    }
    // Environment: entry count plus the first N bounded key=value entries.
    match &projection.environment {
        ProcessInsightFacetState::Pending => lines.push(collecting()),
        ProcessInsightFacetState::Unavailable(reason) => {
            lines.push(insight_unavailable(theme, reason))
        }
        ProcessInsightFacetState::Current(env) => {
            lines.extend(environment_preview_lines(env, theme))
        }
    }
    lines
}

/// Typed one-line message for a facet that cannot contribute — never the
/// Debug formatting of the reason (G-04b). Mirrors the iced/gpui status
/// labels: a permission denial (or a capability the per-feature escalation
/// seam could reach) reads "permission denied"; an unsupported facet reads
/// "unsupported by this provider"; everything else keeps the honest generic
/// "unavailable".
fn insight_unavailable(
    theme: TuiTheme,
    reason: &ProcessInsightUnavailable,
) -> ratatui::text::Line<'static> {
    use taskmanager_platform_contract::SubmissionErrorKind;
    let text = match reason {
        ProcessInsightUnavailable::Provider(
            FailureKind::PermissionDenied | FailureKind::RequiresEscalation,
        ) => t("proc_insights.permission_denied"),
        ProcessInsightUnavailable::Provider(FailureKind::Unsupported)
        | ProcessInsightUnavailable::Submission(SubmissionErrorKind::UnsupportedCapability) => {
            t("proc_insights.unsupported_provider")
        }
        _ => t("proc_insights.unavailable"),
    };
    ratatui::text::Line::from(Span::styled(
        format!("  {text}"),
        Style::new().fg(theme.dim),
    ))
}

/// Compact transport label; `Other` carries the provider's own string.
fn transport_text(transport: &ConnectionTransport) -> String {
    match transport {
        ConnectionTransport::Tcp => "TCP".into(),
        ConnectionTransport::Udp => "UDP".into(),
        ConnectionTransport::Sctp => "SCTP".into(),
        ConnectionTransport::Local => "UNIX".into(),
        ConnectionTransport::Other(value) => value.clone(),
    }
}

/// Compact endpoint label: an IP renders its address, a local socket its
/// path, an opaque one its value; an unspecified endpoint renders a dash.
fn endpoint_text(endpoint: &ConnectionEndpoint) -> String {
    match endpoint {
        ConnectionEndpoint::Ip(address) => address.to_string(),
        ConnectionEndpoint::Local { path } => path.clone(),
        ConnectionEndpoint::Opaque { value } => value.clone(),
        ConnectionEndpoint::Unspecified => missing_value(),
    }
}

/// Render a limit row: `Unlimited` is honest "∞"; a value renders the number.
fn limit_value(limit: Option<LimitValue>, current: Option<u64>) -> String {
    match limit {
        Some(LimitValue::Unlimited) => {
            if let Some(current) = current {
                format!("{current} / ∞")
            } else {
                "∞".to_string()
            }
        }
        Some(LimitValue::Value(value)) => {
            if let Some(current) = current {
                format!("{current} / {value}")
            } else {
                value.to_string()
            }
        }
        None => current.map_or_else(missing_value, |value| value.to_string()),
    }
}

/// Bounded preview row counts for the populated insight facets. The detail
/// panel is terminal-width constrained, so each facet shows a small head plus
/// an honest "…" when more remain — the same shape the connections and device
/// previews already use.
const THREADS_PREVIEW: usize = 3;
const OPEN_FILES_PREVIEW: usize = 3;
const GPU_ENGINES_PREVIEW: usize = 3;
const ENVIRONMENT_PREVIEW: usize = 3;

/// Compact thread row: `tid  comm  state  cpu-time  cpu%`. Missing CPU time or
/// percent render an explicit dash, never a fabricated `0.0` — the first
/// sample, a counter rollback, or a clock gap is a typed gap, not a zero. An
/// empty `comm` is the contract's unknown identity (e.g. Windows ToolHelp32
/// exposes no thread names) and renders the same dash.
fn format_thread_row(thread: &ProcessThreadInfo) -> String {
    let cpu_time = thread
        .cpu_time_secs
        .map_or_else(missing_value, |value| format!("{value:.1}s"));
    let cpu_percent = thread
        .cpu_percent
        .map_or_else(missing_value, |value| format!("{value:.1}%"));
    let comm = if thread.comm.is_empty() {
        missing_value()
    } else {
        thread.comm.clone()
    };
    format!(
        "{}  {}  {}  {}  {}",
        thread.tid,
        comm,
        thread.state.as_short_label(),
        cpu_time,
        cpu_percent,
    )
}

/// Bounded Threads-facet preview: a count title, a compact column header, the
/// first [`THREADS_PREVIEW`] rows, and an honest "…" when more remain. An empty
/// thread list renders the explicit empty state, never a fabricated row.
fn thread_preview_lines(
    threads: &ProcessThreads,
    theme: TuiTheme,
) -> Vec<ratatui::text::Line<'static>> {
    let mut out = Vec::new();
    if threads.threads.is_empty() {
        out.push(ratatui::text::Line::from(Span::styled(
            format!("  {}", t("proc_insights.no_threads")),
            Style::new().fg(theme.dim),
        )));
        return out;
    }
    out.push(ratatui::text::Line::from(format!(
        "  {} {}",
        t("proc_insights.threads"),
        threads.threads.len()
    )));
    out.push(ratatui::text::Line::from(Span::styled(
        format!(
            "    TID  Name  St  {}  {}",
            t("proc_insights.thread_cpu_time"),
            t("proc_insights.thread_cpu_percent")
        ),
        Style::new().fg(theme.dim),
    )));
    for thread in threads.threads.iter().take(THREADS_PREVIEW) {
        out.push(ratatui::text::Line::from(format!(
            "    {}",
            format_thread_row(thread)
        )));
    }
    if threads.threads.len() > THREADS_PREVIEW {
        out.push(ratatui::text::Line::from(Span::styled(
            "    …",
            Style::new().fg(theme.dim),
        )));
    }
    out
}

/// Compact open-file row: `fd → target`. A descriptor whose readlink failed
/// (`target: None`) surfaces the typed unreadable marker, never a blank or a
/// fabricated path.
fn format_open_file_row(entry: &OpenFileEntry, unreadable: &str) -> String {
    let target = entry
        .target
        .clone()
        .unwrap_or_else(|| unreadable.to_string());
    format!("{} → {}", entry.fd, target)
}

/// Bounded Open-files-facet preview: the entry count (plus an "N unreadable"
/// marker when readlink failed for any descriptor), then the first
/// [`OPEN_FILES_PREVIEW`] `fd → target` rows and an honest "…" when more
/// remain. A healthy process with no readable descriptors renders the explicit
/// empty state.
fn open_files_preview_lines(
    open_files: &ProcessOpenFiles,
    theme: TuiTheme,
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
    for entry in open_files.entries.iter().take(OPEN_FILES_PREVIEW) {
        out.push(ratatui::text::Line::from(format!(
            "    {}",
            format_open_file_row(entry, unreadable_label)
        )));
    }
    if open_files.entries.len() > OPEN_FILES_PREVIEW {
        out.push(ratatui::text::Line::from(Span::styled(
            "    …",
            Style::new().fg(theme.dim),
        )));
    }
    out
}

/// Format one environment entry as `key=escaped_value`. Newlines and carriage returns
/// are escaped to keep each entry on a single terminal row.
fn format_env_entry(entry: &ProcessEnvironmentEntry) -> String {
    let escaped = entry.value.replace('\r', "\\r").replace('\n', "\\n");
    format!("{}={}", entry.key, escaped)
}

/// Bounded Environment-facet preview: the entry count, then the first
/// [`ENVIRONMENT_PREVIEW`] `key=value` rows and an honest "…" when more
/// remain or entries were dropped by the capture budget. An empty environment
/// renders the explicit empty state.
fn environment_preview_lines(
    env: &ProcessEnvironment,
    theme: TuiTheme,
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
    for entry in env.entries.iter().take(ENVIRONMENT_PREVIEW) {
        out.push(ratatui::text::Line::from(format!(
            "    {}",
            format_env_entry(entry)
        )));
    }
    if env.entries.len() > ENVIRONMENT_PREVIEW || env.truncated_count > 0 {
        out.push(ratatui::text::Line::from(Span::styled(
            "    …",
            Style::new().fg(theme.dim),
        )));
    }
    out
}

/// Compact GPU device line: `GPU #<id> <util> · VRAM in use <bytes>`.
fn format_gpu_device_row(device: &ProcessGpuDevice) -> String {
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
fn format_engine_time(nanoseconds: u64) -> String {
    let seconds = nanoseconds as f64 / 1_000_000_000.0;
    format!("{seconds:.1}s")
}

/// Format a cumulative cycle counter (xe fdinfo) as a compact count.
fn format_engine_cycles(cycles: u64) -> String {
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
fn format_engine_usage_line(engine: &ProcessGpuEngineUsage) -> String {
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

#[cfg(test)]
#[path = "../../../tests/gui/ui/process_details/insights_tests.rs"]
mod tests;
