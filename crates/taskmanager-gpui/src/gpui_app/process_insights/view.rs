//! Responsive render-only Process Properties insights and capture fixture.

use super::{ProcessInsightsErrorKind, ProcessInsightsRenderState};
use gpui::{Div, ParentElement, Styled, div, px};

use crate::core::device_state::DeviceStatus;
use crate::gpui_app::theme::tokens;
use crate::gpui_app::theme::{Color, Theme, mono_font_with_fallback};
use taskmanager_application::{
    ConnectionAddressFamily, ConnectionTransport, IsolationKind, LimitValue, ProcessConnection,
    ProcessTelemetrySnapshot, ProjectedProcessResources, project_process_resources,
};
use taskmanager_ui::data::key_value_row::KeyValueRow;
use taskmanager_ui::primitives::card_surface::CardSurface;

mod fixture;
mod gpu_engines;
mod open_files;
mod threads;

pub use fixture::process_insights_capture_fixture;

/// Widget-materialization cap shared by the scrollable insight cards (threads,
/// open files, connections). The collected data stays complete — card headers
/// keep the true totals — but only this many row elements are built, so a
/// process with thousands of descriptors or threads cannot rebuild thousands
/// of elements on every frame the modal is open. Rows beyond the cap are
/// reported through the explicit `… {count} more` hint.
pub(super) const MAX_INSIGHT_CARD_ROWS: usize = 200;

/// Pure window math for the card cap: `(shown, hidden)` for a collection of
/// `total` rows. Extracted so the bounded-materialization contract is testable
/// without rendering.
pub(super) fn capped_card_rows(total: usize) -> (usize, usize) {
    let shown = total.min(MAX_INSIGHT_CARD_ROWS);
    (shown, total - shown)
}

/// Copy supplied by the Properties caller. Keeping it outside the component
/// lets final integration wire locale keys without hard-coded production copy.
#[derive(Clone, Copy)]
pub struct ProcessInsightsLabels {
    pub loading: &'static str,
    pub connections: &'static str,
    pub no_connections: &'static str,
    pub network_throughput: &'static str,
    pub received: &'static str,
    pub sent: &'static str,
    pub gpu: &'static str,
    pub no_gpu: &'static str,
    pub gpu_usage: &'static str,
    pub vram: &'static str,
    pub resource_limits: &'static str,
    pub memory: &'static str,
    pub cpu: &'static str,
    pub pids: &'static str,
    pub resource_group: &'static str,
    pub isolation: &'static str,
    pub container_id: &'static str,
    pub sandboxed: &'static str,
    pub host_process: &'static str,
    pub open_files: &'static str,
    pub no_open_files: &'static str,
    pub unreadable: &'static str,
    pub threads: &'static str,
    pub no_threads: &'static str,
    pub thread_id: &'static str,
    pub thread_name: &'static str,
    pub thread_state: &'static str,
    pub thread_cpu_time: &'static str,
    pub thread_cpu_percent: &'static str,
    pub yes: &'static str,
    pub no: &'static str,
    pub unknown: &'static str,
    pub unlimited: &'static str,
    pub healthy: &'static str,
    pub stale: &'static str,
    pub permission_denied: &'static str,
    pub provider_unavailable: &'static str,
    pub unsupported: &'static str,
    pub worker_disconnected: &'static str,
}

impl ProcessInsightsLabels {
    /// Stable English strings reserved for deterministic headless/capture use.
    pub const fn capture_fixture() -> Self {
        Self {
            loading: "Loading process insights…",
            connections: "Connections",
            no_connections: "No active connections",
            network_throughput: "Network throughput",
            received: "Received",
            sent: "Sent",
            gpu: "GPU",
            no_gpu: "No process GPU counters",
            gpu_usage: "Usage",
            vram: "VRAM",
            resource_limits: "Resource limits",
            memory: "Memory",
            cpu: "CPU quota",
            pids: "Processes",
            resource_group: "Resource group",
            isolation: "Isolation",
            container_id: "Container ID",
            sandboxed: "Sandboxed",
            host_process: "Host process",
            open_files: "Open files",
            no_open_files: "No readable file descriptors",
            unreadable: "unreadable",
            threads: "Threads",
            no_threads: "No threads",
            thread_id: "TID",
            thread_name: "Name",
            thread_state: "State",
            thread_cpu_time: "CPU time",
            thread_cpu_percent: "CPU %",
            yes: "Yes",
            no: "No",
            unknown: "Unknown",
            unlimited: "Unlimited",
            healthy: "Available",
            stale: "Process unavailable",
            permission_denied: "Permission denied",
            provider_unavailable: "Provider unavailable",
            unsupported: "Unsupported by this provider",
            worker_disconnected: "Telemetry worker stopped",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcessInsightsLayout {
    pub columns: u8,
    pub card_width: f32,
}

pub fn process_insights_layout(available_width: f32) -> ProcessInsightsLayout {
    let width = available_width.max(240.0);
    if width >= 680.0 {
        ProcessInsightsLayout {
            columns: 2,
            card_width: (width - 8.0) / 2.0,
        }
    } else {
        ProcessInsightsLayout {
            columns: 1,
            card_width: width,
        }
    }
}

/// Responsive render-only body suitable for embedding in Process Properties.
/// `available_width` is the dialog content width, not the full window width.
pub(crate) fn render_process_insights(
    theme: &Theme,
    state: ProcessInsightsRenderState<'_>,
    labels: &ProcessInsightsLabels,
    available_width: f32,
    net_escalation: taskmanager_application::NetworkEscalationState,
    entity: gpui::Entity<crate::gpui_app::root::RootView>,
) -> Div {
    let layout = process_insights_layout(available_width);
    let root = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8);
    match state {
        ProcessInsightsRenderState::Loading => root.child(message_panel(
            theme,
            labels.loading,
            theme.fg_dim,
            layout.card_width,
        )),
        ProcessInsightsRenderState::Error(error) => root.child(message_panel(
            theme,
            error_label(error.kind, labels),
            theme.gpu,
            layout.card_width,
        )),
        ProcessInsightsRenderState::Ready(snapshot) => root.child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap(tokens::SPACE_8)
                .min_w(px(0.0))
                .child(network_card(
                    theme,
                    snapshot,
                    labels,
                    layout.card_width,
                    net_escalation,
                    entity.clone(),
                ))
                .child(gpu_card(theme, snapshot, labels, layout.card_width))
                .child(gpu_engines::gpu_engines_card(
                    theme,
                    snapshot,
                    labels,
                    layout.card_width,
                ))
                .child(resource_card(
                    theme,
                    project_process_resources(&snapshot.resources),
                    labels,
                    layout.card_width,
                ))
                .child(isolation_card(theme, snapshot, labels, layout.card_width))
                .child(open_files::open_files_card(
                    theme,
                    snapshot,
                    labels,
                    layout.card_width,
                ))
                .child(threads::threads_card(
                    theme,
                    snapshot,
                    labels,
                    layout.card_width,
                )),
        ),
    }
}

fn card(theme: &Theme, title: &str, width: f32) -> Div {
    CardSurface::new(theme.palette())
        .background(theme.sidebar_card_bg)
        .padding(tokens::SPACE_10)
        .radius(tokens::card_radius(theme))
        .bordered(false)
        .child(
            div()
                .mb(tokens::SPACE_7)
                .text_size(tokens::FONT_13)
                .font_weight(tokens::FONT_WEIGHT_HEADER.into())
                .child(title.to_string()),
        )
        .render()
        .w(px(width))
        .min_w(px(0.0))
        .text_color(theme.fg)
}

fn message_panel(theme: &Theme, message: &str, color: Color, width: f32) -> Div {
    card(theme, "", width).child(
        div()
            .min_w(px(0.0))
            .text_size(tokens::FONT_12)
            .text_color(color)
            .whitespace_normal()
            .child(message.to_string()),
    )
}

fn metric_row(theme: &Theme, label: &str, value: String) -> Div {
    KeyValueRow::new(label, value, theme.palette())
        .label_width(taskmanager_theme::Length(102.0))
        .value_align_right(false)
        .selectable_value(gpui::ElementId::Name(
            format!("process-insight-value:{label}").into(),
        ))
        .render()
}

fn network_card(
    theme: &Theme,
    snapshot: &ProcessTelemetrySnapshot,
    labels: &ProcessInsightsLabels,
    width: f32,
    net_escalation: taskmanager_application::NetworkEscalationState,
    entity: gpui::Entity<crate::gpui_app::root::RootView>,
) -> Div {
    let network = &snapshot.network;
    let availability = status_label(network.traffic_state.status, labels);
    let mut connections = div()
        .mt(tokens::SPACE_7)
        .pt(tokens::SPACE_7)
        .border_t_1()
        .border_color(theme.border)
        .flex()
        .flex_col()
        .gap(tokens::SPACE_3);
    if network.connections.is_empty() {
        connections = connections.child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(labels.no_connections.to_string()),
        );
    } else {
        let (shown, hidden) = capped_card_rows(network.connections.len());
        connections = connections.child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(format!(
                    "{} · {}",
                    labels.connections,
                    network.connections.len()
                )),
        );
        connections = connections.child(div().flex().flex_col().gap(tokens::SPACE_3).children(
            network.connections.iter().take(shown).map(|connection| {
                div()
                    .min_w(px(0.0))
                    .text_size(tokens::FONT_10)
                    .font(mono_font_with_fallback(theme))
                    .whitespace_normal()
                    .child(format_connection(connection))
            }),
        ));
        if hidden > 0 {
            connections =
                connections.child(crate::gpui_app::elements::more_rows_hint(theme, hidden));
        }
    }
    card(theme, labels.network_throughput, width)
        .child(metric_row(
            theme,
            labels.received,
            format_rate(network.rx_bytes_per_sec, availability),
        ))
        .child(metric_row(
            theme,
            labels.sent,
            format_rate(network.tx_bytes_per_sec, availability),
        ))
        .child(escalation_row(theme, network, net_escalation, entity))
        .child(connections)
}

/// Per-feature per-process-network escalation affordance (ADR-023/024/025):
/// when the accounting backend was denied for lack of `CAP_NET_RAW`, the
/// typed `RequiresEscalation` failure offers the OS-native prompt. The
/// pending state is driven by the correlated `NetworkCaptureEscalated`
/// event; a rejected submission shows the typed reason.
fn escalation_row(
    theme: &Theme,
    network: &crate::core::ProcessNetworkSnapshot,
    net_escalation: taskmanager_application::NetworkEscalationState,
    entity: gpui::Entity<crate::gpui_app::root::RootView>,
) -> Div {
    use crate::core::FailureKind;
    let escalatable = network.traffic_failure == Some(FailureKind::RequiresEscalation);
    if !escalatable {
        return div();
    }
    let entity = entity.clone();
    let (label, active) = match net_escalation {
        taskmanager_application::NetworkEscalationState::Closed => {
            ("Enable per-process network", false)
        }
        taskmanager_application::NetworkEscalationState::Loading(_) => {
            ("Waiting for authorization…", true)
        }
        taskmanager_application::NetworkEscalationState::Ready(_) => ("Enabled", true),
        taskmanager_application::NetworkEscalationState::Failed(_) => {
            ("Authorization failed — retry", false)
        }
    };
    div()
        .mt(tokens::SPACE_7)
        .flex()
        .flex_row()
        .items_center()
        .child(crate::gpui_app::elements::pill(
            theme,
            "process-insights-net-escalation",
            label,
            active,
            false,
            move |_window, cx| {
                entity.update(cx, |view, cx| {
                    view.request_process_network_escalation(cx);
                });
            },
            |_, _, _| {},
        ))
}

fn gpu_card(
    theme: &Theme,
    snapshot: &ProcessTelemetrySnapshot,
    labels: &ProcessInsightsLabels,
    width: f32,
) -> Div {
    let mut content = card(theme, labels.gpu, width);
    if snapshot.gpu.devices.is_empty() {
        return content.child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(if snapshot.gpu.state.status == DeviceStatus::Healthy {
                    labels.no_gpu.to_string()
                } else {
                    status_label(snapshot.gpu.state.status, labels).to_string()
                }),
        );
    }
    for device in &snapshot.gpu.devices {
        content = content.child(
            div()
                .mb(tokens::SPACE_7)
                .min_w(px(0.0))
                .child(
                    div()
                        .truncate()
                        .text_size(tokens::FONT_11)
                        .font(mono_font_with_fallback(theme))
                        .child(device.device_id.clone()),
                )
                .child(metric_row(
                    theme,
                    labels.gpu_usage,
                    device
                        .utilization_pct
                        .map(|value| format!("{value:.1}%"))
                        .unwrap_or_else(|| labels.unknown.to_string()),
                ))
                .child(metric_row(
                    theme,
                    labels.vram,
                    device
                        .memory_bytes
                        .map(format_bytes)
                        .unwrap_or_else(|| labels.unknown.to_string()),
                )),
        );
    }
    content
}

fn resource_card(
    theme: &Theme,
    resources: ProjectedProcessResources<'_>,
    labels: &ProcessInsightsLabels,
    width: f32,
) -> Div {
    let memory = format_pair(
        resources.memory_usage_bytes.map(format_bytes),
        resources
            .memory_limit
            .map(|value| format_limit(value, labels.unlimited, format_bytes)),
        labels.unknown,
    );
    let cpu = match (
        resources.cpu_time_quota_micros,
        resources.cpu_time_period_micros,
    ) {
        (Some(LimitValue::Unlimited), _) => labels.unlimited.to_string(),
        (Some(LimitValue::Value(quota)), Some(period)) if period > 0 => {
            format!("{:.1}%", quota as f64 / period as f64 * 100.0)
        }
        _ => labels.unknown.to_string(),
    };
    let pids = format_pair(
        resources.process_count.map(|value| value.to_string()),
        resources
            .process_limit
            .map(|value| format_limit(value, labels.unlimited, |value| value.to_string())),
        labels.unknown,
    );
    let resource_group = resources
        .resource_group
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| labels.unknown.to_string());
    card(theme, labels.resource_limits, width)
        .child(metric_row(theme, labels.memory, memory))
        .child(metric_row(theme, labels.cpu, cpu))
        .child(metric_row(theme, labels.pids, pids))
        .child(metric_row(theme, labels.resource_group, resource_group))
}

fn isolation_card(
    theme: &Theme,
    snapshot: &ProcessTelemetrySnapshot,
    labels: &ProcessInsightsLabels,
    width: f32,
) -> Div {
    let isolation = &snapshot.isolation;
    let identity = isolation
        .kind
        .as_ref()
        .map(isolation_label)
        .unwrap_or(labels.host_process);
    let sandboxed = match isolation.sandboxed {
        Some(true) => labels.yes,
        Some(false) => labels.no,
        None => labels.unknown,
    };
    let mut content = card(theme, labels.isolation, width)
        .child(metric_row(theme, labels.isolation, identity.to_string()))
        .child(metric_row(theme, labels.sandboxed, sandboxed.to_string()));
    if let Some(container_id) = &isolation.container_id {
        content = content.child(metric_row(theme, labels.container_id, container_id.clone()));
    }
    content
}

fn status_label(status: DeviceStatus, labels: &ProcessInsightsLabels) -> &'static str {
    match status {
        DeviceStatus::Healthy => labels.healthy,
        DeviceStatus::Stale => labels.stale,
        DeviceStatus::PermissionDenied => labels.permission_denied,
        DeviceStatus::MissingTool => labels.provider_unavailable,
        DeviceStatus::Unsupported => labels.unsupported,
    }
}

fn error_label(kind: ProcessInsightsErrorKind, labels: &ProcessInsightsLabels) -> &'static str {
    match kind {
        ProcessInsightsErrorKind::ProcessUnavailable => labels.stale,
        ProcessInsightsErrorKind::PermissionDenied => labels.permission_denied,
        ProcessInsightsErrorKind::ProviderUnavailable => labels.provider_unavailable,
        ProcessInsightsErrorKind::Unsupported => labels.unsupported,
        ProcessInsightsErrorKind::WorkerDisconnected => labels.worker_disconnected,
    }
}

pub(super) fn format_connection(connection: &ProcessConnection) -> String {
    let transport = match (&connection.transport, &connection.family) {
        (ConnectionTransport::Tcp, ConnectionAddressFamily::Ipv6) => "TCP6".to_string(),
        (ConnectionTransport::Udp, ConnectionAddressFamily::Ipv6) => "UDP6".to_string(),
        _ => connection.transport.to_string(),
    };
    format!("{transport}  {} → {}", connection.local, connection.remote)
}

fn format_rate(value: Option<u64>, unavailable: &str) -> String {
    value
        .map(|value| format!("{}/s", format_bytes(value)))
        .unwrap_or_else(|| unavailable.to_string())
}

fn format_bytes(bytes: u64) -> String {
    crate::gpui_app::formatting::bytes_to_human(bytes)
}

fn format_limit(
    value: LimitValue,
    unlimited: &str,
    format_value: impl FnOnce(u64) -> String,
) -> String {
    match value {
        LimitValue::Unlimited => unlimited.to_string(),
        LimitValue::Value(value) => format_value(value),
    }
}

fn format_pair(current: Option<String>, maximum: Option<String>, unknown: &str) -> String {
    match (current, maximum) {
        (Some(current), Some(maximum)) => format!("{current} / {maximum}"),
        (Some(current), None) => format!("{current} / {unknown}"),
        (None, Some(maximum)) => format!("{unknown} / {maximum}"),
        (None, None) => unknown.to_string(),
    }
}

fn isolation_label(kind: &IsolationKind) -> &'static str {
    match kind {
        IsolationKind::Docker => "Docker",
        IsolationKind::Podman => "Podman",
        IsolationKind::Kubernetes => "Kubernetes",
        IsolationKind::Lxc => "LXC",
        IsolationKind::SystemdNspawn => "systemd-nspawn",
        IsolationKind::Flatpak => "Flatpak",
        IsolationKind::Snap => "Snap",
        IsolationKind::Wsl => "WSL",
        IsolationKind::OtherContainer => "Container",
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_process_insights_view_cap_tests.rs"]
mod cap_tests;
