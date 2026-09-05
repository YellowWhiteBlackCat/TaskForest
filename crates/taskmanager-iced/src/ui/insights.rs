//! The process-insights facet projections for the details modal's Insights
//! tab (GPUI `process_insights` parity): one typed sub-section per facet
//! (threads / open files / network / GPU devices / GPU engines / resources /
//! isolation). Each facet honors its `ProcessInsightFacetState`: Pending (or
//! a not-yet-arrived projection) → a muted "collecting…" hint;
//! Unavailable(reason) → a typed message; Current(value) → the value. No cell
//! fabricates a number — a missing CPU value, a cold-start rate gap, or an
//! unreadable readlink renders an explicit dash / typed marker. The network
//! facet additionally surfaces the escalation pill when capture requires
//! privilege elevation (GPUI's "Enable per-process network" path).
//!
//! Extracted from [`super::overlays`] so the overlays module stays under the
//! repository's source-size budget.
mod helpers;
pub(crate) use helpers::*;

use iced::widget::{column, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_application::{
    ProcessInsightFacetState, ProcessInsightUnavailable, ProjectedProcessInsights,
    project_process_resources,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process_telemetry::{IsolationKind, LimitValue};

use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::{MISSING_VALUE, bytes, missing_value};
use taskmanager_theme::{Theme, tokens};

use crate::app::Message;
use crate::focus;

/// The explicit dash for any value the source could not prove (a missing CPU
/// counter, a cold-start rate gap, an unreadable readlink). Never a fabricated
/// `0`/`0.0%`.
const DASH: &str = MISSING_VALUE;
/// Cap on rows rendered per facet sub-section. The modal itself scrolls, so
/// this only keeps one busy process from dominating the panel.
const MAX_FACET_ROWS: usize = 8;

/// Build the per-process insights block (heading + eight facet sub-sections)
/// for the open details overlay. The block always renders: when no projection
/// for this frozen target has arrived yet every facet shows the honest
/// "collecting…" hint, because the per-tick request is already in flight.
pub(super) fn insights_block<'a>(
    theme_snapshot: &'a Theme,
    shell: &ShellApp,
    target: &taskmanager_core::core::process::FrozenProcessIdentity,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let projection = shell
        .projection()
        .process_insights
        .as_ref()
        .filter(|projection| &projection.target == target);
    column![
        section_title(theme_snapshot, t("prop.insights")),
        threads_section(theme_snapshot, projection),
        open_files_section(theme_snapshot, projection),
        network_section(theme_snapshot, projection),
        gpu_devices_section(theme_snapshot, projection),
        gpu_engines_section(theme_snapshot, projection),
        resources_section(theme_snapshot, projection),
        isolation_section(theme_snapshot, projection),
        environment_section(theme_snapshot, projection),
    ]
    .spacing(8)
    .width(Length::Fill)
    .into()
}

/// The network facet: per-process Received / Sent throughput plus a bounded
/// connections list (count when truncated). Mirrors GPUI's network_card. An
/// unavailable traffic observation renders an honest dash, never a fabricated
/// zero; a `RequiresEscalation` traffic state surfaces the typed reason
/// together with the escalation acceptance pill (GPUI's "Enable
/// per-process network" path).
fn network_section<'a>(
    theme_snapshot: &'a Theme,
    projection: Option<&ProjectedProcessInsights>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let state = projection.map(|projection| &projection.network);
    let (heading, mut body) = match state {
        None | Some(ProcessInsightFacetState::Pending) => (
            t("proc_insights.network_throughput").to_string(),
            vec![muted_text(theme_snapshot, t("proc_insights.collecting"))],
        ),
        Some(ProcessInsightFacetState::Unavailable(reason)) => (
            t("proc_insights.network_throughput").to_string(),
            vec![muted_text(theme_snapshot, facet_unavailable_text(reason))],
        ),
        Some(ProcessInsightFacetState::Current(network)) => {
            let heading = format!(
                "{} · {}",
                t("proc_insights.connections"),
                network.connections.len()
            );
            let mut rows = vec![
                kv_row(
                    theme_snapshot,
                    t("proc_insights.received").to_string(),
                    network
                        .rx_bytes_per_sec
                        .map(|v| format!("{}/s", bytes(v)))
                        .unwrap_or_else(missing_value),
                ),
                kv_row(
                    theme_snapshot,
                    t("proc_insights.sent").to_string(),
                    network
                        .tx_bytes_per_sec
                        .map(|v| format!("{}/s", bytes(v)))
                        .unwrap_or_else(missing_value),
                ),
            ];
            if network.connections.is_empty() {
                rows.push(muted_text(
                    theme_snapshot,
                    t("proc_insights.no_connections"),
                ));
            } else {
                for connection in network.connections.iter().take(MAX_FACET_ROWS) {
                    rows.push(muted_text(theme_snapshot, format_connection(connection)));
                }
                if network.connections.len() > MAX_FACET_ROWS {
                    rows.push(muted_text(
                        theme_snapshot,
                        format!("… +{} more", network.connections.len() - MAX_FACET_ROWS),
                    ));
                }
            }
            (heading, rows)
        }
    };
    // The escalation seam: when per-process capture needs privilege elevation
    // the typed reason is accompanied by the acceptance pill (GPUI parity).
    if matches!(
        state,
        Some(ProcessInsightFacetState::Unavailable(
            ProcessInsightUnavailable::Provider(FailureKind::RequiresEscalation)
        ))
    ) {
        body.push(escalation_pill(theme_snapshot));
    }
    section_column(theme_snapshot, &heading, body)
}

/// The acceptance pill for the per-process network capture escalation
/// (GPUI's "Enable per-process network" button): a focusable action that
/// emits the shared one-shot escalation request.
fn escalation_pill<'a>(
    theme_snapshot: &'a Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    focus::dynamic_button(
        theme_snapshot,
        crate::app::FocusTarget::ProcessNetworkEscalation,
        t("proc_insights.enable_network_capture").to_string(),
        Message::RequestProcessNetworkEscalation,
        false,
    )
}

/// One connection readout line: `{transport}  {local} → {remote}` with
/// family-aware transport naming (TCP6/UDP6), mirroring gpui's
/// `format_connection` so the local/remote endpoints are never dropped.
#[must_use]
fn format_connection(
    connection: &taskmanager_core::core::process_telemetry::ProcessConnection,
) -> String {
    let transport = match (&connection.transport, &connection.family) {
        (
            taskmanager_core::core::process_telemetry::ConnectionTransport::Tcp,
            taskmanager_core::core::process_telemetry::ConnectionAddressFamily::Ipv6,
        ) => "TCP6".to_string(),
        (
            taskmanager_core::core::process_telemetry::ConnectionTransport::Udp,
            taskmanager_core::core::process_telemetry::ConnectionAddressFamily::Ipv6,
        ) => "UDP6".to_string(),
        _ => connection.transport.to_string(),
    };
    format!("{transport}  {} → {}", connection.local, connection.remote)
}

/// The per-PCI GPU device facet: one row per device (device id, usage %, VRAM).
/// Mirrors GPUI's gpu_card device rollup (distinct from the per-engine facet).
fn gpu_devices_section<'a>(
    theme_snapshot: &'a Theme,
    projection: Option<&ProjectedProcessInsights>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let state = projection.map(|projection| &projection.gpu);
    let (heading, body) = match state {
        None | Some(ProcessInsightFacetState::Pending) => (
            t("common.gpu").to_string(),
            vec![muted_text(theme_snapshot, t("proc_insights.collecting"))],
        ),
        Some(ProcessInsightFacetState::Unavailable(reason)) => (
            t("common.gpu").to_string(),
            vec![muted_text(theme_snapshot, facet_unavailable_text(reason))],
        ),
        Some(ProcessInsightFacetState::Current(gpu)) => {
            if gpu.devices.is_empty() {
                (
                    t("common.gpu").to_string(),
                    vec![muted_text(theme_snapshot, t("proc_insights.no_gpu"))],
                )
            } else {
                let mut rows = Vec::new();
                for device in gpu.devices.iter().take(MAX_FACET_ROWS) {
                    rows.push(kv_row(
                        theme_snapshot,
                        device.device_id.clone(),
                        device
                            .utilization_pct
                            .map(|v| format!("{:.0}%", v.round()))
                            .unwrap_or_else(missing_value),
                    ));
                    if let Some(vram) = device.memory_bytes {
                        rows.push(kv_row(
                            theme_snapshot,
                            t("proc_insights.vram").to_string(),
                            bytes(vram),
                        ));
                    }
                }
                if gpu.devices.len() > MAX_FACET_ROWS {
                    rows.push(muted_text(
                        theme_snapshot,
                        format!("… +{} more", gpu.devices.len() - MAX_FACET_ROWS),
                    ));
                }
                (t("common.gpu").to_string(), rows)
            }
        }
    };
    section_column(theme_snapshot, &heading, body)
}

/// Format an optional (current, limit) pair into an honest display string.
/// A missing value renders with an explicit dash or omitted when both are
/// unobserved; an unlimited limit renders "∞", never a fabricated zero.
pub(crate) fn format_resource_pair(
    current: Option<String>,
    limit: Option<LimitValue>,
    format_val: impl Fn(u64) -> String,
) -> Option<String> {
    let limit_str = limit.map(|l| match l {
        LimitValue::Unlimited => "∞".to_string(),
        LimitValue::Value(v) => format_val(v),
    });
    match (current, limit_str) {
        (Some(c), Some(m)) => Some(format!("{c} / {m}")),
        (Some(c), None) => Some(c),
        (None, Some(m)) => Some(format!("{DASH} / {m}")),
        (None, None) => None,
    }
}

/// The resource-limits facet: memory usage / limit, CPU quota as a percentage
/// of one period, pid count when the provider exposes them, and CGroup locator.
/// Mirrors GPUI's resource_card. An unlimited quota renders "∞", never a
/// fabricated zero.
pub(crate) fn resources_section<'a>(
    theme_snapshot: &'a Theme,
    projection: Option<&ProjectedProcessInsights>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let state = projection.map(|projection| &projection.resources);
    let (heading, body) = match state {
        None | Some(ProcessInsightFacetState::Pending) => (
            t("proc_insights.resource_limits").to_string(),
            vec![muted_text(theme_snapshot, t("proc_insights.collecting"))],
        ),
        Some(ProcessInsightFacetState::Unavailable(reason)) => (
            t("proc_insights.resource_limits").to_string(),
            vec![muted_text(theme_snapshot, facet_unavailable_text(reason))],
        ),
        Some(ProcessInsightFacetState::Current(resources)) => {
            let resources = project_process_resources(resources);
            let mut rows = Vec::new();
            if let Some(mem_str) = format_resource_pair(
                resources.memory_usage_bytes.map(bytes),
                resources.memory_limit,
                bytes,
            ) {
                rows.push(kv_row(
                    theme_snapshot,
                    t("common.memory").to_string(),
                    mem_str,
                ));
            }
            if let Some(quota) = resources.cpu_time_quota_micros {
                let pct = match quota {
                    LimitValue::Unlimited => "∞".to_string(),
                    LimitValue::Value(q) => {
                        if let Some(period) = resources.cpu_time_period_micros
                            && period > 0
                        {
                            format!("{:.0}%", (q as f64 / period as f64) * 100.0)
                        } else {
                            missing_value()
                        }
                    }
                };
                rows.push(kv_row(
                    theme_snapshot,
                    t("proc_insights.cpu_quota").to_string(),
                    pct,
                ));
            }
            // Pid count and resource-group locator (GPUI resource_card
            // parity): an unlimited pids limit renders "∞", never a
            // fabricated number.
            if let Some(pids_str) = format_resource_pair(
                resources.process_count.map(|count| count.to_string()),
                resources.process_limit,
                |v| v.to_string(),
            ) {
                rows.push(kv_row(
                    theme_snapshot,
                    t("proc_insights.pids").to_string(),
                    pids_str,
                ));
            }
            if let Some(group) = resources.resource_group {
                rows.push(kv_row(
                    theme_snapshot,
                    t("proc_insights.resource_group").to_string(),
                    group.to_owned(),
                ));
            }
            if rows.is_empty() {
                rows.push(muted_text(theme_snapshot, t("proc_insights.unknown")));
            }
            (t("proc_insights.resource_limits").to_string(), rows)
        }
    };
    section_column(theme_snapshot, &heading, body)
}

/// The isolation facet: the container/sandbox kind (Docker / Podman / …), the
/// container id, and the sandboxed flag. A host process renders "Host process".
/// Mirrors GPUI's isolation_card.
pub(crate) fn isolation_section<'a>(
    theme_snapshot: &'a Theme,
    projection: Option<&ProjectedProcessInsights>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let state = projection.map(|projection| &projection.isolation);
    let (heading, body) = match state {
        None | Some(ProcessInsightFacetState::Pending) => (
            t("proc_insights.isolation").to_string(),
            vec![muted_text(theme_snapshot, t("proc_insights.collecting"))],
        ),
        Some(ProcessInsightFacetState::Unavailable(reason)) => (
            t("proc_insights.isolation").to_string(),
            vec![muted_text(theme_snapshot, facet_unavailable_text(reason))],
        ),
        Some(ProcessInsightFacetState::Current(isolation)) => {
            let kind = match isolation.kind {
                Some(IsolationKind::Docker) => "Docker",
                Some(IsolationKind::Podman) => "Podman",
                Some(IsolationKind::Kubernetes) => "Kubernetes",
                Some(IsolationKind::Lxc) => "LXC",
                Some(IsolationKind::SystemdNspawn) => "systemd-nspawn",
                Some(IsolationKind::Flatpak) => "Flatpak",
                Some(IsolationKind::Snap) => "Snap",
                Some(IsolationKind::Wsl) => "WSL",
                Some(IsolationKind::OtherContainer) => "Container",
                None => t("proc_insights.host_process"),
            };
            let mut rows = vec![kv_row(
                theme_snapshot,
                t("proc_insights.isolation").to_string(),
                kind,
            )];
            if let Some(id) = isolation.container_id.as_deref()
                && !id.is_empty()
            {
                rows.push(kv_row(
                    theme_snapshot,
                    t("proc_insights.container_id").to_string(),
                    id.to_string(),
                ));
            }
            let sandboxed_str = match isolation.sandboxed {
                Some(true) => "Yes",
                Some(false) => "No",
                None => t("proc_insights.unknown"),
            };
            rows.push(kv_row(
                theme_snapshot,
                t("proc_insights.sandboxed").to_string(),
                sandboxed_str,
            ));
            (t("proc_insights.isolation").to_string(), rows)
        }
    };
    section_column(theme_snapshot, &heading, body)
}

/// The environment facet: bounded environment key/value rows with honest
/// Pending / Unavailable / Current empty states. Mirrors GPUI's environment_card.
pub(crate) fn environment_section<'a>(
    theme_snapshot: &'a Theme,
    projection: Option<&ProjectedProcessInsights>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let state = projection.map(|projection| &projection.environment);
    let (heading, body) = match state {
        None | Some(ProcessInsightFacetState::Pending) => (
            t("prop.environment").to_string(),
            vec![muted_text(theme_snapshot, t("proc_insights.collecting"))],
        ),
        Some(ProcessInsightFacetState::Unavailable(reason)) => (
            t("prop.environment").to_string(),
            vec![muted_text(theme_snapshot, facet_unavailable_text(reason))],
        ),
        Some(ProcessInsightFacetState::Current(environment)) => {
            let heading = if environment.truncated_count > 0 {
                format!(
                    "{} · {} · +{}",
                    t("prop.environment"),
                    environment.entries.len(),
                    environment.truncated_count,
                )
            } else if environment.entries.is_empty() {
                t("prop.environment").to_string()
            } else {
                format!("{} · {}", t("prop.environment"), environment.entries.len())
            };
            if environment.entries.is_empty() {
                (
                    heading,
                    vec![muted_text(theme_snapshot, t("prop.environment_empty"))],
                )
            } else {
                let mut rows = Vec::new();
                for entry in environment.entries.iter().take(MAX_FACET_ROWS) {
                    rows.push(kv_row(
                        theme_snapshot,
                        entry.key.clone(),
                        entry.value.clone(),
                    ));
                }
                if environment.entries.len() > MAX_FACET_ROWS {
                    rows.push(muted_text(
                        theme_snapshot,
                        format!("… +{} more", environment.entries.len() - MAX_FACET_ROWS),
                    ));
                }
                (heading, rows)
            }
        }
    };
    section_column(theme_snapshot, &heading, body)
}

/// The threads facet: a Fixed-width column-header row plus one row per thread
/// (bounded; an ellipsis notes the truncation). Missing CPU time / CPU% render
/// the explicit dash — never a fabricated zero.
fn threads_section<'a>(
    theme_snapshot: &'a Theme,
    projection: Option<&ProjectedProcessInsights>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let state = projection.map(|projection| &projection.threads);
    let (heading, body) = match state {
        None | Some(ProcessInsightFacetState::Pending) => (
            t("proc_insights.threads").to_string(),
            vec![muted_text(theme_snapshot, t("proc_insights.collecting"))],
        ),
        Some(ProcessInsightFacetState::Unavailable(reason)) => (
            t("proc_insights.threads").to_string(),
            vec![muted_text(theme_snapshot, facet_unavailable_text(reason))],
        ),
        Some(ProcessInsightFacetState::Current(threads)) => {
            let heading = if threads.threads.is_empty() {
                t("proc_insights.threads").to_string()
            } else {
                format!("{} · {}", t("proc_insights.threads"), threads.threads.len())
            };
            if threads.threads.is_empty() {
                (
                    heading,
                    vec![muted_text(theme_snapshot, t("proc_insights.no_threads"))],
                )
            } else {
                let mut rows = vec![thread_header(theme_snapshot)];
                for thread in threads.threads.iter().take(MAX_FACET_ROWS) {
                    rows.push(thread_row(thread));
                }
                if threads.threads.len() > MAX_FACET_ROWS {
                    rows.push(muted_text(
                        theme_snapshot,
                        format!("… +{} more", threads.threads.len() - MAX_FACET_ROWS),
                    ));
                }
                (heading, rows)
            }
        }
    };
    section_column(theme_snapshot, &heading, body)
}

/// The open-files facet: an "Open files · N · M unreadable" header plus a
/// bounded `fd → target` list. A descriptor whose readlink failed keeps its
/// row with the typed unreadable marker; a healthy process with no readable
/// descriptors shows the explicit empty state.
fn open_files_section<'a>(
    theme_snapshot: &'a Theme,
    projection: Option<&ProjectedProcessInsights>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let state = projection.map(|projection| &projection.open_files);
    let (heading, body) = match state {
        None | Some(ProcessInsightFacetState::Pending) => (
            t("proc_insights.open_files").to_string(),
            vec![muted_text(theme_snapshot, t("proc_insights.collecting"))],
        ),
        Some(ProcessInsightFacetState::Unavailable(reason)) => (
            t("proc_insights.open_files").to_string(),
            vec![muted_text(theme_snapshot, facet_unavailable_text(reason))],
        ),
        Some(ProcessInsightFacetState::Current(open_files)) => {
            let heading = if open_files.unreadable_count > 0 {
                format!(
                    "{} · {} · {} {}",
                    t("proc_insights.open_files"),
                    open_files.entries.len(),
                    open_files.unreadable_count,
                    t("proc_insights.unreadable"),
                )
            } else {
                format!(
                    "{} · {}",
                    t("proc_insights.open_files"),
                    open_files.entries.len()
                )
            };
            if open_files.entries.is_empty() {
                (
                    heading,
                    vec![muted_text(theme_snapshot, t("proc_insights.no_open_files"))],
                )
            } else {
                let mut rows = Vec::new();
                for entry in open_files.entries.iter().take(MAX_FACET_ROWS) {
                    rows.push(
                        text(format_open_file_row(entry))
                            .size(f32::from(tokens::FONT_12))
                            .into(),
                    );
                }
                if open_files.entries.len() > MAX_FACET_ROWS {
                    rows.push(muted_text(
                        theme_snapshot,
                        format!("… +{} more", open_files.entries.len() - MAX_FACET_ROWS),
                    ));
                }
                (heading, rows)
            }
        }
    };
    section_column(theme_snapshot, &heading, body)
}

/// The per-process GPU-engines facet: one row per engine — engine name plus
/// the single-core-equivalent utilization rate. A cold-start rate gap (first
/// sample, counter rollback, or a cycles-only xe source) renders the explicit
/// dash, never a fabricated `0.0%`.
fn gpu_engines_section<'a>(
    theme_snapshot: &'a Theme,
    projection: Option<&ProjectedProcessInsights>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let state = projection.map(|projection| &projection.gpu);
    let (heading, body) = match state {
        None | Some(ProcessInsightFacetState::Pending) => (
            t("proc_insights.gpu_engines").to_string(),
            vec![muted_text(theme_snapshot, t("proc_insights.collecting"))],
        ),
        Some(ProcessInsightFacetState::Unavailable(reason)) => (
            t("proc_insights.gpu_engines").to_string(),
            vec![muted_text(theme_snapshot, facet_unavailable_text(reason))],
        ),
        Some(ProcessInsightFacetState::Current(gpu)) => {
            let engines = &gpu.engines.engines;
            let heading = if engines.is_empty() {
                t("proc_insights.gpu_engines").to_string()
            } else {
                format!("{} · {}", t("proc_insights.gpu_engines"), engines.len())
            };
            if engines.is_empty() {
                (
                    heading,
                    vec![muted_text(
                        theme_snapshot,
                        t("proc_insights.no_gpu_engines"),
                    )],
                )
            } else {
                let mut rows = Vec::new();
                for engine in engines.iter().take(MAX_FACET_ROWS) {
                    rows.push(
                        text(format_engine_usage(
                            &engine.name,
                            &engine.usage_pct,
                            &engine.engine_time_ns,
                            &engine.engine_cycles,
                        ))
                        .size(f32::from(tokens::FONT_12))
                        .into(),
                    );
                }
                if engines.len() > MAX_FACET_ROWS {
                    rows.push(muted_text(
                        theme_snapshot,
                        format!("… +{} more", engines.len() - MAX_FACET_ROWS),
                    ));
                }
                (heading, rows)
            }
        }
    };
    section_column(theme_snapshot, &heading, body)
}

#[cfg(test)]
#[path = "../../tests/gui/ui/insights/tests.rs"]
mod tests;
