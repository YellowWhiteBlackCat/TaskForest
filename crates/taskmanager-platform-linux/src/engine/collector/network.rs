//! Linux network-domain collection with one explicit lifecycle authority.
//!
//! `/sys/class/net` is the only source allowed to confirm presence or absence.
//! Counter, address, wireless, and `iw` failures are retained as enrichment
//! status and never erase an interface discovered by sysfs.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use sysinfo::Networks;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{
    NetworkAdapterType, NetworkMetrics, NetworkScalarObservations, NetworkWirelessObservations,
};
use taskmanager_core::{
    DeviceId, DeviceState, OptionalObservation, ProviderId, ScalarObservation, SourceOutcome,
    SourceStatus,
};
use taskmanager_platform_contract::{DeviceDiscovery, DeviceSourceSnapshot};

use crate::engine::hardware::is_virtual_interface;

mod counters;
mod sources;
use counters::{
    CounterObservation, CounterValues, NetworkCounterState, RawCounterSample, link_utilization_pct,
    observe_counter_samples,
};
use sources::{
    InterfaceAddresses, IwLinkResult, SourceObservation, SysfsInterface, SysfsInventoryObservation,
};

const SYSFS_INVENTORY_PROVIDER: &str = "linux.network.sysfs-inventory";
const SYSFS_METADATA_PROVIDER: &str = "linux.network.sysfs-metadata";
const SYSINFO_COUNTER_PROVIDER: &str = "linux.network.sysinfo-counters";
const ADDRESS_PROVIDER: &str = "linux.network.address-enumeration";
const PROC_WIRELESS_PROVIDER: &str = "linux.network.proc-wireless";
const IW_PROVIDER: &str = "linux.network.iw-link";

pub(super) type NetworkDomainSnapshot = DeviceSourceSnapshot<Vec<NetworkMetrics>>;

#[derive(Clone, Debug, Default)]
pub(super) struct NetworkCollectionState {
    last_inventory: Vec<SysfsInterface>,
    counters: NetworkCounterState,
    observations: NetworkObservationState,
}

impl NetworkCollectionState {
    pub(super) fn reset_absent(&mut self, device_ids: &[DeviceId]) {
        self.counters.reset_absent(device_ids);
        self.observations.reset_absent(device_ids);
    }

    pub(super) fn confirm_reappeared(&mut self, device_ids: &[DeviceId]) {
        self.counters.confirm_reappeared(device_ids);
        self.observations.confirm_reappeared(device_ids);
    }

    pub(super) fn expire(&mut self, device_ids: &[DeviceId]) {
        self.counters.expire(device_ids);
        self.observations.expire(device_ids);
    }
}

#[derive(Clone, Debug, Default)]
struct NetworkObservationState {
    by_device: HashMap<String, (NetworkScalarObservations, NetworkWirelessObservations)>,
    awaiting_reappearance: HashSet<String>,
}

impl NetworkObservationState {
    fn reconcile(&mut self, metrics: &mut [NetworkMetrics]) {
        for metric in metrics {
            let scalar_observations = *metric.scalar_observations();
            let wireless_observations = metric.wireless_observations().clone();
            let (scalar_observations, wireless_observations) =
                if let Some((previous_scalars, previous_wireless)) =
                    self.by_device.get_mut(metric.device_id.as_ref())
                {
                    (
                        scalar_observations.retain_previous(*previous_scalars),
                        wireless_observations.retain_previous(std::mem::take(previous_wireless)),
                    )
                } else {
                    (scalar_observations, wireless_observations)
                };
            let adapter_type = metric.adapter_type();
            metric.apply_observations(
                adapter_type,
                scalar_observations,
                wireless_observations.clone(),
            );
            self.by_device.insert(
                metric.device_id.as_ref().to_owned(),
                (scalar_observations, wireless_observations),
            );
        }
    }

    fn reset_absent(&mut self, device_ids: &[DeviceId]) {
        for device_id in device_ids {
            self.by_device.remove(device_id.as_str());
            self.awaiting_reappearance
                .insert(device_id.as_str().to_owned());
        }
    }

    fn confirm_reappeared(&mut self, device_ids: &[DeviceId]) {
        for device_id in device_ids {
            if !self.awaiting_reappearance.remove(device_id.as_str()) {
                // Defensive generation boundary: a reappearance receipt
                // without a matching absence must not inherit old field truth.
                self.by_device.remove(device_id.as_str());
            }
        }
    }

    fn expire(&mut self, device_ids: &[DeviceId]) {
        for device_id in device_ids {
            self.by_device.remove(device_id.as_str());
            self.awaiting_reappearance.remove(device_id.as_str());
        }
    }
}

#[derive(Clone, Debug)]
struct ResolvedInventory {
    interfaces: Vec<SysfsInterface>,
    discovered_devices: Vec<DeviceId>,
    outcome: SourceOutcome,
    metadata_outcome: SourceOutcome,
    metadata_item_count: usize,
    fresh_interfaces: HashSet<Arc<str>>,
}

/// Collect one network-domain observation.
///
/// The caller refreshes `sysinfo::Networks`; this helper associates those
/// counters with the independently discovered sysfs inventory.
pub(super) fn collect_network_domain(
    networks: &Networks,
    state: &mut NetworkCollectionState,
    now: Instant,
    now_ms: u64,
) -> NetworkDomainSnapshot {
    collect_network_domain_from_paths(
        networks,
        state,
        Path::new("/sys/class/net"),
        Path::new("/proc/net/wireless"),
        now,
        now_ms,
    )
}

fn collect_network_domain_from_paths(
    networks: &Networks,
    state: &mut NetworkCollectionState,
    sysfs_root: &Path,
    proc_wireless_path: &Path,
    now: Instant,
    now_ms: u64,
) -> NetworkDomainSnapshot {
    let inventory = resolve_inventory(state, sources::read_sysfs_inventory(sysfs_root, now_ms));
    let counters = observe_counters(
        networks,
        &inventory.interfaces,
        &inventory.fresh_interfaces,
        inventory.outcome,
        &mut state.counters,
        now,
        now_ms,
    );
    let addresses = scope_map(sources::enumerate_addresses(), &inventory.interfaces);
    let wireless = scope_map(
        sources::read_proc_wireless(proc_wireless_path),
        &inventory.interfaces,
    );
    let wireless_interfaces = inventory
        .interfaces
        .iter()
        .filter(|interface| {
            matches!(
                classify_adapter_type(&interface.name, interface.arp_type),
                NetworkAdapterType::WiFi
            )
        })
        .map(|interface| interface.name.clone())
        .collect::<Vec<_>>();
    let iw = sources::read_iw_links(wireless_interfaces);

    let mut snapshot = assemble_snapshot(inventory, counters, addresses, wireless, iw, now_ms);
    state.observations.reconcile(&mut snapshot.value);
    snapshot
}

fn resolve_inventory(
    state: &mut NetworkCollectionState,
    mut observed: SysfsInventoryObservation,
) -> ResolvedInventory {
    let fresh_interfaces = observed
        .value
        .iter()
        .map(|interface| interface.name.clone())
        .collect::<HashSet<_>>();
    for interface in &mut observed.value {
        if let Some(previous) = state
            .last_inventory
            .iter()
            .find(|previous| previous.name == interface.name)
            && interface.mac_addr.is_none()
        {
            interface.mac_addr.clone_from(&previous.mac_addr);
        }
    }
    let discovered_devices = observed
        .value
        .iter()
        .map(|interface| DeviceId::new(interface.stable_id.as_ref().to_owned()))
        .collect();

    let mut interfaces = match observed.discovery_outcome {
        SourceOutcome::Available | SourceOutcome::Empty => {
            state.last_inventory.clone_from(&observed.value);
            observed.value
        }
        SourceOutcome::Partial(_) => {
            let mut merged = state
                .last_inventory
                .iter()
                .cloned()
                .map(|interface| (interface.name.clone(), interface))
                .collect::<BTreeMap<_, _>>();
            for interface in observed.value {
                merged.insert(interface.name.clone(), interface);
            }
            merged.into_values().collect()
        }
        SourceOutcome::Unavailable(_) => state.last_inventory.clone(),
    };
    if let Some(failure) = source_failure(observed.discovery_outcome) {
        for interface in &mut interfaces {
            if !fresh_interfaces.contains(&interface.name) {
                interface.link_speed =
                    ScalarObservation::unavailable(failure).retain_previous(interface.link_speed);
                interface.link_up =
                    ScalarObservation::unavailable(failure).retain_previous(interface.link_up);
            }
        }
    }
    if matches!(observed.discovery_outcome, SourceOutcome::Partial(_)) {
        state.last_inventory.clone_from(&interfaces);
    }
    ResolvedInventory {
        interfaces,
        discovered_devices,
        outcome: observed.discovery_outcome,
        metadata_outcome: observed.metadata_outcome,
        metadata_item_count: observed.metadata_item_count,
        fresh_interfaces,
    }
}

fn scope_map<T>(
    mut observed: SourceObservation<HashMap<String, T>>,
    inventory: &[SysfsInterface],
) -> SourceObservation<HashMap<String, T>> {
    observed.value.retain(|name, _| {
        inventory
            .iter()
            .any(|interface| interface.name.as_ref() == name.as_str())
    });
    observed.outcome = match (observed.value.is_empty(), observed.outcome) {
        (true, SourceOutcome::Available) => SourceOutcome::Empty,
        (true, SourceOutcome::Partial(failure)) => SourceOutcome::Unavailable(failure),
        (_, outcome) => outcome,
    };
    observed
}

fn observe_counters(
    networks: &Networks,
    inventory: &[SysfsInterface],
    fresh_interfaces: &HashSet<Arc<str>>,
    discovery_outcome: SourceOutcome,
    state: &mut NetworkCounterState,
    now: Instant,
    now_ms: u64,
) -> CounterObservation {
    let mut samples = HashMap::new();
    for interface in inventory {
        if let Some(data) = networks.get(interface.name.as_ref()) {
            samples.insert(
                interface.name.clone(),
                RawCounterSample {
                    total_rx_bytes: data.total_received(),
                    total_tx_bytes: data.total_transmitted(),
                },
            );
        }
    }
    observe_counter_samples(
        state,
        inventory,
        fresh_interfaces,
        discovery_outcome,
        &samples,
        now,
        now_ms,
    )
}

fn assemble_snapshot(
    inventory: ResolvedInventory,
    counters: CounterObservation,
    mut addresses: SourceObservation<HashMap<String, InterfaceAddresses>>,
    mut wireless: SourceObservation<HashMap<String, i32>>,
    mut iw: SourceObservation<HashMap<Arc<str>, IwLinkResult>>,
    now_ms: u64,
) -> NetworkDomainSnapshot {
    let metrics = inventory
        .interfaces
        .iter()
        .map(|interface| {
            let counter = counters
                .value
                .get(&interface.name)
                .copied()
                .unwrap_or_else(|| CounterValues::unavailable(FailureKind::TemporarilyUnavailable));
            // Consume the per-interface entries so the wire build moves the
            // address/wireless values instead of cloning them per tick.
            let address = addresses.value.remove(interface.name.as_ref());
            let address = address.map(|address| (address.ipv4, address.ipv6));
            let adapter_type = classify_adapter_type(&interface.name, interface.arp_type);
            let is_wireless = matches!(adapter_type, NetworkAdapterType::WiFi);
            let (link_speed_mbps, utilization_pct) =
                backfill_wireless_link_speed(WirelessLinkBackfill {
                    is_wireless,
                    sysfs_link_speed: interface.link_speed,
                    counter_utilization: counter.utilization,
                    rx_rate: counter.rx_rate,
                    tx_rate: counter.tx_rate,
                    interface_name: interface.name.as_ref(),
                    iw: &iw,
                    now_ms,
                });
            let scalar_observations = NetworkScalarObservations {
                total_rx_bytes: counter.total_rx_bytes,
                total_tx_bytes: counter.total_tx_bytes,
                rx_bytes_per_sec: counter.rx_rate,
                tx_bytes_per_sec: counter.tx_rate,
                utilization_pct,
                link_speed_mbps,
                link_up: interface.link_up,
            };
            let wireless_observations = assemble_wireless_observations(
                is_wireless,
                &interface.name,
                &mut wireless,
                &mut iw,
                now_ms,
            );
            let mut metric = NetworkMetrics::new(interface.name.clone());
            metric.device_id = interface.stable_id.clone();
            metric.device_state = DeviceState::healthy(now_ms);
            metric.ipv4_addr = address
                .as_ref()
                .and_then(|(ipv4, _)| ipv4.clone().map(Arc::from));
            metric.ipv6_addr = address
                .as_ref()
                .and_then(|(_, ipv6)| ipv6.clone().map(Arc::from));
            metric.mac_addr = interface.mac_addr.clone();
            metric.driver = interface.driver.clone();
            metric.adapter = interface.adapter.clone();
            metric.apply_observations(adapter_type, scalar_observations, wireless_observations);
            metric
        })
        .collect::<Vec<_>>();

    let discovery = match inventory.outcome {
        SourceOutcome::Available => DeviceDiscovery::Available(inventory.discovered_devices),
        SourceOutcome::Empty => DeviceDiscovery::Empty,
        SourceOutcome::Partial(failure) => DeviceDiscovery::Partial {
            discovered_devices: inventory.discovered_devices,
            failure,
        },
        SourceOutcome::Unavailable(failure) => DeviceDiscovery::Unavailable(failure),
    };
    let enrichments = vec![
        source_status(
            SYSFS_METADATA_PROVIDER,
            inventory.metadata_outcome,
            inventory.metadata_item_count,
        ),
        source_status(
            SYSINFO_COUNTER_PROVIDER,
            counters.outcome,
            counters.current_count,
        ),
        source_status(
            ADDRESS_PROVIDER,
            addresses.outcome,
            addresses
                .value
                .values()
                .filter(|address| address.ipv4.is_some() || address.ipv6.is_some())
                .count(),
        ),
        source_status(
            PROC_WIRELESS_PROVIDER,
            wireless.outcome,
            wireless.value.len(),
        ),
        source_status(
            IW_PROVIDER,
            iw.outcome,
            iw.value
                .values()
                .filter(|result| {
                    matches!(
                        result,
                        IwLinkResult::Associated { .. } | IwLinkResult::NotAssociated
                    )
                })
                .count(),
        ),
    ];
    DeviceSourceSnapshot::from_discovery(
        metrics,
        ProviderId::borrowed(SYSFS_INVENTORY_PROVIDER),
        discovery,
        enrichments,
    )
}

/// Backfill `link_speed_mbps` (and recompute utilization) from `iw` tx-bitrate
/// when sysfs does not expose a link speed.
///
/// `/sys/class/net/<wifi>/speed` is absent or reads 0 on most mac80211
/// drivers, so the counter path (`observe_counter_samples`, which runs BEFORE
/// `iw` is fetched) computes utilization against a None link speed and yields
/// `Unavailable`. Here, where the `iw` result is already in hand, we restore
/// both the displayed link speed and a real utilization denominator from the
/// tx bitrate. The sysfs typed state (Unavailable / Stale) is preserved
/// verbatim when no iw bitrate is available — None is never fabricated as 0.
///
/// The utilization ratio itself flows through the single shared
/// `counters::link_utilization_pct` (the same function the counter path uses),
/// so there is no second formula to keep in sync.
struct WirelessLinkBackfill<'a> {
    is_wireless: bool,
    sysfs_link_speed: ScalarObservation<u64>,
    counter_utilization: ScalarObservation<f32>,
    rx_rate: ScalarObservation<u64>,
    tx_rate: ScalarObservation<u64>,
    interface_name: &'a str,
    iw: &'a SourceObservation<HashMap<Arc<str>, IwLinkResult>>,
    now_ms: u64,
}

fn backfill_wireless_link_speed(
    input: WirelessLinkBackfill<'_>,
) -> (ScalarObservation<u64>, ScalarObservation<f32>) {
    let WirelessLinkBackfill {
        is_wireless,
        sysfs_link_speed,
        counter_utilization,
        rx_rate,
        tx_rate,
        interface_name,
        iw,
        now_ms,
    } = input;
    if !is_wireless || sysfs_link_speed.current_value().is_some() {
        return (sysfs_link_speed, counter_utilization);
    }
    let Some(IwLinkResult::Associated {
        tx_bitrate_mbps: Some(bitrate),
        ..
    }) = iw.value.get(interface_name)
    else {
        return (sysfs_link_speed, counter_utilization);
    };
    let link_speed_mbps = ScalarObservation::available(*bitrate, now_ms);
    let utilization = recompute_utilization(
        counter_utilization,
        rx_rate,
        tx_rate,
        link_speed_mbps,
        now_ms,
    );
    (link_speed_mbps, utilization)
}

/// Recompute link utilization now that a backfilled link speed is available.
/// The ratio itself delegates to the shared `counters::link_utilization_pct`
/// (the same function the counter path uses); the partial-failure branch in
/// `counters::observe_utilization` cannot occur here because the backfilled
/// `link_speed_mbps` is always fully `Available`.
fn recompute_utilization(
    previous: ScalarObservation<f32>,
    rx_rate: ScalarObservation<u64>,
    tx_rate: ScalarObservation<u64>,
    link_speed: ScalarObservation<u64>,
    now_ms: u64,
) -> ScalarObservation<f32> {
    let (Some(rx_bytes_per_sec), Some(tx_bytes_per_sec)) = (
        rx_rate.current_value().copied(),
        tx_rate.current_value().copied(),
    ) else {
        let failure = rx_rate
            .availability()
            .failure()
            .or(tx_rate.availability().failure())
            .unwrap_or(FailureKind::TemporarilyUnavailable);
        return ScalarObservation::unavailable(failure).retain_previous(previous);
    };
    let Some(link_speed_mbps) = link_speed.current_value().copied() else {
        let failure = link_speed
            .availability()
            .failure()
            .unwrap_or(FailureKind::Unsupported);
        return ScalarObservation::unavailable(failure).retain_previous(previous);
    };
    let Some(value) = link_utilization_pct(rx_bytes_per_sec, tx_bytes_per_sec, link_speed_mbps)
    else {
        // Non-positive capacity (a zero tx bitrate) leaves the ratio
        // undefined — a typed gap, never a NaN or a clamped fake 100%.
        return ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
            .retain_previous(previous);
    };
    ScalarObservation::available(value, now_ms)
}

fn assemble_wireless_observations(
    is_wireless: bool,
    interface_name: &str,
    wireless: &mut SourceObservation<HashMap<String, i32>>,
    iw: &mut SourceObservation<HashMap<Arc<str>, IwLinkResult>>,
    now_ms: u64,
) -> NetworkWirelessObservations {
    if !is_wireless {
        return NetworkWirelessObservations::not_applicable(now_ms);
    }

    let (
        association,
        ssid,
        iw_signal_dbm,
        bssid,
        frequency_mhz,
        channel,
        rx_bitrate_mbps,
        tx_bitrate_mbps,
        protocol,
    ) = match iw.value.remove(interface_name) {
        Some(IwLinkResult::Associated {
            bssid,
            ssid,
            signal_dbm,
            frequency_mhz,
            channel,
            rx_bitrate_mbps,
            tx_bitrate_mbps,
            protocol,
        }) => (
            OptionalObservation::present(true, now_ms),
            OptionalObservation::present(Arc::from(ssid), now_ms),
            signal_dbm,
            present_or_unavailable(bssid.map(Arc::from), now_ms),
            present_or_unavailable(frequency_mhz, now_ms),
            present_or_unavailable(channel, now_ms),
            present_or_unavailable(rx_bitrate_mbps, now_ms),
            present_or_unavailable(tx_bitrate_mbps, now_ms),
            present_or_unavailable(protocol.map(Arc::from), now_ms),
        ),
        Some(IwLinkResult::NotAssociated) => (
            OptionalObservation::absent(now_ms),
            OptionalObservation::absent(now_ms),
            None,
            OptionalObservation::absent(now_ms),
            OptionalObservation::absent(now_ms),
            OptionalObservation::absent(now_ms),
            OptionalObservation::absent(now_ms),
            OptionalObservation::absent(now_ms),
            OptionalObservation::absent(now_ms),
        ),
        Some(IwLinkResult::Failed(failure)) => (
            OptionalObservation::unavailable(failure),
            OptionalObservation::unavailable(failure),
            None,
            OptionalObservation::unavailable(failure),
            OptionalObservation::unavailable(failure),
            OptionalObservation::unavailable(failure),
            OptionalObservation::unavailable(failure),
            OptionalObservation::unavailable(failure),
            OptionalObservation::unavailable(failure),
        ),
        None => {
            let failure = source_failure(iw.outcome).unwrap_or(FailureKind::TemporarilyUnavailable);
            (
                OptionalObservation::unavailable(failure),
                OptionalObservation::unavailable(failure),
                None,
                OptionalObservation::unavailable(failure),
                OptionalObservation::unavailable(failure),
                OptionalObservation::unavailable(failure),
                OptionalObservation::unavailable(failure),
                OptionalObservation::unavailable(failure),
                OptionalObservation::unavailable(failure),
            )
        }
    };
    let signal_dbm = if association.is_current_absent() {
        OptionalObservation::absent(now_ms)
    } else if let Some(signal) = wireless.value.remove(interface_name) {
        OptionalObservation::present(signal, now_ms)
    } else if let Some(dbm) = iw_signal_dbm {
        // `/proc/net/wireless` is empty on most modern mac80211 drivers, so
        // fall back to the signal `iw dev <iface> link` already reported —
        // without this the WiFi signal renders "—" on current kernels.
        OptionalObservation::present(dbm, now_ms)
    } else {
        OptionalObservation::unavailable(
            source_failure(wireless.outcome).unwrap_or(FailureKind::TemporarilyUnavailable),
        )
    };
    NetworkWirelessObservations {
        association,
        ssid,
        signal_dbm,
        bssid,
        frequency_mhz,
        channel,
        rx_bitrate_mbps,
        tx_bitrate_mbps,
        protocol,
    }
}

fn present_or_unavailable<T>(value: Option<T>, observed_at_ms: u64) -> OptionalObservation<T> {
    value.map_or_else(
        || OptionalObservation::unavailable(FailureKind::Unsupported),
        |value| OptionalObservation::present(value, observed_at_ms),
    )
}

fn source_status(
    provider: &'static str,
    outcome: SourceOutcome,
    item_count: usize,
) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed(provider),
        outcome,
        item_count,
    }
}

const fn source_failure(outcome: SourceOutcome) -> Option<FailureKind> {
    match outcome {
        SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => Some(failure),
        SourceOutcome::Available | SourceOutcome::Empty => None,
    }
}

fn classify_adapter_type(name: &str, arp_type: Option<u64>) -> NetworkAdapterType {
    if name == "lo" || arp_type == Some(772) {
        NetworkAdapterType::Loopback
    } else if is_vpn_interface(name) {
        NetworkAdapterType::Vpn
    } else if is_virtual_interface(name) {
        NetworkAdapterType::Virtual
    } else if matches!(arp_type, Some(801..=804)) || name.starts_with('w') {
        NetworkAdapterType::WiFi
    } else if name.starts_with("en") || name.starts_with("eth") {
        NetworkAdapterType::Ethernet
    } else {
        NetworkAdapterType::Other
    }
}

/// Best-effort Linux name classification for tunnel/VPN adapters. `/sys` and
/// rtnetlink expose the interface but do not provide one portable "VPN"
/// boolean, so only well-known tunnel families are promoted to the dedicated
/// Mission Center category; unknown software interfaces remain `Other`.
fn is_vpn_interface(name: &str) -> bool {
    name.starts_with("tun")
        || name.starts_with("tap")
        || name.starts_with("wg")
        || name.starts_with("ppp")
        || name.starts_with("ipsec")
        || name.starts_with("tailscale")
}

#[cfg(test)]
#[path = "../../../tests/headless/engine/collector/network.rs"]
mod tests;
