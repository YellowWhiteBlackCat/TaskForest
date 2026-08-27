//! Private serde compatibility boundary for network adapter rows.

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{
    NetworkAdapterType, NetworkMetrics, NetworkScalarObservations, NetworkWirelessObservations,
};
use crate::core::device_state::DeviceState;
use crate::core::{DeviceGeneration, OptionalObservation, ScalarAvailability, ScalarObservation};

const LEGACY_NETWORK_OBSERVED_AT_MS: u64 = 0;

#[derive(Serialize, Deserialize)]
struct NetworkMetricsWire {
    #[serde(default)]
    device_id: Arc<str>,
    #[serde(default)]
    device_generation: DeviceGeneration,
    #[serde(default)]
    device_state: DeviceState,
    #[serde(default)]
    interface_name: Arc<str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rx_bytes_per_sec: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tx_bytes_per_sec: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total_rx_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total_tx_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ipv4_addr: Option<Arc<str>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ipv6_addr: Option<Arc<str>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mac_addr: Option<Arc<str>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    is_wireless: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    link_speed_mbps: Option<u64>,
    /// Presence matters: explicit `Other` is typed truth and must beat a
    /// conflicting legacy `is_wireless=true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    adapter_type: Option<NetworkAdapterType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    utilization_pct: Option<f32>,
    #[serde(default)]
    scalar_observations: NetworkScalarObservations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signal_dbm: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ssid: Option<Arc<str>>,
    #[serde(default)]
    wireless_observations: NetworkWirelessObservations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    driver: Option<Arc<str>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    adapter: Option<Arc<str>>,
}

impl Serialize for NetworkMetrics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let adapter_type =
            (self.adapter_type != NetworkAdapterType::Unknown).then_some(self.adapter_type);
        NetworkMetricsWire {
            device_id: self.device_id.clone(),
            device_generation: self.device_generation,
            device_state: self.device_state,
            interface_name: self.interface_name.clone(),
            rx_bytes_per_sec: self.current_rx_bytes_per_sec(),
            tx_bytes_per_sec: self.current_tx_bytes_per_sec(),
            total_rx_bytes: self.current_total_rx_bytes(),
            total_tx_bytes: self.current_total_tx_bytes(),
            ipv4_addr: self.ipv4_addr.clone(),
            ipv6_addr: self.ipv6_addr.clone(),
            mac_addr: self.mac_addr.clone(),
            is_wireless: adapter_type.map(|kind| kind == NetworkAdapterType::WiFi),
            link_speed_mbps: self.current_link_speed_mbps(),
            adapter_type,
            utilization_pct: self.current_utilization_pct(),
            scalar_observations: self.scalar_observations,
            signal_dbm: self.current_signal_dbm(),
            ssid: self.wireless_observations.ssid.current_value().cloned(),
            wireless_observations: self.wireless_observations.clone(),
            driver: self.driver.clone(),
            adapter: self.adapter.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for NetworkMetrics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = NetworkMetricsWire::deserialize(deserializer)?;
        let trusted_identity = trustworthy_network_identity(&wire);
        let mut adapter_type = wire.adapter_type.unwrap_or(NetworkAdapterType::Unknown);
        if trusted_identity && wire.adapter_type.is_none() && wire.is_wireless == Some(true) {
            adapter_type = NetworkAdapterType::WiFi;
        }

        let mut scalar_observations = wire.scalar_observations;
        if trusted_identity {
            hydrate_unknown(&mut scalar_observations.total_rx_bytes, wire.total_rx_bytes);
            hydrate_unknown(&mut scalar_observations.total_tx_bytes, wire.total_tx_bytes);
            hydrate_unknown(
                &mut scalar_observations.rx_bytes_per_sec,
                wire.rx_bytes_per_sec,
            );
            hydrate_unknown(
                &mut scalar_observations.tx_bytes_per_sec,
                wire.tx_bytes_per_sec,
            );
            hydrate_finite_unknown(
                &mut scalar_observations.utilization_pct,
                wire.utilization_pct,
            );
            hydrate_unknown(
                &mut scalar_observations.link_speed_mbps,
                wire.link_speed_mbps,
            );
        }

        let mut wireless_observations = wire.wireless_observations;
        if trusted_identity && adapter_type == NetworkAdapterType::WiFi {
            hydrate_optional_unknown(&mut wireless_observations.signal_dbm, wire.signal_dbm);
            let legacy_ssid = wire.ssid.filter(|ssid| !ssid.trim().is_empty());
            hydrate_optional_unknown(&mut wireless_observations.ssid, legacy_ssid.clone());
            if legacy_ssid.is_some()
                && wireless_observations.association.availability() == ScalarAvailability::Unknown
            {
                wireless_observations.association =
                    OptionalObservation::present(true, LEGACY_NETWORK_OBSERVED_AT_MS);
            }
        }

        Ok(Self {
            device_id: wire.device_id,
            device_generation: wire.device_generation,
            device_state: wire.device_state,
            interface_name: wire.interface_name,
            ipv4_addr: wire.ipv4_addr,
            ipv6_addr: wire.ipv6_addr,
            mac_addr: wire.mac_addr,
            adapter_type,
            scalar_observations,
            wireless_observations,
            driver: wire.driver,
            adapter: wire.adapter,
        })
    }
}

fn hydrate_unknown<T>(observation: &mut ScalarObservation<T>, value: Option<T>) {
    if observation.availability() == ScalarAvailability::Unknown
        && let Some(value) = value
    {
        *observation = ScalarObservation::available(value, LEGACY_NETWORK_OBSERVED_AT_MS);
    }
}

fn hydrate_finite_unknown(observation: &mut ScalarObservation<f32>, value: Option<f32>) {
    hydrate_unknown(observation, value.filter(|value| value.is_finite()));
}

fn hydrate_optional_unknown<T>(observation: &mut OptionalObservation<T>, value: Option<T>) {
    if observation.availability() == ScalarAvailability::Unknown
        && let Some(value) = value
    {
        *observation = OptionalObservation::present(value, LEGACY_NETWORK_OBSERVED_AT_MS);
    }
}

fn trustworthy_network_identity(wire: &NetworkMetricsWire) -> bool {
    !wire.device_id.trim().is_empty() || !wire.interface_name.trim().is_empty()
}
