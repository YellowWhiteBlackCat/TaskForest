//! Network throughput + per-interface link facts for the Windows system
//! domain (split out of `system.rs`). `sysinfo` supplies the Windows network
//! byte counters, addresses, and operational state behind its safe API. Link
//! capacity is enriched by the bounded `GetIfTable2` seam; no command
//! interpreter is part of the refresh path.

use std::collections::HashMap;
use std::time::Instant;

use std::sync::Arc;

use taskmanager_core::{
    CumulativeCounter, FailureKind, NetworkAdapterType, NetworkMetrics, NetworkScalarObservations,
    NetworkTelemetryObservation, NetworkWirelessObservations, ScalarObservation,
};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::NetworkTelemetryProvider;

use super::{NETWORK_TELEMETRY_PROVIDER, available_source, unavailable_source};

/// Network throughput and adapter metadata from safe sysinfo/native sources.
/// Per-interface rates are counter deltas over the refresh interval. Wireless
/// identifiers and signal data are intentionally unavailable here; the
/// periodic refresh must not request Windows location consent. Link type and
/// capacity still come from the non-location IP Helper metadata query.
pub struct WinNetworkTelemetryProvider {
    networks: sysinfo::Networks,
    rate_counters: HashMap<String, (CumulativeCounter, CumulativeCounter)>,
    rate_started_at: Instant,
    lifecycles: taskmanager_core::DeviceLifecycleRegistry,
}

impl WinNetworkTelemetryProvider {
    pub fn new() -> Self {
        Self {
            networks: sysinfo::Networks::new_with_refreshed_list(),
            rate_counters: HashMap::new(),
            rate_started_at: Instant::now(),
            lifecycles: taskmanager_core::DeviceLifecycleRegistry::new(
                taskmanager_core::DEFAULT_DEVICE_ABSENCE_RETENTION_MS,
            ),
        }
    }
}

impl NetworkTelemetryProvider for WinNetworkTelemetryProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<NetworkTelemetryObservation, ProviderFailure> {
        self.networks.refresh(true);
        self.lifecycles.begin_refresh();
        let (adapter_facts, adapter_failure) =
            match taskmanager_windows_api::enumerate_network_adapters() {
                Ok(adapters) => (
                    Some(
                        adapters
                            .into_iter()
                            .map(|adapter| (adapter.name.clone(), adapter))
                            .collect::<HashMap<_, _>>(),
                    ),
                    None,
                ),
                Err(taskmanager_windows_api::WindowsApiError::Unsupported) => {
                    (None, Some(FailureKind::Unsupported))
                }
                Err(_) => (None, Some(FailureKind::TemporarilyUnavailable)),
            };
        let counter_at_ms =
            u64::try_from(self.rate_started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        let mut metrics = Vec::new();
        for (name, data) in &self.networks {
            let name = name.clone();
            if is_loopback(&name) || is_intermediate_filter(&name) {
                continue;
            }
            let device_id = format!("windows:nic:{name}");
            let device_state = taskmanager_core::DeviceState::healthy(observed_at_ms);
            let lifecycle =
                self.lifecycles
                    .observe(device_id.as_str(), device_state, observed_at_ms);
            let device_generation = taskmanager_core::DeviceGeneration::new(lifecycle.generation);
            let (rx_total, tx_total) = (data.total_received(), data.total_transmitted());
            let counters = self.rate_counters.entry(name.clone()).or_default();
            let rx_rate = counters
                .0
                .observe(
                    Ok(rx_total),
                    counter_at_ms,
                    FailureKind::TemporarilyUnavailable,
                )
                .per_second(observed_at_ms);
            let tx_rate = counters
                .1
                .observe(
                    Ok(tx_total),
                    counter_at_ms,
                    FailureKind::TemporarilyUnavailable,
                )
                .per_second(observed_at_ms);

            let adapter = adapter_facts.as_ref().and_then(|facts| {
                facts.get(&name).or_else(|| {
                    facts.values().find(|a| {
                        a.name.eq_ignore_ascii_case(&name)
                            || a.description.contains(&name)
                            || name.contains(&a.name)
                            || (name.to_lowercase().contains("wi-fi")
                                && a.adapter_type
                                    == taskmanager_windows_api::WindowsAdapterType::WiFi)
                            || (name.to_lowercase().contains("wlan")
                                && a.adapter_type
                                    == taskmanager_windows_api::WindowsAdapterType::WiFi)
                            || (name.to_lowercase().contains("ethernet")
                                && a.adapter_type
                                    == taskmanager_windows_api::WindowsAdapterType::Ethernet)
                            || (name.contains("以太网")
                                && a.adapter_type
                                    == taskmanager_windows_api::WindowsAdapterType::Ethernet)
                    })
                })
            });
            let link_speed_mbps = adapter.and_then(adapter_link_speed_mbps).map_or_else(
                || {
                    ScalarObservation::unavailable(
                        adapter_failure.unwrap_or(FailureKind::Unsupported),
                    )
                },
                |speed| ScalarObservation::available(speed, observed_at_ms),
            );
            let link_up = adapter
                .and_then(|facts| facts.link_up)
                .or_else(|| operational_state_link_up(data.operational_state()))
                .map_or_else(
                    || ScalarObservation::unavailable(FailureKind::Unsupported),
                    |up| ScalarObservation::available(up, observed_at_ms),
                );
            let (ipv4_addr, ipv6_addr) = addresses(data.ip_networks());

            let adapter_type = match adapter.map(|a| a.adapter_type) {
                Some(taskmanager_windows_api::WindowsAdapterType::WiFi) => NetworkAdapterType::WiFi,
                Some(taskmanager_windows_api::WindowsAdapterType::Ethernet) => {
                    NetworkAdapterType::Ethernet
                }
                Some(taskmanager_windows_api::WindowsAdapterType::Vpn) => NetworkAdapterType::Vpn,
                Some(taskmanager_windows_api::WindowsAdapterType::Virtual) => {
                    NetworkAdapterType::Virtual
                }
                Some(taskmanager_windows_api::WindowsAdapterType::Loopback) => {
                    NetworkAdapterType::Loopback
                }
                Some(taskmanager_windows_api::WindowsAdapterType::Other) => {
                    NetworkAdapterType::Other
                }
                None => NetworkAdapterType::Unknown,
            };

            let adapter_description = adapter
                .map(|a| a.description.as_str().trim())
                .filter(|desc| !desc.is_empty())
                .map(Arc::from);

            let wireless_observations = wireless_observations_for(adapter_type, observed_at_ms);
            let mut row = NetworkMetrics::new(Arc::from(name.clone()));
            row.device_id = format!("windows:nic:{name}").into();
            row.device_generation = device_generation;
            row.device_state = device_state;
            row.ipv4_addr = ipv4_addr;
            row.ipv6_addr = ipv6_addr;
            row.mac_addr = Some(data.mac_address().to_string().into());
            row.adapter = adapter_description;
            let utilization_pct =
                derive_utilization_pct(rx_rate, tx_rate, link_speed_mbps, observed_at_ms);
            let observations = NetworkScalarObservations {
                total_rx_bytes: ScalarObservation::available(rx_total, observed_at_ms),
                total_tx_bytes: ScalarObservation::available(tx_total, observed_at_ms),
                rx_bytes_per_sec: rx_rate,
                tx_bytes_per_sec: tx_rate,
                link_speed_mbps,
                utilization_pct,
                link_up,
            };
            row.apply_observations(adapter_type, observations, wireless_observations);
            metrics.push(row);
        }

        let outcome = if metrics.is_empty() {
            taskmanager_core::DeviceRefreshOutcome::Unavailable(
                taskmanager_core::DeviceStatus::Stale,
            )
        } else {
            taskmanager_core::DeviceRefreshOutcome::Complete
        };
        let _delta = self.lifecycles.finish_refresh(outcome, observed_at_ms);
        let lifecycles = self
            .lifecycles
            .iter()
            .map(|(id, l)| (taskmanager_core::DeviceId::new(id), *l))
            .collect::<std::collections::BTreeMap<_, _>>();

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
            lifecycles,
        ))
    }
}

fn adapter_link_speed_mbps(
    adapter: &taskmanager_windows_api::WindowsNetworkAdapter,
) -> Option<u64> {
    let bits_per_sec = adapter
        .receive_link_speed_bps
        .into_iter()
        .chain(adapter.transmit_link_speed_bps)
        .max()?;
    let megabits = bits_per_sec / 1_000_000;
    (megabits > 0).then_some(megabits)
}

fn operational_state_link_up(state: sysinfo::InterfaceOperationalState) -> Option<bool> {
    match state {
        sysinfo::InterfaceOperationalState::Up => Some(true),
        sysinfo::InterfaceOperationalState::Down
        | sysinfo::InterfaceOperationalState::Testing
        | sysinfo::InterfaceOperationalState::Dormant
        | sysinfo::InterfaceOperationalState::NotPresent
        | sysinfo::InterfaceOperationalState::LowerLayerDown => Some(false),
        sysinfo::InterfaceOperationalState::Unknown => None,
        _ => None,
    }
}

fn addresses(
    networks: &[sysinfo::IpNetwork],
) -> (Option<std::sync::Arc<str>>, Option<std::sync::Arc<str>>) {
    let mut ipv4 = None;
    let mut ipv6 = None;
    for network in networks {
        match network.addr {
            std::net::IpAddr::V4(address) if ipv4.is_none() => {
                ipv4 = Some(address.to_string().into());
            }
            std::net::IpAddr::V6(address) if ipv6.is_none() => {
                ipv6 = Some(address.to_string().into());
            }
            _ => {}
        }
    }
    (ipv4, ipv6)
}

fn is_loopback(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("loopback") || matches!(lower.as_str(), "lo" | "lo0" | "localhost")
}

fn is_intermediate_filter(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("filter")
        || lower.contains("packet scheduler")
        || lower.contains("lightweight")
        || lower.contains("-wfp")
        || lower.contains("-qos")
        || lower.contains("native wifi")
        || lower.contains("-0000")
        || lower.contains("ndis")
}

fn wireless_observations_for(
    adapter_type: NetworkAdapterType,
    observed_at_ms: u64,
) -> NetworkWirelessObservations {
    if adapter_type == NetworkAdapterType::WiFi {
        NetworkWirelessObservations::unavailable(FailureKind::Unsupported)
    } else {
        NetworkWirelessObservations::not_applicable(observed_at_ms)
    }
}

/// Percent of link capacity used by rx+tx, as a pure function of byte rates and
/// a future native link-speed observation. Mirrors the Linux adapter's
/// `link_utilization_pct` byte-for-byte: the platform DAG forbids importing it
/// across crates, so the formula is re-implemented locally — the single source
/// of truth is the *formula*, kept in lockstep. Saturating on both ends:
/// `(rx+tx) bytes/s` as bits/s over `link_speed_mbps * 1_000_000` bits/s,
/// clamped to 0..100.
fn link_utilization_pct(rx_bytes_per_sec: u64, tx_bytes_per_sec: u64, link_speed_mbps: u64) -> f32 {
    let bytes_per_second = rx_bytes_per_sec.saturating_add(tx_bytes_per_sec);
    let capacity_bits_per_second = link_speed_mbps as f64 * 1_000_000.0;
    (bytes_per_second as f64 * 8.0 / capacity_bits_per_second * 100.0).clamp(0.0, 100.0) as f32
}

/// Derive `utilization_pct` from rx+tx byte rates against link capacity, in the
/// same shape as the Linux adapter's `observe_utilization` first-arm: link
/// capacity known -> computed ratio (available); link capacity unknown -> the
/// scalar rides the SAME typed failure as `link_speed_mbps` (honest
/// unavailable, never a fabricated 0). Windows rates are always fresh (counter
/// deltas over the refresh interval), so there is no previous-retention or
/// partial-failure path here, unlike Linux.
fn derive_utilization_pct(
    rx_bytes_per_sec: ScalarObservation<u64>,
    tx_bytes_per_sec: ScalarObservation<u64>,
    link_speed_mbps: ScalarObservation<u64>,
    observed_at_ms: u64,
) -> ScalarObservation<f32> {
    match (
        rx_bytes_per_sec.current_value().copied(),
        tx_bytes_per_sec.current_value().copied(),
        link_speed_mbps.current_value().copied(),
    ) {
        (Some(rx), Some(tx), Some(mbps)) => {
            ScalarObservation::available(link_utilization_pct(rx, tx, mbps), observed_at_ms)
        }
        _ => {
            let failure = rx_bytes_per_sec
                .availability()
                .failure()
                .or_else(|| tx_bytes_per_sec.availability().failure())
                .or_else(|| link_speed_mbps.availability().failure())
                .unwrap_or(FailureKind::Unsupported);
            ScalarObservation::unavailable(failure)
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_system_network.rs"]
mod tests;
