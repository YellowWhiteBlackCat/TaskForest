//! Network performance-page stat readout construction.
//!
//! Row contract (求同存异): a field that exists on this host but whose current
//! sample is missing pushes `None` (the panel renders the shared dash); a
//! field that simply does not exist here (no IPv6 address, unknown link
//! speed) omits its row entirely instead of parking a permanent dash.

use crate::core::metrics::NetworkMetrics;
use crate::i18n;

use crate::gpui_app::formatting::{DisplayUnits, UnitKind};
use taskmanager_shell::viewmodel::StatRow;

use super::{device_status_i18n_key, rate_str};
use crate::gpui_app::sidebar::network_category_label;

pub(super) fn network_stats(
    n: &NetworkMetrics,
    is_wireless: bool,
    units: DisplayUnits,
) -> Vec<StatRow> {
    let mut stats = vec![
        StatRow::text(
            i18n::t("device.status"),
            Some(i18n::t(device_status_i18n_key(n.device_state.status)).into()),
        ),
        StatRow::text(
            i18n::t("net.receive"),
            n.current_rx_bytes_per_sec()
                .map(|value| rate_str(units, value)),
        ),
        StatRow::text(
            i18n::t("net.send"),
            n.current_tx_bytes_per_sec()
                .map(|value| rate_str(units, value)),
        ),
        StatRow::text(
            i18n::t("net.connection"),
            // Authoritative signal is the kernel carrier (current_link_up);
            // fall back to ANY assigned address (IPv4 OR IPv6) when carrier is
            // unknown. Previously this keyed on IPv4 only, so an IPv6-only link
            // (common on IPv6-only networks) wrongly showed "disconnected".
            Some(
                if match n.current_link_up() {
                    Some(up) => up,
                    None => n.ipv4_addr.is_some() || n.ipv6_addr.is_some(),
                } {
                    i18n::t("common.connected").into()
                } else {
                    i18n::t("common.disconnected").into()
                },
            ),
        ),
        StatRow::text(
            i18n::t("net.total_received"),
            n.current_total_rx_bytes()
                .map(|value| units.format(value, UnitKind::Network, false)),
        ),
        StatRow::text(
            i18n::t("net.total_sent"),
            n.current_total_tx_bytes()
                .map(|value| units.format(value, UnitKind::Network, false)),
        ),
        StatRow::text(
            i18n::t("common.type"),
            Some(if is_wireless {
                i18n::t("sidebar.wireless").into()
            } else {
                i18n::t("net.wired").into()
            }),
        ),
    ];
    // ── Address facts: rows exist only when the address exists ──
    if let Some(ipv4) = n.ipv4_addr.as_deref() {
        stats.push(StatRow::text(i18n::t("net.ipv4"), Some(ipv4.to_owned())));
    }
    if let Some(ipv6) = n.ipv6_addr.as_deref() {
        stats.push(StatRow::text(i18n::t("net.ipv6"), Some(ipv6.to_owned())));
    }
    if let Some(mac) = n.mac_addr.as_deref() {
        stats.push(StatRow::text(i18n::t("net.mac"), Some(mac.to_owned())));
    }
    // ── Link speed: absent speed omits the row (and the utilization row
    // below is keyed on the same fact, so the pair appears together). ──
    if let Some(speed) = n.current_link_speed_mbps() {
        stats.push(StatRow::text(
            i18n::t("net.link"),
            Some(format!("{speed} Mbps")),
        ));
        stats.push(StatRow::text(
            i18n::t("common.utilization"),
            n.current_utilization_pct()
                .map(|value| format!("{value:.0}%")),
        ));
    }
    // Optional native driver/model facts. Values arrive through the immutable
    // `NetworkMetrics` read model; render performs no native I/O.
    if let Some(driver) = n.driver.as_deref() {
        stats.push(StatRow::text(
            i18n::t("common.driver"),
            Some(driver.to_owned()),
        ));
    }
    if let Some(adapter) = n.adapter.as_deref() {
        stats.push(StatRow::text(
            i18n::t("common.adapter"),
            Some(adapter.to_owned()),
        ));
    }
    // ── Wireless signal level (dBm; only for associated wireless links) ──
    if let Some(sig) = n.current_signal_dbm() {
        let quality = ((sig as f32 + 90.0) / 60.0 * 100.0).clamp(0.0, 100.0);
        stats.push(StatRow::text(
            i18n::t("common.signal"),
            Some(format!("{sig} dBm ({quality:.0}%)")),
        ));
    }
    if is_wireless {
        if let Some(bssid) = n.current_bssid() {
            stats.push(StatRow::text(i18n::t("net.bssid"), Some(bssid.to_owned())));
        }
        let mut details = Vec::new();
        if let Some(protocol) = n.current_protocol() {
            details.push(format!("{} {protocol}", i18n::t("net.protocol")));
        }
        if let Some(channel) = n.current_channel() {
            details.push(format!("{} {channel}", i18n::t("net.channel")));
        }
        if let Some(frequency) = n.current_frequency_mhz() {
            details.push(format!("{} {frequency} MHz", i18n::t("net.frequency")));
        }
        if let Some(rate) = n.current_rx_bitrate_mbps() {
            details.push(format!("{} {rate} Mbps", i18n::t("net.rx_rate")));
        }
        if let Some(rate) = n.current_tx_bitrate_mbps() {
            details.push(format!("{} {rate} Mbps", i18n::t("net.tx_rate")));
        }
        if !details.is_empty() {
            stats.push(StatRow::text(
                i18n::t("net.wireless_details"),
                Some(details.join(" · ")),
            ));
        }
    }
    stats
}

pub(super) fn network_title(network: &NetworkMetrics, is_wireless: bool) -> String {
    match (is_wireless, network.current_ssid()) {
        (true, Some(ssid)) if !ssid.is_empty() => format!(
            "{}: {} ({})",
            i18n::t("sidebar.wifi"),
            ssid,
            network.interface_name
        ),
        (true, _) => format!(
            "{} ({})",
            i18n::t("sidebar.wireless"),
            network.interface_name
        ),
        (false, _) => format!(
            "{} ({})",
            network_category_label(network.adapter_type()),
            network.interface_name
        ),
    }
}

pub(super) fn network_link_speed_graph_max(network: &NetworkMetrics) -> Option<f32> {
    network_link_speed_graph_max_mbps(network.current_link_speed_mbps())
}

pub(super) fn network_link_speed_graph_max_mbps(speed_mbps: Option<u64>) -> Option<f32> {
    let speed_mbps = speed_mbps?;
    let speed_mbps = u32::try_from(speed_mbps.min(u64::from(u32::MAX))).ok()?;
    let speed_mbps = u16::try_from(speed_mbps).ok()?;
    Some((f32::from(speed_mbps) / 8.0).max(1.0))
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_network_stats_tests.rs"]
mod tests;
