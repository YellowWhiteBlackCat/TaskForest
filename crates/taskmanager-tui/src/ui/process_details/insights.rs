//! Bounded `ProcessInsights` card rendering for the selected-process detail
//! panel and the Process Properties modal's Insights tab. Extracted verbatim
//! from `process_details.rs` to keep the renderer under the source line budget.
//! `insights_lines` stays reachable at `super::insights_lines` /
//! `crate::ui::process_details::insights_lines` via a `pub(crate) use` in the
//! parent module.

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use taskmanager_application::{
    ProcessInsightFacet, ProcessInsightUnavailable, i18n::t, project_process_resources,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process_telemetry::{
    ConnectionEndpoint, ConnectionTransport, ProcessThreadInfo, ProcessThreads,
};
use taskmanager_shell::presentation::{bytes, missing_value, process_resource_rows};

use crate::TuiTheme;
use taskmanager_core::core::process::ProcessLiveKey;

mod formatting;
pub(crate) use formatting::{
    capabilities_preview_lines_with_limit, environment_preview_lines_with_limit,
    format_engine_usage_line, format_gpu_device_row, open_files_preview_lines_with_limit,
};
use taskmanager_application::ProcessAffinityState;
use taskmanager_core::core::process_telemetry::ThreadWaitKind;
use taskmanager_shell::presentation::{
    capabilities_summary, namespaces_summary, network_connection_counters_summary,
    sandbox_details_summary,
};

/// Whether the network facet for the exact live identity reports the typed
/// `RequiresEscalation` state — the one facet whose unavailability the
/// per-feature escalation seam (ADR-023) can reach. The `e` trigger key and
/// the rendered authorization hint both gate on this, so the keyboard can
/// never fire an escalation prompt the current projection did not ask for.
pub(crate) fn network_requires_escalation(app: &crate::TuiApp, identity: ProcessLiveKey) -> bool {
    use taskmanager_application::ProcessInsightFacetState;
    app.projection()
        .process_insights
        .as_ref()
        .is_some_and(|projection| {
            projection.target.live_key() == Some(identity)
                && match &projection.network {
                    ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
                        FailureKind::RequiresEscalation,
                    )) => true,
                    ProcessInsightFacetState::Current(value) => {
                        value.traffic_failure == Some(FailureKind::RequiresEscalation)
                    }
                    _ => false,
                }
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
    identity: ProcessLiveKey,
) -> Vec<ratatui::text::Line<'static>> {
    insights_lines_with_limit(app, theme, identity, THREADS_PREVIEW, None)
}

/// Rich process-insights representation for the dedicated Process Properties
/// modal: renders complete facet lists with scroll navigation rather than
/// the compact 3-row limit of the fixed inline bottom panel.
pub(crate) fn modal_insights_lines(
    app: &crate::TuiApp,
    theme: TuiTheme,
    identity: ProcessLiveKey,
    facet: ProcessInsightFacet,
) -> Vec<ratatui::text::Line<'static>> {
    insights_lines_with_limit(app, theme, identity, usize::MAX, Some(facet))
}

pub(crate) fn insights_lines_with_limit(
    app: &crate::TuiApp,
    theme: TuiTheme,
    identity: ProcessLiveKey,
    limit: usize,
    facet: Option<ProcessInsightFacet>,
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
    if projection.target.live_key() != Some(identity) {
        return vec![collecting()];
    }
    let selected = |candidate| facet.is_none_or(|facet| facet == candidate);
    let mut lines = Vec::new();
    lines.push(ratatui::text::Line::from(Span::styled(
        t("prop.insights"),
        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
    )));
    // Network connections: count plus bounded endpoint list. An
    // escalation-requiring capture renders the typed reason line plus the
    // `e` trigger hint (G-04b) — never the Debug formatting of the reason.
    if selected(ProcessInsightFacet::Network) {
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
                if snapshot.traffic_failure == Some(FailureKind::RequiresEscalation) {
                    lines.push(ratatui::text::Line::from(format!(
                        "e · {}",
                        t("proc_insights.enable_network_capture")
                    )));
                }
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
                if let Some(counters) = snapshot.connection_counters.as_ref()
                    && let Some(summary) = network_connection_counters_summary(counters)
                {
                    lines.push(ratatui::text::Line::from(format!("  {summary}")));
                }
                for connection in snapshot.connections.iter().take(limit) {
                    let rtt = connection
                        .rtt_ms
                        .filter(|value| value.is_finite() && *value >= 0.0)
                        .map_or_else(String::new, |value| {
                            format!(" · {} {value:.1} ms", t("net.rtt"))
                        });
                    lines.push(ratatui::text::Line::from(format!(
                        "    {} [{} · {}] {} -> {}{}",
                        transport_text(&connection.transport),
                        connection.state,
                        if connection.is_loopback() {
                            "LOOPBACK"
                        } else {
                            "EXTERNAL"
                        },
                        endpoint_text(&connection.local),
                        endpoint_text(&connection.remote),
                        rtt,
                    )));
                }
                if snapshot.connections.len() > limit {
                    lines.push(ratatui::text::Line::from(Span::styled(
                        "    …",
                        Style::new().fg(theme.dim),
                    )));
                }
            }
        }
    }
    // GPU: engine time / VRAM per device.
    if selected(ProcessInsightFacet::Gpu) {
        match &projection.gpu {
            ProcessInsightFacetState::Pending => lines.push(collecting()),
            ProcessInsightFacetState::Unavailable(reason) => {
                lines.push(insight_unavailable(theme, reason))
            }
            ProcessInsightFacetState::Current(snapshot) => {
                for device in snapshot.devices.iter().take(limit.max(2)) {
                    lines.push(ratatui::text::Line::from(format!(
                        "  {}",
                        format_gpu_device_row(device)
                    )));
                }
                if snapshot.devices.len() > limit.max(2) {
                    lines.push(ratatui::text::Line::from(Span::styled(
                        "  …",
                        Style::new().fg(theme.dim),
                    )));
                }
                // Per-engine breakdown (drm-engine fdinfo, deeper-indented under
                // the device rollup): name + current usage_pct + cumulative time/cycles.
                // An empty engine list renders nothing fabricated — a live, non-GPU process.
                for engine in snapshot.engines.engines.iter().take(limit) {
                    lines.push(ratatui::text::Line::from(format!(
                        "    {}",
                        format_engine_usage_line(engine)
                    )));
                }
                if snapshot.engines.engines.len() > limit {
                    lines.push(ratatui::text::Line::from(Span::styled(
                        "    …",
                        Style::new().fg(theme.dim),
                    )));
                }
            }
        }
    }
    // Resource limits: memory + CPU quota rows.
    if selected(ProcessInsightFacet::Resources) {
        match &projection.resources {
            ProcessInsightFacetState::Pending => lines.push(collecting()),
            ProcessInsightFacetState::Unavailable(reason) => {
                lines.push(insight_unavailable(theme, reason))
            }
            ProcessInsightFacetState::Current(snapshot) => {
                for (label, value) in process_resource_rows(snapshot) {
                    lines.push(ratatui::text::Line::from(format!("  {label} {value}")));
                }
                if let Some(groups) = snapshot.current_resource_groups() {
                    let mut capabilities = Vec::new();
                    for group in groups {
                        for cap in &group.capabilities {
                            if !capabilities.contains(cap) {
                                capabilities.push(cap.clone());
                            }
                        }
                    }
                    if !capabilities.is_empty() {
                        lines.extend(capabilities_preview_lines_with_limit(
                            &capabilities,
                            theme,
                            limit,
                        ));
                    }
                }
            }
        }
    }
    // Isolation: one honest line.
    if selected(ProcessInsightFacet::Isolation) {
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
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("proc_insights.security_profile"),
                    isolation
                        .security_profile
                        .as_deref()
                        .unwrap_or_else(|| t("proc_insights.unknown")),
                )));
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("proc_insights.seccomp"),
                    isolation.seccomp_mode.map_or_else(
                        || t("proc_insights.unknown").to_owned(),
                        |mode| mode.to_string(),
                    ),
                )));
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("proc_insights.no_new_privs"),
                    isolation.no_new_privs.map_or_else(
                        || t("proc_insights.unknown").to_owned(),
                        |enabled| t(if enabled { "common.yes" } else { "common.no" }).to_owned(),
                    ),
                )));
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("proc_insights.ptrace_scope"),
                    isolation.yama_ptrace_scope.map_or_else(
                        || t("proc_insights.unknown").to_owned(),
                        |scope| scope.to_string(),
                    ),
                )));
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("proc_insights.capabilities"),
                    isolation
                        .capabilities
                        .as_ref()
                        .map(capabilities_summary)
                        .unwrap_or_else(|| t("proc_insights.unknown").to_owned()),
                )));
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("proc_insights.namespaces"),
                    isolation
                        .namespaces
                        .as_ref()
                        .map(namespaces_summary)
                        .unwrap_or_else(|| t("proc_insights.unknown").to_owned()),
                )));
                if let Some(details) = sandbox_details_summary(isolation) {
                    lines.push(ratatui::text::Line::from(format!(
                        "  {} {}",
                        t("proc_insights.sandbox_details"),
                        details,
                    )));
                }
            }
        }
    }
    // Threads: compact column header plus the first N thread rows.
    if selected(ProcessInsightFacet::Threads) {
        match &projection.threads {
            ProcessInsightFacetState::Pending => lines.push(collecting()),
            ProcessInsightFacetState::Unavailable(reason) => {
                lines.push(insight_unavailable(theme, reason))
            }
            ProcessInsightFacetState::Current(threads) => {
                lines.extend(thread_preview_lines_with_limit(threads, theme, limit))
            }
        }
    }
    // Open files: entry count (+ unreadable marker) plus the first N descriptors.
    if selected(ProcessInsightFacet::OpenFiles) {
        match &projection.open_files {
            ProcessInsightFacetState::Pending => lines.push(collecting()),
            ProcessInsightFacetState::Unavailable(reason) => {
                lines.push(insight_unavailable(theme, reason))
            }
            ProcessInsightFacetState::Current(open_files) => {
                let res = match &projection.resources {
                    ProcessInsightFacetState::Current(r) => Some(project_process_resources(r)),
                    _ => None,
                };
                lines.extend(open_files_preview_lines_with_limit(
                    open_files,
                    res.as_ref(),
                    theme,
                    limit,
                ));
            }
        }
    }
    // Environment: entry count plus the first N bounded key=value entries.
    if selected(ProcessInsightFacet::Environment) {
        match &projection.environment {
            ProcessInsightFacetState::Pending => lines.push(collecting()),
            ProcessInsightFacetState::Unavailable(reason) => {
                lines.push(insight_unavailable(theme, reason))
            }
            ProcessInsightFacetState::Current(env) => {
                lines.extend(environment_preview_lines_with_limit(env, theme, limit))
            }
        }
    }
    // CPU Affinity: render observed affinity state for this process
    if selected(ProcessInsightFacet::Resources) {
        match app.shell.process_affinity_state() {
            ProcessAffinityState::Ready(ready) if ready.target.live_key() == Some(identity) => {
                let total = app.logical_cpu_count();
                let cpus_summary = if ready.cpus.is_empty() {
                    missing_value()
                } else if total > 0 && ready.cpus.len() == total {
                    format!("All ({total} CPUs)")
                } else if ready.cpus.len() <= 8 {
                    let mut sorted: Vec<u32> = ready.cpus.to_vec();
                    sorted.sort_unstable();
                    let joined = sorted
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("CPUs {joined}")
                } else {
                    format!("{} / {total} CPUs", ready.cpus.len())
                };
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("proc.affinity"),
                    cpus_summary,
                )));
            }
            ProcessAffinityState::Loading { target, .. } if target.live_key() == Some(identity) => {
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("proc.affinity"),
                    t("common.collecting_telemetry"),
                )));
            }
            _ => {}
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

/// Bounded preview row counts for the populated insight facets. The detail
/// panel is terminal-width constrained, so each facet shows a small head plus
/// an honest "…" when more remain — the same shape the connections and device
/// previews already use.
const THREADS_PREVIEW: usize = 3;

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
    let wait = thread.run_queue_wait_ns.map(|nanos| {
        let kind = thread
            .wait_kind
            .map(ThreadWaitKind::as_str)
            .unwrap_or("wait");
        format!("{kind} {:.1}ms", nanos as f64 / 1_000_000.0)
    });
    if let Some(ref wchan) = thread.wchan {
        let mut line = format!(
            "{}  {}  {}  {}  {}  [{}]",
            thread.tid,
            comm,
            thread.state.as_short_label(),
            cpu_time,
            cpu_percent,
            wchan,
        );
        if let Some(wait) = wait {
            line.push_str("  ");
            line.push_str(&wait);
        }
        line
    } else {
        let mut line = format!(
            "{}  {}  {}  {}  {}",
            thread.tid,
            comm,
            thread.state.as_short_label(),
            cpu_time,
            cpu_percent,
        );
        if let Some(wait) = wait {
            line.push_str("  ");
            line.push_str(&wait);
        }
        line
    }
}

/// Bounded Threads-facet preview: a count title, a compact column header, the
/// first [`THREADS_PREVIEW`] rows, and an honest "…" when more remain. An empty
/// thread list renders the explicit empty state, never a fabricated row.
#[cfg_attr(not(test), allow(dead_code))]
fn thread_preview_lines(
    threads: &ProcessThreads,
    theme: TuiTheme,
) -> Vec<ratatui::text::Line<'static>> {
    thread_preview_lines_with_limit(threads, theme, THREADS_PREVIEW)
}

fn thread_preview_lines_with_limit(
    threads: &ProcessThreads,
    theme: TuiTheme,
    limit: usize,
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
            "    TID  Name  St  {}  {}  {}",
            t("proc_insights.thread_cpu_time"),
            t("proc_insights.thread_wait"),
            t("proc_insights.thread_cpu_percent")
        ),
        Style::new().fg(theme.dim),
    )));
    for thread in threads.threads.iter().take(limit) {
        out.push(ratatui::text::Line::from(format!(
            "    {}",
            format_thread_row(thread)
        )));
    }
    if threads.threads.len() > limit {
        out.push(ratatui::text::Line::from(Span::styled(
            "    …",
            Style::new().fg(theme.dim),
        )));
    }
    out
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/process_details/insights_tests.rs"]
mod tests;
