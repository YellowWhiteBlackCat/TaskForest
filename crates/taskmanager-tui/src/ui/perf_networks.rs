//! Per-network-adapter detail block for the Performance page.
//!
//! Reads the live snapshot's `networks` vector through the typed `Option`-
//! returning accessors so an unavailable field renders an honest dash instead
//! of a fabricated zero. Read-only consume of `taskmanager_application::
//! NetworkMetrics`; this crate never mutates the shared snapshot shape. The
//! accessor names mirror `crates/taskmanager-gpui/src/gpui_app/perf_views/network_stats.rs` so the two
//! frontends agree on what "unavailable" means for each NIC field.
//!
//! Render contract: the Performance resource selector hands this section the
//! full content area of the Network tab; the section renders nothing for a
//! zero-height area and an honest empty panel for an empty vector, so a cold
//! host never reads as a fabricated idle NIC. Each NIC row carries its OWN
//! two-row receive/transmit throughput trend (the split-direction windows
//! from that interface's own `LiveGraphHistory`); the resource tab
//! deliberately keeps the per-device history authoritative.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Paragraph, Wrap};

use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::{NetworkAdapterType, NetworkMetrics};
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::{MISSING_VALUE, device_status_i18n_key};
use taskmanager_ui_contract::IconId;

use crate::TuiApp;
use crate::TuiTheme;

/// Render the per-network-adapter detail section into `area`. A zero-height
/// area (the small-terminal case where no panel was allocated) renders nothing.
/// Each NIC row carries its OWN two-row receive/transmit throughput trend
/// (that interface's split-direction windows from the shared
/// `LiveGraphHistory`), so per-device is the point; the system-wide headline
/// is omitted. An empty vector renders an honest empty panel — never a
/// fabricated idle NIC.
pub(super) fn render_network_section(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    networks: &[NetworkMetrics],
) {
    if area.height == 0 {
        return;
    }
    // The applied subcategory visibility filters the NIC list (GPUI Settings
    // devices parity): a hidden class drops out of the panel entirely.
    let filtered = visible_networks(app, networks);
    if filtered.is_empty() {
        super::render_empty_panel(
            frame,
            theme,
            area,
            t("sidebar.network"),
            // Aligned with the shared `network.empty` catalog entry the other
            // frontends read (single-source wording).
            t("network.empty"),
        );
        return;
    }
    let lines = network_lines(
        &filtered,
        app,
        theme,
        app.prefs.units[4],
        app.prefs.units[5],
        app.prefs.graph_points,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .block(super::panel(t("sidebar.network"), theme))
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// Build one honest detail line set per NIC. Each line's unavailable fields
/// resolve to "—" through the shared observers so a cold/unprobed adapter
/// never reads as a fabricated 0% / 0 Mbps / "0 B/s". Each NIC also gets its
/// OWN two-row receive/transmit throughput trend (that interface's
/// split-direction windows from its `LiveGraphHistory`, one shared scale)
/// right under its header; a direction with <2 finite samples renders the
/// dotted placeholder instead of a fabricated flat line. Rates and
/// cumulative totals honor the applied network unit pair (bytes/bits ×
/// base-2/base-10).
/// The NIC list filtered by the applied network-subcategory visibility
/// (Ethernet → Wired; WiFi → Wireless; Vpn → VPN; Virtual → Virtual;
/// Loopback/Other → Other). The whole-family toggle is enforced by the caller
/// (the digit rail) and here alike: an empty visible list renders the honest
/// empty state, never a fabricated NIC.
fn visible_networks<'a>(app: &TuiApp, networks: &'a [NetworkMetrics]) -> Vec<&'a NetworkMetrics> {
    let show = &app.prefs.show;
    let visible = NetworkClassVisibility {
        wired: show[4],
        wireless: show[5],
        vpn: show[6],
        virtual_devices: show[7],
        other: show[8],
    };
    let mut result = Vec::with_capacity(networks.len());
    for network in networks {
        if visible.allows(network.adapter_type()) {
            result.push(network);
        }
    }
    result
}

#[derive(Clone, Copy)]
struct NetworkClassVisibility {
    wired: bool,
    wireless: bool,
    vpn: bool,
    virtual_devices: bool,
    other: bool,
}

impl NetworkClassVisibility {
    fn allows(self, adapter_type: taskmanager_core::core::metrics::NetworkAdapterType) -> bool {
        match adapter_type {
            taskmanager_core::core::metrics::NetworkAdapterType::Ethernet => self.wired,
            taskmanager_core::core::metrics::NetworkAdapterType::WiFi => self.wireless,
            taskmanager_core::core::metrics::NetworkAdapterType::Vpn => self.vpn,
            taskmanager_core::core::metrics::NetworkAdapterType::Virtual => self.virtual_devices,
            taskmanager_core::core::metrics::NetworkAdapterType::Unknown
            | taskmanager_core::core::metrics::NetworkAdapterType::Loopback
            | taskmanager_core::core::metrics::NetworkAdapterType::Other => self.other,
        }
    }
}

fn network_lines(
    networks: &[&NetworkMetrics],
    shell: &ShellApp,
    theme: TuiTheme,
    use_bytes: bool,
    use_base2: bool,
    graph_window: usize,
) -> Vec<ratatui::text::Line<'static>> {
    let mut lines = Vec::with_capacity(networks.iter().map(|n| network_body_line_count(n)).sum());
    for network in networks {
        let data = super::perf_data::network_data(network, use_bytes, use_base2);
        // Header: icon + interface + adapter type. The shared application
        // surface exposes the typed adapter classification directly. A Wi-Fi
        // adapter reads "Wireless" and every other adapter reads "Wired".
        let kind = if network.adapter_type() == NetworkAdapterType::WiFi {
            t("sidebar.wireless").to_string()
        } else {
            t("net.wired").to_string()
        };
        lines.push(ratatui::text::Line::from(format!(
            "{} {} · {}",
            theme.glyph(IconId::Network),
            network.interface_name,
            kind,
        )));
        // Device health verdict (GPUI network_stats first stat; shared
        // presentation single-source). The typed DeviceStatus vocabulary
        // expresses degraded/stale, permission-denied and missing-tool states
        // — facts the carrier-based Connected/Disconnected link verdict below
        // deliberately cannot carry.
        lines.push(ratatui::text::Line::from(format!(
            "  {} {}",
            t("device.status"),
            t(device_status_i18n_key(network.device_state.status)),
        )));
        // Per-NIC throughput trend: this interface's own receive and transmit
        // windows (the split-direction companions of the summed series, same
        // stable key the recorder uses) as two label-prefixed rows on ONE
        // shared scale, so the directions read as comparable amplitudes.
        // Receive keeps the network family color; transmit rides the dim
        // variant — the TUI counterpart of the iced same-hue lift. A direction
        // with <2 finite samples renders the dotted "collecting"
        // placeholder; a missing sample inside a live row renders a gap dot.
        let rx_window = shell
            .history
            .network_rx_bytes_per_sec_for(&network.device_id, network.device_generation.get());
        let tx_window = shell
            .history
            .network_tx_bytes_per_sec_for(&network.device_id, network.device_generation.get());
        let trend = super::sparkline::device_dual_trend_in(
            theme.terminal.glyphs,
            &rx_window,
            &tx_window,
            graph_window,
        );
        let label_width =
            super::text::cell_width(t("net.receive")).max(super::text::cell_width(t("net.send")));
        lines.push(super::sparkline::dual_trend_line(
            t("net.receive"),
            label_width,
            &trend.primary,
            Style::new().fg(theme.good),
        ));
        lines.push(super::sparkline::dual_trend_line(
            t("net.send"),
            label_width,
            &trend.secondary,
            Style::new().fg(theme.dim),
        ));
        // The total-throughput summary stays on the summed window: the two
        // direction rows carry the shape, this line carries the rx+tx total
        // statistics.
        let window = shell
            .history
            .network_bytes_per_sec_for(&network.device_id, network.device_generation.get());
        if let Some(summary) = super::sparkline::device_summary_line_in(
            theme.terminal.glyphs,
            t("common.throughput"),
            &window,
            super::sparkline::DeviceSummaryUnit::BytesPerSecond,
        ) {
            lines.push(ratatui::text::Line::from(format!("  {summary}")));
        }
        // rx/tx rate + utilization. Each scalar is independently unavailable;
        // a confirmed measured zero stays visible while a provider failure or
        // an unknown link speed (utilization is only meaningful then) renders
        // "—". Rates honor the applied unit pair.
        lines.push(ratatui::text::Line::from(format!(
            "  ↓ {}/s · ↑ {}/s · {} {}",
            data.rx,
            data.tx,
            t("common.utilization"),
            data.utilization,
        )));
        // Negotiated link speed + connection state. A missing link speed is an
        // honest dash; the connection verdict mirrors network_stats.rs — the
        // authoritative carrier observation, falling back to any assigned
        // address so an IPv6-only link is not wrongly "Disconnected".
        lines.push(ratatui::text::Line::from(format!(
            "  {} {} · {} {}",
            t("net.link"),
            data.link,
            t("net.connection"),
            data.connection,
        )));
        // Assigned addresses + hardware MAC. Each is independently optional; the
        // line renders only when at least one is present so an unprobed NIC
        // prints nothing rather than three dashes. IPv6 can be long; the panel
        // wraps so nothing is silently dropped.
        let ipv4 = network
            .ipv4_addr
            .as_deref()
            .filter(|addr| !addr.is_empty())
            .unwrap_or(MISSING_VALUE);
        let ipv6 = network
            .ipv6_addr
            .as_deref()
            .filter(|addr| !addr.is_empty())
            .unwrap_or(MISSING_VALUE);
        let mac = network
            .mac_addr
            .as_deref()
            .filter(|addr| !addr.is_empty())
            .unwrap_or(MISSING_VALUE);
        let any_address = network.ipv4_addr.as_deref().is_some_and(|a| !a.is_empty())
            || network.ipv6_addr.as_deref().is_some_and(|a| !a.is_empty())
            || network.mac_addr.as_deref().is_some_and(|a| !a.is_empty());
        if any_address {
            lines.push(ratatui::text::Line::from(format!(
                "  {} {} · {} {} · {} {}",
                t("net.ipv4"),
                ipv4,
                t("net.ipv6"),
                ipv6,
                t("net.mac"),
                mac,
            )));
        }
        // Cumulative transferred totals, only when the provider exposes them.
        // Totals honor the applied unit pair like the live rates.
        if let Some((total_rx, total_tx)) = data.totals.as_ref() {
            lines.push(ratatui::text::Line::from(format!(
                "  {} {} · {} {}",
                t("net.total_received"),
                total_rx,
                t("net.total_sent"),
                total_tx,
            )));
        }
        // Native driver/module + adapter model, only when reported.
        let driver = network.driver.as_deref().filter(|value| !value.is_empty());
        let adapter = network.adapter.as_deref().filter(|value| !value.is_empty());
        if driver.is_some() || adapter.is_some() {
            lines.push(ratatui::text::Line::from(format!(
                "  {} {} · {} {}",
                t("common.driver"),
                driver.unwrap_or(MISSING_VALUE),
                t("common.adapter"),
                adapter.unwrap_or(MISSING_VALUE),
            )));
        }
        // Wireless-only association: SSID + signal level. Renders nothing for a
        // wired adapter (honest absence, not a fabricated "— dBm" line). Either
        // field may be unavailable independently for an unassociated but
        // wireless interface. The derived quality percentage (GPUI parity) is
        // painted ONLY from an observed dBm — a missing signal stays an honest
        // dash.
        if let Some(wireless) = data.wireless.as_ref() {
            // No existing catalog key for the "SSID" label; kept English by the
            // i18n rule (do not edit locales) and listed in the task notes.
            lines.push(ratatui::text::Line::from(format!(
                "  {} {} · SSID {}",
                t("common.signal"),
                wireless.signal,
                wireless.ssid,
            )));
            if let Some(bssid) = wireless.bssid.as_deref() {
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("net.bssid"),
                    bssid,
                )));
            }
            if !wireless.details.is_empty() {
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("net.wireless_details"),
                    wireless.details.join(" · "),
                )));
            }
        }
    }
    lines
}

/// The number of body lines one NIC contributes: header + device status + two
/// direction trends + summary + rates + link (always seven), plus at most the
/// address, totals and driver/adapter rows, plus one honest wireless line for
/// a wireless adapter. Kept as a loose upper bound for the line buffer
/// preallocation.
fn network_body_line_count(network: &NetworkMetrics) -> usize {
    if network.adapter_type() == NetworkAdapterType::WiFi {
        13
    } else {
        10
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_networks_tests.rs"]
mod tests;
