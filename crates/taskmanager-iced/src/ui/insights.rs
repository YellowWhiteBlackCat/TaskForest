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

use iced::widget::{column, row, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_application::{
    ProcessInsightFacetState, ProcessInsightUnavailable, ProjectedProcessInsights,
    project_process_resources,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process_telemetry::{
    IsolationKind, LimitValue, OpenFileEntry, ProcessThreadInfo,
};
use taskmanager_platform_contract::SubmissionErrorKind;

use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::{MISSING_VALUE, bytes, missing_value};
use taskmanager_theme::{Theme, tokens};

use crate::app::Message;
use crate::focus;
use crate::theme;

/// The explicit dash for any value the source could not prove (a missing CPU
/// counter, a cold-start rate gap, an unreadable readlink). Never a fabricated
/// `0`/`0.0%`.
const DASH: &str = MISSING_VALUE;
/// Cap on rows rendered per facet sub-section. The modal itself scrolls, so
/// this only keeps one busy process from dominating the panel.
const MAX_FACET_ROWS: usize = 8;

/// Build the per-process insights block (heading + seven facet sub-sections)
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
                (t("common.gpu").to_string(), rows)
            }
        }
    };
    section_column(theme_snapshot, &heading, body)
}

/// The resource-limits facet: memory usage / limit, CPU quota as a percentage
/// of one period, and pid count when the provider exposes them. Mirrors GPUI's
/// resource_card. An unlimited quota renders "∞", never a fabricated zero.
fn resources_section<'a>(
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
            if let Some(used) = resources.memory_usage_bytes {
                let limit = match resources.memory_limit {
                    Some(LimitValue::Unlimited) | None => "∞".to_string(),
                    Some(LimitValue::Value(v)) => bytes(v),
                };
                rows.push(kv_row(
                    theme_snapshot,
                    t("common.memory").to_string(),
                    format!("{} / {}", bytes(used), limit),
                ));
            }
            if let (Some(quota), Some(period)) = (
                resources.cpu_time_quota_micros,
                resources.cpu_time_period_micros,
            ) {
                let pct = match quota {
                    LimitValue::Unlimited => "∞".to_string(),
                    LimitValue::Value(q) if period > 0 => {
                        format!("{:.0}%", (q as f64 / period as f64) * 100.0)
                    }
                    LimitValue::Value(_) => missing_value(),
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
            if let Some(count) = resources.process_count {
                let limit = match resources.process_limit {
                    Some(LimitValue::Unlimited) | None => "∞".to_string(),
                    Some(LimitValue::Value(v)) => v.to_string(),
                };
                rows.push(kv_row(
                    theme_snapshot,
                    t("proc_insights.pids").to_string(),
                    format!("{count} / {limit}"),
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
                rows.push(muted_text(theme_snapshot, t("proc_insights.no_gpu")));
            }
            (t("proc_insights.resource_limits").to_string(), rows)
        }
    };
    section_column(theme_snapshot, &heading, body)
}

/// The isolation facet: the container/sandbox kind (Docker / Podman / …), the
/// container id, and the sandboxed flag. A host process renders "Host process".
/// Mirrors GPUI's isolation_card.
fn isolation_section<'a>(
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
            if let Some(sandboxed) = isolation.sandboxed {
                rows.push(kv_row(
                    theme_snapshot,
                    t("proc_insights.sandboxed").to_string(),
                    if sandboxed { "Yes" } else { "No" },
                ));
            }
            (t("proc_insights.isolation").to_string(), rows)
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

/// Assemble one facet sub-section: an accent-colored heading followed by its
/// content rows.
fn section_column<'a>(
    theme_snapshot: &'a Theme,
    heading: &str,
    body: Vec<Element<'a, Message, iced::Theme, iced::Renderer>>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let mut col = column![section_title(theme_snapshot, heading)];
    for child in body {
        col = col.push(child);
    }
    col.spacing(4).width(Length::Fill).into()
}

fn section_title<'a>(
    theme_snapshot: &'a Theme,
    heading: &str,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    text(heading.to_string())
        .size(f32::from(tokens::FONT_13))
        .color(taskmanager_theme::iced::color(
            theme_snapshot.palette().accent,
        ))
        .into()
}

fn muted_text<'a, S: iced::advanced::text::IntoFragment<'a>>(
    theme_snapshot: &'a Theme,
    body: S,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    text(body)
        .size(f32::from(tokens::FONT_12))
        .color(theme::muted_text_color(theme_snapshot))
        .into()
}

/// A small label / value row for an insight facet (memory, quota, container
/// id, …). The label is muted and Fixed-width so the values align; the value
/// is the body color. The label is OWNED so the returned element borrows only
/// the theme (not the facet projection), keeping the section lifetime-simple.
fn kv_row<'a, V: iced::advanced::text::IntoFragment<'a>>(
    theme_snapshot: &'a Theme,
    label: String,
    value: V,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    row![
        text(label).width(Length::Fixed(150.0)).color(muted),
        text(value).width(Length::Fill),
    ]
    .spacing(8)
    .width(Length::Fill)
    .into()
}

/// The Fixed-width column-header row shared by every thread data row.
fn thread_header<'a>(
    theme_snapshot: &'a Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    row![
        text("TID").width(Length::Fixed(56.0)).color(muted),
        text("Name").width(Length::Fixed(196.0)).color(muted),
        text("State").width(Length::Fixed(48.0)).color(muted),
        text("CPU-time").width(Length::Fixed(72.0)).color(muted),
        text("CPU%").width(Length::Fill).color(muted),
    ]
    .spacing(8)
    .padding(2)
    .width(Length::Fill)
    .into()
}

/// One thread as a Fixed-width row, matching [`thread_header`]'s columns. An
/// empty `comm` is the contract's unknown identity (e.g. Windows ToolHelp32
/// exposes no thread names) and renders the explicit dash like every other
/// typed gap.
fn thread_row<'a>(thread: &ProcessThreadInfo) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let comm = if thread.comm.is_empty() {
        missing_value()
    } else {
        thread.comm.clone()
    };
    row![
        text(thread.tid.to_string()).width(Length::Fixed(56.0)),
        text(comm).width(Length::Fixed(196.0)),
        text(thread.state.as_short_label()).width(Length::Fixed(48.0)),
        text(cpu_time_text(thread.cpu_time_secs)).width(Length::Fixed(72.0)),
        text(cpu_percent_text(thread.cpu_percent)).width(Length::Fill),
    ]
    .spacing(8)
    .padding(2)
    .width(Length::Fill)
    .into()
}

/// Honest cumulative CPU time for one thread: the parsed seconds, or the
/// explicit dash when the source lacked CPU counters.
fn cpu_time_text(cpu: Option<f64>) -> String {
    cpu.map_or_else(|| DASH.to_string(), |value| format!("{value:.1}s"))
}

/// Honest instantaneous CPU% for one thread: the identity-bound rate, or the
/// explicit dash for the first sample / a counter gap.
fn cpu_percent_text(cpu: Option<f32>) -> String {
    cpu.map_or_else(|| DASH.to_string(), |value| format!("{value:.1}%"))
}

/// One open file descriptor as `fd → target`. An unreadable target (a failed
/// readlink) keeps the row with the typed "unreadable" marker.
fn format_open_file_row(entry: &OpenFileEntry) -> String {
    let target = entry
        .target
        .clone()
        .unwrap_or_else(|| t("proc_insights.unreadable").to_string());
    format!("fd {} → {}", entry.fd, target)
}

/// One GPU engine as `name  usage%`. The cold-start rate gap renders the
/// explicit dash rather than a fabricated `0.0%`.
/// One engine line: `name  rate` plus the cumulative busy time or cycle count
/// when the driver reports it (GPUI `gpu_engines` parity): i915 exposes busy
/// nanoseconds, xe exposes cycles — the cumulative readout shows whichever
/// the source typed, never both.
fn format_engine_usage(
    name: &str,
    usage_pct: &ScalarObservation<f32>,
    time_ns: &ScalarObservation<u64>,
    cycles: &ScalarObservation<u64>,
) -> String {
    let usage = usage_pct
        .current_value()
        .map_or_else(|| DASH.to_string(), |value| format!("{value:.1}%"));
    let cumulative = time_ns
        .current_value()
        .map(|nanos| taskmanager_shell::presentation::duration(*nanos / 1_000_000_000))
        .or_else(|| {
            cycles
                .current_value()
                .map(|value| format!("{value} cycles"))
        })
        .unwrap_or_else(|| DASH.to_string());
    format!("{name}  {usage}  {cumulative}")
}

/// Typed one-line message for a facet that cannot contribute. Mirrors the
/// gpui status labels: a permission denial (or a capability the per-feature
/// escalation seam could reach) is "permission denied"; an unsupported facet
/// is "unsupported"; everything else is the honest generic "unavailable".
pub(crate) fn facet_unavailable_text(reason: &ProcessInsightUnavailable) -> String {
    match reason {
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied)
        | ProcessInsightUnavailable::Provider(FailureKind::RequiresEscalation) => {
            "permission denied"
        }
        ProcessInsightUnavailable::Provider(FailureKind::Unsupported)
        | ProcessInsightUnavailable::Submission(SubmissionErrorKind::UnsupportedCapability) => {
            "unsupported"
        }
        _ => "unavailable",
    }
    .to_string()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/insights/tests.rs"]
mod tests;
