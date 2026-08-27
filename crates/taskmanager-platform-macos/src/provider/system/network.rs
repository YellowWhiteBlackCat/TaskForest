//! Network throughput + per-interface link/Wi-Fi facts for the macOS system
//! domain (split out of `system.rs`).
//!
//! Safety policy (ADR-019): `sysinfo` provides the getifaddrs counters behind a
//! safe API. Per-interface negotiated link speed and carrier state come from a
//! bounded `ifconfig -a` shell-out (`media:` / `status:` lines), and the Wi-Fi
//! SSID comes from `networksetup -getairportnetwork` (the Wi-Fi interface is
//! located via `networksetup -listallhardwareports`). Both are cached ~10 s
//! (link speed and SSID are quasi-static; tool startup is too costly for a
//! per-refresh call). IPv4/IPv6 addresses, utilization and driver fields have no
//! safe accessor yet and stay `None`. On a host without those tools (the Linux
//! crossbuild CI) the link/SSID scalars degrade honestly to typed unavailable
//! states instead of fabricating values.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use taskmanager_core::{
    CumulativeCounter, FailureKind, NetworkAdapterType, NetworkMetrics, NetworkScalarObservations,
    NetworkTelemetryObservation, NetworkWirelessObservations, OptionalObservation,
    ScalarObservation,
};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::NetworkTelemetryProvider;

use taskmanager_platform_portable::run_with_timeout;

use super::{NETWORK_TELEMETRY_PROVIDER, available_source, unavailable_source};

/// Network throughput from `sysinfo` (getifaddrs counters behind a safe API).
/// Per-interface negotiated link speed and carrier state come from a bounded
/// `ifconfig -a` shell-out (`media:` / `status:` lines), and the Wi-Fi SSID
/// comes from `networksetup -getairportnetwork` (the Wi-Fi interface is located
/// via `networksetup -listallhardwareports`). Both are cached ~10 s (link speed
/// and SSID are quasi-static; tool startup is too costly for a per-refresh
/// call). IPv4/IPv6 addresses, utilization and driver fields have no safe
/// accessor yet and stay `None`. On a host without those tools (the Linux
/// crossbuild CI) the link/SSID scalars degrade honestly to typed unavailable
/// states instead of fabricating values.
pub struct MacNetworkTelemetryProvider {
    networks: sysinfo::Networks,
    rate_counters: HashMap<String, (CumulativeCounter, CumulativeCounter)>,
    /// Per-interface link facts (`ifconfig -a`) plus the Wi-Fi SSID
    /// (`networksetup`), co-refreshed at most every ~10 s. Mirrors the Windows
    /// `WinNetworkTelemetryProvider::fresh_link_map` cache pattern.
    facts: NetworkFacts,
    facts_at: Option<Instant>,
}

impl MacNetworkTelemetryProvider {
    pub fn new() -> Self {
        Self {
            networks: sysinfo::Networks::new(),
            rate_counters: HashMap::new(),
            facts: NetworkFacts::default(),
            facts_at: None,
        }
    }

    /// Return the cached link map + Wi-Fi SSID, re-shelling only when older
    /// than ~10 s. Link speed and SSID are quasi-static; this keeps
    /// `ifconfig`/`networksetup` startup cost out of the per-refresh path.
    fn fresh_facts(&mut self, now: Instant) -> &NetworkFacts {
        let stale = self
            .facts_at
            .is_none_or(|at| now.duration_since(at) >= Duration::from_secs(10));
        if stale {
            self.facts = collect_network_facts();
            self.facts_at = Some(now);
        }
        &self.facts
    }
}

impl NetworkTelemetryProvider for MacNetworkTelemetryProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<NetworkTelemetryObservation, ProviderFailure> {
        self.networks.refresh(true);
        let now = Instant::now();
        // Clone out of the cache borrow so we can iterate `&self.networks`.
        let facts = self.fresh_facts(now).clone();

        let mut metrics = Vec::new();
        let mut interfaces = Vec::new();
        for (name, data) in &self.networks {
            if is_loopback(name) {
                continue;
            }
            let (rx_total, tx_total) = (data.total_received(), data.total_transmitted());
            let counters = self.rate_counters.entry(name.clone()).or_default();
            let rx_rate = counters
                .0
                .observe(
                    Ok(rx_total),
                    observed_at_ms,
                    FailureKind::TemporarilyUnavailable,
                )
                .per_second(observed_at_ms);
            let tx_rate = counters
                .1
                .observe(
                    Ok(tx_total),
                    observed_at_ms,
                    FailureKind::TemporarilyUnavailable,
                )
                .per_second(observed_at_ms);

            let mut row = NetworkMetrics::new(Arc::from(name.as_str()));
            row.device_id = Arc::from(format!("macos:nic:{name}"));
            row.mac_addr = Some(Arc::from(data.mac_address().to_string()));
            let link = facts.links.get(&name.to_ascii_lowercase()).copied();
            let scalar_observations = NetworkScalarObservations {
                total_rx_bytes: ScalarObservation::available(rx_total, observed_at_ms),
                total_tx_bytes: ScalarObservation::available(tx_total, observed_at_ms),
                rx_bytes_per_sec: rx_rate,
                tx_bytes_per_sec: tx_rate,
                // `ifconfig` gives negotiated media + carrier state directly.
                // Utilization (rx+tx vs link capacity) is derived elsewhere and
                // stays Unsupported until a safe source exists.
                link_speed_mbps: match link.and_then(|l| l.speed_mbps) {
                    Some(mbps) => ScalarObservation::available(mbps, observed_at_ms),
                    None => ScalarObservation::unavailable(FailureKind::MissingDependency),
                },
                utilization_pct: ScalarObservation::unavailable(FailureKind::Unsupported),
                link_up: match link {
                    Some(adapter) => ScalarObservation::available(adapter.up, observed_at_ms),
                    None => ScalarObservation::unavailable(FailureKind::MissingDependency),
                },
            };
            let mut adapter_type = NetworkAdapterType::Unknown;
            let mut wireless_observations = NetworkWirelessObservations::default();
            // Wi-Fi SSID + association only for the identified Wi-Fi interface;
            // every other interface keeps its default (Unknown) wireless state
            // rather than over-claiming non-applicability for an interface we
            // have not classified.
            if let Some(wifi) = facts.wifi.as_ref()
                && wifi.iface.eq_ignore_ascii_case(name)
            {
                let ssid_obs = match &wifi.ssid {
                    WifiSsidState::Associated(name) => {
                        OptionalObservation::present(Arc::from(name.as_str()), observed_at_ms)
                    }
                    WifiSsidState::NotAssociated => OptionalObservation::absent(observed_at_ms),
                    WifiSsidState::Denied => {
                        OptionalObservation::unavailable(FailureKind::MissingDependency)
                    }
                };
                let association = match &wifi.ssid {
                    WifiSsidState::Associated(_) => {
                        OptionalObservation::present(true, observed_at_ms)
                    }
                    WifiSsidState::NotAssociated => OptionalObservation::absent(observed_at_ms),
                    WifiSsidState::Denied => {
                        OptionalObservation::unavailable(FailureKind::MissingDependency)
                    }
                };
                adapter_type = NetworkAdapterType::WiFi;
                wireless_observations = NetworkWirelessObservations {
                    association,
                    ssid: ssid_obs,
                    // `networksetup -getairportnetwork` does not surface RSSI;
                    // signal_dbm stays honestly unavailable.
                    signal_dbm: OptionalObservation::unavailable(FailureKind::Unsupported),
                    // The same safe macOS command path does not expose the
                    // negotiated AP/link details. Keep each fact typed as
                    // unsupported instead of letting a Rust default look
                    // like a measured zero or empty value.
                    bssid: OptionalObservation::unavailable(FailureKind::Unsupported),
                    frequency_mhz: OptionalObservation::unavailable(FailureKind::Unsupported),
                    channel: OptionalObservation::unavailable(FailureKind::Unsupported),
                    rx_bitrate_mbps: OptionalObservation::unavailable(FailureKind::Unsupported),
                    tx_bitrate_mbps: OptionalObservation::unavailable(FailureKind::Unsupported),
                    protocol: OptionalObservation::unavailable(FailureKind::Unsupported),
                };
            }
            row.apply_observations(adapter_type, scalar_observations, wireless_observations);
            metrics.push(row);
            interfaces.push(name.clone());
        }
        self.rate_counters
            .retain(|name, _| interfaces.iter().any(|current| current == name));

        let sources = if metrics.is_empty() {
            vec![unavailable_source(
                NETWORK_TELEMETRY_PROVIDER,
                FailureKind::TemporarilyUnavailable,
            )]
        } else {
            vec![available_source(NETWORK_TELEMETRY_PROVIDER, metrics.len())]
        };
        Ok(NetworkTelemetryObservation::current(
            metrics,
            observed_at_ms,
            sources,
            Vec::new(),
            Default::default(),
        ))
    }
}

fn is_loopback(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("loopback") || lower.starts_with("lo") || lower == "localhost"
}

/// Per-interface link facts parsed from `ifconfig -a` (route C, ADR-019):
/// the `media:` parenthesized token ("1000baseT") -> Mbps; the `status:` line
/// ("active") -> link_up.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct IfaceLink {
    speed_mbps: Option<u64>,
    up: bool,
}

/// Honest Wi-Fi association state for the identified Wi-Fi interface.
#[derive(Clone, Debug, PartialEq, Eq)]
enum WifiSsidState {
    /// Associated and the SSID was surfaced.
    Associated(String),
    /// Confirmed not associated (no current network).
    NotAssociated,
    /// The tool is absent or refused (e.g. CoreLocation authorization denied
    /// on newer macOS) — degrade honestly rather than fabricate.
    Denied,
}

/// Cached link + Wi-Fi facts. `wifi` is `None` when no Wi-Fi interface was
/// found by `networksetup -listallhardwareports`; `Some(..)` carries the
/// located interface name and its current SSID state.
#[derive(Clone, Default, Debug)]
struct NetworkFacts {
    /// Keyed by lower-cased interface name (matches the sysinfo network name).
    links: HashMap<String, IfaceLink>,
    wifi: Option<WifiFacts>,
}

#[derive(Clone, Debug)]
struct WifiFacts {
    iface: String,
    ssid: WifiSsidState,
}

/// Parse a `media:` parenthesized token ("1000baseT", "100baseTX", "10baseT",
/// "2.5GBASE-T", "10GBASE-T") into Mbps. Returns `None` for an unrecognised
/// form (wireless identifiers like "IEEE802.11", "autoselect" with no parens,
/// or "none") so the scalar degrades honestly rather than fabricating a number.
/// Pure: unit-tested.
fn parse_media_speed(token: &str) -> Option<u64> {
    let token = token.trim();
    let lower = token.to_ascii_lowercase();
    let base_idx = lower.find("base")?;
    let prefix = token[..base_idx].trim();
    let lower_prefix = prefix.to_ascii_lowercase();
    // "2.5GBASE-T" / "10GBASE-T" form: prefix ends in 'G' -> Mbps = G * 1000.
    if let Some(g_idx) = lower_prefix.rfind('g') {
        let num: f64 = prefix[..g_idx].trim().parse().ok()?;
        return Some((num * 1000.0) as u64);
    }
    prefix.parse::<u64>().ok()
}

/// Parse `ifconfig -a` output into a per-interface (lower-cased name) link
/// map. A new interface block begins at a line whose first token ends with ':'
/// at column 0; `media:` and `status:` lines within the block fill the speed
/// and carrier state. Pure: unit-tested.
fn parse_ifconfig_a(stdout: &str) -> HashMap<String, IfaceLink> {
    let mut out: HashMap<String, IfaceLink> = HashMap::new();
    let mut current: Option<String> = None;
    for raw in stdout.lines() {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        // Block start: a token ending in ':' at column 0 (no leading whitespace)
        // AND the ifconfig `flags=` field — so a shell error line like
        // `ifconfig: command not found` (no flags=) is not mistaken for an iface.
        if !raw.starts_with(char::is_whitespace) && line.contains("flags=") {
            let first_token = line.split_once(':').map(|(name, _)| name).unwrap_or(line);
            if !first_token.is_empty()
                && first_token
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
            {
                current = Some(first_token.to_ascii_lowercase());
                // Ensure an entry exists even when no media/status follows.
                out.entry(current.clone().unwrap_or_default()).or_default();
                continue;
            }
        }
        let Some(iface) = current.as_deref() else {
            continue;
        };
        let entry = out.entry(iface.to_string()).or_default();
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("media:") {
            // "media: autoselect (1000baseT <full-duplex>)" -> paren content.
            if let Some(open) = rest.find('(') {
                let inside = &rest[open + 1..];
                let token = inside.split_whitespace().next().unwrap_or("");
                entry.speed_mbps = parse_media_speed(token);
            }
        } else if let Some(rest) = trimmed.strip_prefix("status:") {
            entry.up = rest.trim() == "active";
        }
    }
    out
}

/// Parse `networksetup -getairportnetwork <iface>` stdout into a Wi-Fi SSID
/// state. "Current Wi-Fi Network: <SSID>" -> Associated; a success line
/// without an SSID -> NotAssociated. Pure: unit-tested.
fn parse_airport_network(stdout: &str) -> WifiSsidState {
    const PREFIX: &str = "Current Wi-Fi Network:";
    for raw in stdout.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix(PREFIX) {
            let ssid = rest.trim();
            if ssid.is_empty() {
                return WifiSsidState::NotAssociated;
            }
            return WifiSsidState::Associated(ssid.to_string());
        }
    }
    WifiSsidState::NotAssociated
}

/// Parse `networksetup -listallhardwareports` stdout to locate the Wi-Fi
/// interface name. Returns the device name (e.g. "en0") of the first hardware
/// port named "Wi-Fi" or "AirPort". Pure: unit-tested.
fn parse_wifi_hardware_port(stdout: &str) -> Option<String> {
    let mut want_device = false;
    for raw in stdout.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("Hardware Port:") {
            let name = rest.trim();
            want_device = name == "Wi-Fi" || name.eq_ignore_ascii_case("AirPort");
        } else if want_device && let Some(rest) = line.strip_prefix("Device:") {
            let dev = rest.trim();
            if !dev.is_empty() {
                return Some(dev.to_string());
            }
        }
    }
    None
}

/// Shell out to `ifconfig -a` once per cache window and return per-interface
/// link facts. Empty — so link scalars stay honest `MissingDependency` — when
/// `ifconfig` is absent (Linux CI), exited non-zero, or timed out.
fn ifconfig_link_map() -> HashMap<String, IfaceLink> {
    let mut command = std::process::Command::new("ifconfig");
    command.arg("-a");
    let output = match run_with_timeout(&mut command, Duration::from_secs(2)) {
        Ok(output) if output.status.success() => output,
        _ => return HashMap::new(),
    };
    parse_ifconfig_a(&String::from_utf8_lossy(&output.stdout))
}

/// Shell out to `networksetup -listallhardwareports` to locate the Wi-Fi
/// interface, then to `networksetup -getairportnetwork <iface>` for the
/// current SSID. Returns `None` when no Wi-Fi interface was found; the SSID
/// state is `Denied` when the tool is absent or refused (CoreLocation on
/// newer macOS) so the scalar degrades honestly.
fn collect_wifi_facts() -> Option<WifiFacts> {
    let iface = wifi_hardware_port()?;
    let ssid = airport_network(&iface);
    Some(WifiFacts { iface, ssid })
}

fn wifi_hardware_port() -> Option<String> {
    let mut command = std::process::Command::new("networksetup");
    command.args(["-listallhardwareports"]);
    let output = run_with_timeout(&mut command, Duration::from_secs(2)).ok()?;
    if !output.status.success() {
        return None;
    }
    parse_wifi_hardware_port(&String::from_utf8_lossy(&output.stdout))
}

fn airport_network(iface: &str) -> WifiSsidState {
    let mut command = std::process::Command::new("networksetup");
    command.args(["-getairportnetwork", iface]);
    match run_with_timeout(&mut command, Duration::from_secs(2)) {
        Ok(output) if output.status.success() => {
            parse_airport_network(&String::from_utf8_lossy(&output.stdout))
        }
        // Tool absent (Linux CI) or CoreLocation authorization refused on newer
        // macOS (the call exits non-zero without authorization): degrade
        // honestly instead of reporting an empty/unassociated network.
        _ => WifiSsidState::Denied,
    }
}

/// Collect the cached network facts: link map from `ifconfig -a` and Wi-Fi SSID
/// from `networksetup`. Each call independently fails soft; a missing tool
/// (Linux CI) yields an empty link map and `None` Wi-Fi rather than an error.
fn collect_network_facts() -> NetworkFacts {
    NetworkFacts {
        links: ifconfig_link_map(),
        wifi: collect_wifi_facts(),
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/macos_provider_system_network.rs"]
mod tests;
