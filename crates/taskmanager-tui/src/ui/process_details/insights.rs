//! Bounded `ProcessInsights` card rendering for the selected-process detail
//! panel and the Process Properties modal's Insights tab. Extracted verbatim
//! from `process_details.rs` to keep the renderer under the source line budget.
//! `insights_lines` stays reachable at `super::insights_lines` /
//! `crate::ui::process_details::insights_lines` via a `pub(crate) use` in the
//! parent module.

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use taskmanager_application::{
    ConnectionEndpoint, ConnectionTransport, FailureKind, LimitValue, OpenFileEntry,
    ProcessInsightUnavailable, ProcessOpenFiles, ProcessThreadInfo, ProcessThreads,
    ScalarObservation, i18n::t,
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
                let vram = device.memory_bytes.map_or_else(missing_value, bytes);
                let util = device
                    .utilization_pct
                    .map_or_else(missing_value, |value| format!("{value:.1}%"));
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {} · {} {}",
                    t("common.gpu"),
                    util,
                    t("gpu.vram_in_use"),
                    vram,
                )));
            }
            if snapshot.devices.len() > 2 {
                lines.push(ratatui::text::Line::from(Span::styled(
                    "  …",
                    Style::new().fg(theme.dim),
                )));
            }
            // Per-engine breakdown (drm-engine fdinfo, deeper-indented under
            // the device rollup): name + current usage_pct. An empty engine
            // list renders nothing fabricated — a live, non-GPU process.
            for engine in snapshot.engines.engines.iter().take(GPU_ENGINES_PREVIEW) {
                lines.push(ratatui::text::Line::from(format!(
                    "    {}",
                    format_engine_usage_line(&engine.name, &engine.usage_pct)
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
        }
    }
    // Isolation: one honest line.
    match &projection.isolation {
        ProcessInsightFacetState::Pending => lines.push(collecting()),
        ProcessInsightFacetState::Unavailable(reason) => {
            lines.push(insight_unavailable(theme, reason))
        }
        ProcessInsightFacetState::Current(isolation) => {
            let detail = isolation
                .container_id
                .as_deref()
                .map_or_else(missing_value, |id| id.to_string());
            lines.push(ratatui::text::Line::from(format!(
                "  {} {}",
                t("proc_insights.container_id"),
                detail,
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
    use taskmanager_application::SubmissionErrorKind;
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

/// Compact per-engine line: `name usage%`. The current usage is a typed
/// [`ScalarObservation`] — the cold-start first sample and any counter rollback
/// are a typed gap, rendered as an explicit dash rather than a fabricated
/// `0.0%`.
fn format_engine_usage_line(name: &str, usage_pct: &ScalarObservation<f32>) -> String {
    let usage = usage_pct
        .current_value()
        .map_or_else(missing_value, |value| format!("{value:.1}%"));
    format!("{name} {usage}")
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/process_details/insights_tests.rs"]
mod tests;
