//! Responsive render-only Process Properties insights and capture fixture.

use super::{ProcessInsightsErrorKind, ProcessInsightsRenderState};
#[cfg(any(test, feature = "test-support"))]
use gpui::InteractiveElement;
use gpui::{Div, ParentElement, Styled, div, px};

use taskmanager_application::{ProjectedProcessResources, project_process_resources};
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::process_telemetry::{LimitValue, ProcessTelemetrySnapshot};
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_theme::tokens;
use taskmanager_theme::{Color, Theme};

use crate::gpui_app::theme::mono_font_with_fallback;
use taskmanager_ui::data::key_value_row::KeyValueRow;
use taskmanager_ui::primitives::card_surface::CardSurface;

mod fixture;
mod formatting;
mod gpu_engines;
mod labels;
mod open_files;
mod threads;

pub use fixture::process_insights_capture_fixture;
pub(super) use formatting::format_connection;
use formatting::{format_bytes, format_limit, format_pair, format_rate, isolation_label};
pub use labels::ProcessInsightsLabels;

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

/// Attach the test-support row selector to one insight-card row.
///
/// GPUI's test harness exposes geometry per debug selector and no text
/// readback, so a render test can only prove that a row was painted — and
/// which typed token it carried — through these selectors. `kind` names the
/// card, `index` the row's projection position, and `token` the typed state
/// the row's own format branch renders (`readable`/`unreadable`, `cpu-gap`/
/// `cpu-measured`, …), mirroring the services status-cell selector. The
/// product build compiles the identity arm, so this costs nothing at runtime.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn insight_row(row: Div, kind: &'static str, index: usize, token: &'static str) -> Div {
    row.debug_selector(move || format!("tm-insight-{kind}:{index}:{token}"))
}

#[cfg(not(any(test, feature = "test-support")))]
pub(super) fn insight_row(
    row: Div,
    _kind: &'static str,
    _index: usize,
    _token: &'static str,
) -> Div {
    row
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
    units: UnitPreferences,
) -> Div {
    let layout = process_insights_layout(available_width);
    let root = div().w_full().min_w(px(0.0)).flex().flex_col().gap(
        taskmanager_ui::theme_binding::definite_length(tokens::SPACE_8),
    );
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
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .min_w(px(0.0))
                .child(network_card(
                    theme,
                    snapshot,
                    labels,
                    layout.card_width,
                    net_escalation,
                    entity.clone(),
                    units,
                ))
                .child(gpu_card(theme, snapshot, labels, layout.card_width, units))
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
                    units,
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
                ))
                .child(environment_card(theme, snapshot, labels, layout.card_width)),
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
                .mb(taskmanager_ui::theme_binding::length(tokens::SPACE_7))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                .font_weight(taskmanager_ui::theme_binding::font_weight(
                    tokens::FONT_WEIGHT_HEADER,
                ))
                .child(title.to_string()),
        )
        .render()
        .w(px(width))
        .min_w(px(0.0))
        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
}

fn message_panel(theme: &Theme, message: &str, color: Color, width: f32) -> Div {
    card(theme, "", width).child(
        div()
            .min_w(px(0.0))
            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
            .text_color(taskmanager_ui::theme_binding::hsla(color))
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
    units: UnitPreferences,
) -> Div {
    let network = &snapshot.network;
    let availability = status_label(network.traffic_state.status, labels);
    let mut connections = div()
        .mt(taskmanager_ui::theme_binding::length(tokens::SPACE_7))
        .pt(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_7,
        ))
        .border_t_1()
        .border_color(taskmanager_ui::theme_binding::hsla(theme.border))
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_3,
        ));
    if network.connections.is_empty() {
        connections = connections.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(labels.no_connections.to_string()),
        );
    } else {
        let (shown, hidden) = capped_card_rows(network.connections.len());
        connections = connections.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(format!(
                    "{} · {}",
                    labels.connections,
                    network.connections.len()
                )),
        );
        connections = connections.child(
            div()
                .flex()
                .flex_col()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_3,
                ))
                .children(network.connections.iter().take(shown).map(|connection| {
                    div()
                        .min_w(px(0.0))
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_10))
                        .font(mono_font_with_fallback(theme))
                        .whitespace_normal()
                        .child(format_connection(connection))
                })),
        );
        if hidden > 0 {
            connections =
                connections.child(crate::gpui_app::elements::more_rows_hint(theme, hidden));
        }
    }
    let counter_row = network
        .connection_counters
        .as_ref()
        .and_then(taskmanager_shell::presentation::network_connection_counters_summary)
        .map(|summary| metric_row(theme, labels.network_throughput, summary))
        .unwrap_or_else(|| div());
    card(theme, labels.network_throughput, width)
        .child(metric_row(
            theme,
            labels.received,
            format_rate(units, network.rx_bytes_per_sec, availability),
        ))
        .child(metric_row(
            theme,
            labels.sent,
            format_rate(units, network.tx_bytes_per_sec, availability),
        ))
        .child(counter_row)
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
    network: &taskmanager_core::core::ProcessNetworkSnapshot,
    net_escalation: taskmanager_application::NetworkEscalationState,
    entity: gpui::Entity<crate::gpui_app::root::RootView>,
) -> Div {
    use taskmanager_core::core::FailureKind;
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
        .mt(taskmanager_ui::theme_binding::length(tokens::SPACE_7))
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
    units: UnitPreferences,
) -> Div {
    let mut content = card(theme, labels.gpu, width);
    if snapshot.gpu.devices.is_empty() {
        return content.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
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
                .mb(taskmanager_ui::theme_binding::length(tokens::SPACE_7))
                .min_w(px(0.0))
                .child(
                    div()
                        .truncate()
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
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
                        .map(|bytes| format_bytes(units, bytes))
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
    units: UnitPreferences,
) -> Div {
    let memory = format_pair(
        resources
            .memory_usage_bytes
            .map(|bytes| format_bytes(units, bytes)),
        resources
            .memory_limit
            .map(|value| format_limit(value, labels.unlimited, |b| format_bytes(units, b))),
        labels.unknown,
    );
    let memory = resources
        .memory_usage_percent()
        .map_or(memory.clone(), |percent| {
            format!("{memory} ({percent:.0}%)")
        });
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
    let pids = resources
        .process_usage_percent()
        .map_or(pids.clone(), |percent| format!("{pids} ({percent:.0}%)"));
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
    content = content
        .child(metric_row(
            theme,
            labels.security_profile,
            isolation
                .security_profile
                .clone()
                .unwrap_or_else(|| labels.unknown.to_owned()),
        ))
        .child(metric_row(
            theme,
            labels.seccomp,
            isolation
                .seccomp_mode
                .map_or_else(|| labels.unknown.to_owned(), |mode| mode.to_string()),
        ))
        .child(metric_row(
            theme,
            labels.no_new_privs,
            isolation.no_new_privs.map_or_else(
                || labels.unknown.to_owned(),
                |enabled| {
                    if enabled {
                        labels.yes.to_owned()
                    } else {
                        labels.no.to_owned()
                    }
                },
            ),
        ))
        .child(metric_row(
            theme,
            labels.ptrace_scope,
            isolation
                .yama_ptrace_scope
                .map_or_else(|| labels.unknown.to_owned(), |scope| scope.to_string()),
        ));
    content = content.child(metric_row(
        theme,
        labels.capabilities,
        isolation
            .capabilities
            .as_ref()
            .map(taskmanager_shell::presentation::capabilities_summary)
            .unwrap_or_else(|| labels.unknown.to_owned()),
    ));
    content = content.child(metric_row(
        theme,
        labels.namespaces,
        isolation
            .namespaces
            .as_ref()
            .map(taskmanager_shell::presentation::namespaces_summary)
            .unwrap_or_else(|| labels.unknown.to_owned()),
    ));
    if let Some(details) = taskmanager_shell::presentation::sandbox_details_summary(isolation) {
        content = content.child(metric_row(theme, labels.sandbox_details, details));
    }
    content
}

pub(crate) fn environment_card(
    theme: &Theme,
    snapshot: &ProcessTelemetrySnapshot,
    labels: &ProcessInsightsLabels,
    width: f32,
) -> Div {
    let environment = &snapshot.environment;
    if environment.state.status != DeviceStatus::Healthy {
        return card(theme, labels.environment, width).child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(status_label(environment.state.status, labels).to_string()),
        );
    }
    let mut content = card(theme, labels.environment, width);
    if environment.entries.is_empty() {
        return content.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(labels.no_environment.to_string()),
        );
    }
    let header = if environment.truncated_count > 0 {
        format!(
            "{} · {} · +{}",
            labels.environment,
            environment.entries.len(),
            environment.truncated_count,
        )
    } else {
        format!("{} · {}", labels.environment, environment.entries.len())
    };
    content = content.child(
        div()
            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
            .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
            .child(header),
    );
    let (shown, hidden) = capped_card_rows(environment.entries.len());
    content = content.child(
        div()
            .flex()
            .flex_col()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_3,
            ))
            .children(
                environment
                    .entries
                    .iter()
                    .take(shown)
                    .enumerate()
                    .map(|(i, entry)| {
                        let value =
                            taskmanager_application::process_details_vm::render_environment_value(
                                &entry.key,
                                &entry.value,
                            );
                        KeyValueRow::new(&entry.key, value, theme.palette())
                            .label_width(taskmanager_theme::Length(110.0))
                            .value_align_right(false)
                            .selectable_value(gpui::ElementId::Name(
                                format!("process-insight-env:{i}:{}", entry.key).into(),
                            ))
                            .render()
                    }),
            ),
    );
    if hidden > 0 {
        content = content.child(crate::gpui_app::elements::more_rows_hint(theme, hidden));
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

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_process_insights_view_cap_tests.rs"]
mod cap_tests;

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_process_insights_view_environment_tests.rs"]
mod environment_tests;
