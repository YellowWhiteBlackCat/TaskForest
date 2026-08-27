//! Network-device detail projection for the Iced Performance page.

use std::rc::Rc;

use super::*;
use iced::widget::column;
use taskmanager_theme::tokens;

/// The Performance-page per-NIC panel readiness, mirroring the GPU/disk
/// panels' Loading/Empty/Ready states.
#[must_use]
pub(crate) fn network_section_state(snapshot: Option<&SystemSnapshot>) -> tables::ListState {
    match snapshot {
        None => tables::ListState::Loading,
        Some(snapshot) if snapshot.networks.is_empty() => tables::ListState::Empty,
        Some(_) => tables::ListState::Ready,
    }
}

/// One network adapter's display identity: the typed adapter category (the
/// six-way `NetworkAdapterType` classification, shared with the device rail
/// via [`super::super::perf_rail::network_category_label`]) plus the interface
/// name when known, else the stable device id, else the category alone.
#[must_use]
pub(crate) fn network_title(nic: &NetworkMetrics) -> String {
    let category = super::super::perf_rail::network_category_label(nic.adapter_type());
    let name = (!nic.interface_name.trim().is_empty())
        .then(|| nic.interface_name.trim().to_string())
        .or_else(|| (!nic.device_id.trim().is_empty()).then(|| nic.device_id.trim().to_string()));
    match name {
        Some(name) => format!("{category}: {name}"),
        None => category,
    }
}

/// The single dBm → percentage mapping for Wi-Fi signal quality
/// (-90 dBm → 0%, -30 dBm → 100%, clamped). Shared by the rail caption, the
/// detail summary, and the progress bar so every surface agrees.
#[must_use]
pub(crate) fn wifi_signal_quality_percent(dbm: i32) -> f32 {
    ((dbm as f32 + 90.0) / 60.0 * 100.0).clamp(0.0, 100.0)
}

/// Whether the adapter is currently associated/carrier-up. Falls back to an
/// observed IPv4/IPv6 address when the link state is unknown — never invents
/// a connection.
#[must_use]
pub(crate) fn network_connected(nic: &NetworkMetrics) -> bool {
    match super::projection::NetworkObservation::from(nic).link_up {
        Some(up) => up,
        None => {
            nic.ipv4_addr.as_deref().is_some_and(|a| !a.is_empty())
                || nic.ipv6_addr.as_deref().is_some_and(|a| !a.is_empty())
        }
    }
}

/// The physical NIC model and kernel driver as a dedicated hardware card —
/// only rows with a proven value; a missing driver never renders a dash.
#[must_use]
pub(crate) fn network_hardware_rows(nic: &NetworkMetrics) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if let Some(adapter) = nic.adapter.as_deref().filter(|text| !text.is_empty()) {
        rows.push((t("common.adapter").to_string(), adapter.to_string()));
    }
    if let Some(driver) = nic.driver.as_deref().filter(|text| !text.is_empty()) {
        rows.push((t("common.driver").to_string(), driver.to_string()));
    }
    rows
}

/// Project one network adapter's honest scalar readouts as label/value rows
/// for the Performance page. Unavailable observations render "—" or are
/// omitted entirely so a missing provider cannot masquerade as a measured zero.
#[must_use]
pub(crate) fn network_summary_lines(
    nic: &NetworkMetrics,
    use_bytes: bool,
    use_base2: bool,
) -> Vec<(String, String)> {
    let observed = super::projection::NetworkObservation::from(nic);
    // The Type row carries the typed six-way classification, not the legacy
    // wireless boolean. Association facts below still require WiFi.
    let type_label = super::super::perf_rail::network_category_label(nic.adapter_type());
    let is_wireless = nic.adapter_type() == NetworkAdapterType::WiFi;
    let mut rows = vec![
        (
            t("device.status").to_string(),
            t(device_status_i18n_key(nic.device_state.status)).to_string(),
        ),
        (
            t("net.receive").to_string(),
            rate_text_pref(observed.rx_bytes_per_sec, use_bytes, use_base2),
        ),
        (
            t("net.send").to_string(),
            rate_text_pref(observed.tx_bytes_per_sec, use_bytes, use_base2),
        ),
        (
            t("net.link").to_string(),
            observed
                .link_speed_mbps
                .map(|speed| format!("{speed} Mbps"))
                .unwrap_or_else(missing_value),
        ),
        (t("common.type").to_string(), type_label.to_string()),
    ];

    for (label, value) in [
        (t("net.ipv4"), nic.ipv4_addr.as_deref()),
        (t("net.ipv6"), nic.ipv6_addr.as_deref()),
        (t("net.mac"), nic.mac_addr.as_deref()),
    ] {
        rows.push((
            label.to_string(),
            value
                .filter(|text| !text.is_empty())
                .map_or_else(missing_value, str::to_owned),
        ));
    }

    let connected = network_connected(nic);
    rows.push((
        t("net.connection").to_string(),
        if connected {
            t("common.connected").to_string()
        } else {
            t("common.disconnected").to_string()
        },
    ));

    if let Some(total) = observed.total_rx_bytes {
        rows.push((
            t("net.total_received").to_string(),
            quantity_text_pref(total, use_bytes, use_base2),
        ));
    }
    if let Some(total) = observed.total_tx_bytes {
        rows.push((
            t("net.total_sent").to_string(),
            quantity_text_pref(total, use_bytes, use_base2),
        ));
    }
    if let Some(driver) = nic.driver.as_deref().filter(|text| !text.is_empty()) {
        rows.push((t("common.driver").to_string(), driver.to_string()));
    }
    if let Some(adapter) = nic.adapter.as_deref().filter(|text| !text.is_empty()) {
        rows.push((t("common.adapter").to_string(), adapter.to_string()));
    }

    if observed.link_speed_mbps.is_some() {
        rows.push((
            t("common.utilization").to_string(),
            observed
                .utilization_pct
                .map_or_else(missing_value, |value| format!("{:.0}%", value.round())),
        ));
    }

    if is_wireless {
        if let Some(ssid) = observed.ssid.as_deref() {
            rows.push(("SSID".to_string(), ssid.to_string()));
        }
        if let Some(dbm) = observed.signal_dbm {
            let quality = wifi_signal_quality_percent(dbm);
            rows.push((
                t("common.signal").to_string(),
                format!("{dbm} dBm ({quality:.0}%)"),
            ));
        }
        if let Some(bssid) = observed.bssid.as_deref() {
            rows.push((t("net.bssid").to_string(), bssid.to_owned()));
        }
        let mut details = Vec::new();
        if let Some(protocol) = observed.protocol.as_deref() {
            details.push(format!("{} {protocol}", t("net.protocol")));
        }
        if let Some(channel) = observed.channel {
            details.push(format!("{} {channel}", t("net.channel")));
        }
        if let Some(frequency) = observed.frequency_mhz {
            details.push(format!("{} {frequency} MHz", t("net.frequency")));
        }
        if let Some(rate) = observed.rx_bitrate_mbps {
            details.push(format!("{} {rate} Mbps", t("net.rx_rate")));
        }
        if let Some(rate) = observed.tx_bitrate_mbps {
            details.push(format!("{} {rate} Mbps", t("net.tx_rate")));
        }
        if !details.is_empty() {
            rows.push((t("net.wireless_details").to_string(), details.join(" · ")));
        }
    }
    rows
}

/// The Performance-page per-NIC panel: one block per network adapter in the
/// shared snapshot, each block topped by its own throughput trend.
pub(crate) fn network_section(
    app: &crate::IcedApp,
    index: usize,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let snapshot = app.shell.projection().snapshot.as_ref();
    let theme_snapshot = app.theme();
    let color = theme::color(theme_snapshot.network);
    let rows = match (network_section_state(snapshot), snapshot) {
        (tables::ListState::Loading, _) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
        (tables::ListState::Empty, _) => {
            vec![tables::message_panel(theme_snapshot, t("network.empty"))]
        }
        (tables::ListState::Ready, Some(snapshot)) => match snapshot.networks.get(index) {
            Some(nic) => {
                let observed = super::projection::NetworkObservation::from(nic);
                let mut graph = app.graph_prefs();
                graph.hover = true;
                if !app.network_dynamic_scaling() {
                    graph.max_override = observed
                        .link_speed_mbps
                        .map(|speed_mbps| (speed_mbps as f64 * 1_000_000.0 / 8.0) as f32);
                }
                vec![network_block(
                    app,
                    nic,
                    color,
                    theme_snapshot,
                    app.compact_layout(),
                    app.network_units(),
                    graph,
                )]
            }
            None => vec![tables::message_panel(theme_snapshot, t("network.empty"))],
        },
        (tables::ListState::Ready, None) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
    };
    device_rows_panel(rows, theme_snapshot)
}

fn network_block<'a>(
    app: &'a crate::IcedApp,
    nic: &NetworkMetrics,
    color: iced::Color,
    theme_snapshot: &'a taskmanager_theme::Theme,
    compact: bool,
    units: UnitPrefs,
    graph: device_chart::GraphPrefs,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let observed = super::projection::NetworkObservation::from(nic);
    let (rx_samples, tx_samples) =
        app.cached_network_split_series(&nic.device_id, nic.device_generation.get());
    // Two-series rx/tx graph: the network family token strokes receive, the
    // same token lifted toward white strokes transmit, both resolved through
    // one shared slot grid and one shared max (the link-speed ceiling when
    // dynamic scaling is off) so the directions stay directly comparable;
    // each direction keeps its own gap evidence. The canvas takes the
    // remaining column height on a wide card and the fixed primary height
    // inside a compact scrollable.
    let mut graphs = vec![device_chart::multi::device_multi_graph_fill(
        device_chart::multi::DeviceMultiGraphSpec {
            primary: network_split_series(t("net.receive").to_string(), rx_samples),
            secondary: network_split_series(t("net.send").to_string(), tx_samples),
            family_color: color,
            capacity: app.graph_data_points(),
            format_value: network_throughput_formatter(units),
            prefs: device_chart::GraphPrefs {
                smooth: graph.smooth,
                max_override: graph.max_override,
                hover: graph.hover,
            },
        },
        t("net.throughput").to_string(),
        theme_snapshot,
        compact,
    )];
    let mut copy_actions = Vec::new();
    if let Some(ipv4) = nic.ipv4_addr.as_deref().filter(|s| !s.is_empty()) {
        copy_actions.push(focus::dynamic_button(
            theme_snapshot,
            FocusTarget::AboutCopyDetails,
            format!("IPv4: {ipv4}"),
            Message::CopyTextToClipboard {
                label: "IPv4".to_string(),
                text: ipv4.to_string(),
            },
            false,
        ));
    }
    if let Some(ipv6) = nic.ipv6_addr.as_deref().filter(|s| !s.is_empty()) {
        copy_actions.push(focus::dynamic_button(
            theme_snapshot,
            FocusTarget::AboutCopyDetails,
            format!("IPv6: {ipv6}"),
            Message::CopyTextToClipboard {
                label: "IPv6".to_string(),
                text: ipv6.to_string(),
            },
            false,
        ));
    }
    if let Some(mac) = nic.mac_addr.as_deref().filter(|s| !s.is_empty()) {
        copy_actions.push(focus::dynamic_button(
            theme_snapshot,
            FocusTarget::AboutCopyDetails,
            format!("MAC: {mac}"),
            Message::CopyTextToClipboard {
                label: "MAC".to_string(),
                text: mac.to_string(),
            },
            false,
        ));
    }
    if nic.adapter_type() == NetworkAdapterType::WiFi {
        let mut wifi_items: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();
        // Semantic connection-state dot: healthy+connected reads success,
        // disconnected reads muted, degraded/error states read danger.
        let status_color =
            if nic.device_state.status == DeviceStatus::Healthy && network_connected(nic) {
                theme::color(theme_snapshot.success)
            } else if nic.device_state.status == DeviceStatus::Healthy {
                crate::theme::muted_text_color(theme_snapshot)
            } else {
                theme::color(theme_snapshot.gpu)
            };
        wifi_items.push(
            text("\u{25CF}")
                .size(f32::from(tokens::FONT_10))
                .color(status_color)
                .into(),
        );
        if let Some(ssid) = observed.ssid.as_deref() {
            wifi_items.push(
                text(format!("SSID: {ssid}"))
                    .size(f32::from(tokens::FONT_13))
                    .into(),
            );
        }
        if let Some(dbm) = observed.signal_dbm {
            let quality = wifi_signal_quality_percent(dbm);
            wifi_items.push(
                row![
                    text(format!("{} {dbm} dBm ({quality:.0}%)", t("common.signal")))
                        .size(f32::from(tokens::FONT_12)),
                    container(iced::widget::progress_bar(0.0..=100.0, quality))
                        .width(iced::Length::Fixed(120.0)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center)
                .into(),
            );
        }
        if !network_connected(nic) {
            wifi_items.push(
                text(t("common.disconnected"))
                    .size(f32::from(tokens::FONT_12))
                    .color(crate::theme::muted_text_color(theme_snapshot))
                    .into(),
            );
        }
        if !wifi_items.is_empty() {
            graphs.push(
                container(row(wifi_items).spacing(16).align_y(iced::Alignment::Center))
                    .style(move |_| theme::card_style(theme_snapshot))
                    .padding(8)
                    .width(iced::Length::Fill)
                    .into(),
            );
        }
    }
    let hardware_rows = network_hardware_rows(nic);
    if !hardware_rows.is_empty() {
        let cells: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = hardware_rows
            .iter()
            .map(|(label, value)| {
                row![
                    text(label.clone())
                        .width(iced::Length::Fixed(110.0))
                        .size(f32::from(tokens::FONT_12)),
                    text(value.clone())
                        .size(f32::from(tokens::FONT_12))
                        .width(iced::Length::Fill),
                ]
                .spacing(8)
                .padding(2)
                .width(iced::Length::Fill)
                .into()
            })
            .collect();
        graphs.push(
            container(
                column![
                    text(t("common.hardware")).size(f32::from(tokens::FONT_13)),
                    column(cells).spacing(1),
                ]
                .spacing(4),
            )
            .style(move |_| theme::card_style(theme_snapshot))
            .padding(8)
            .width(iced::Length::Fill)
            .into(),
        );
    }
    if !copy_actions.is_empty() {
        graphs.push(row(copy_actions).spacing(6).into());
    }
    perf_layout::main_with_stats(
        theme_snapshot,
        network_title(nic),
        t("net.throughput").to_string(),
        graphs,
        network_summary_lines(nic, units.use_bytes, units.use_base2),
        compact,
        perf_layout::DetailExtent::for_scroll_parent(compact),
    )
}

/// One legend-labeled series of the NIC's two-series graph. The stroke color
/// is derived from the family token inside the chart factory; `Color::WHITE`
/// here is the placeholder the factory contract overwrites.
fn network_split_series(
    label: String,
    samples: Rc<[f32]>,
) -> device_chart::multi::DeviceMultiSeries {
    device_chart::multi::DeviceMultiSeries {
        samples,
        label,
        color: iced::Color::WHITE,
    }
}

/// The injected unit formatter for the two-series NIC graph's y-axis ticks and
/// hover pill: the same `throughput_scale`/`summary_value` authority the
/// single-series graphs and scalar rows use (decimal bits at the network
/// product default), resolved to a plain `fn` pointer for the resolved network
/// unit pair.
fn network_throughput_formatter(units: UnitPrefs) -> fn(f32) -> String {
    fn pair(use_bytes: bool, use_base2: bool, value: f32) -> String {
        device_chart::summary_value(
            throughput_scale(UnitPrefs {
                use_bytes,
                use_base2,
            }),
            value,
        )
    }
    match (units.use_bytes, units.use_base2) {
        (true, true) => |value| pair(true, true, value),
        (true, false) => |value| pair(true, false, value),
        (false, true) => |value| pair(false, true, value),
        (false, false) => |value| pair(false, false, value),
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/perf_devices/network_split_chart_tests.rs"]
mod split_chart_tests;
