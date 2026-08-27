//! Network telemetry metrics: interface identity and addresses, rx/tx byte
//! and rate scalar observations with availability, link speed and carrier
//! state, typed adapter classification, and wireless association/SSID/signal/
//! link-detail observations with a private schema-v1 compatibility boundary.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{OptionalObservation, OptionalObservationState, ScalarAvailability, ScalarObservation};
use crate::core::device_state::DeviceState;
use crate::core::{DeviceGeneration, FailureKind};

mod wire;

/// Coarse, platform-neutral network-adapter classification.
///
/// Native adapters map their own interface types into this vocabulary. The
/// Legacy `is_wireless` is handled only by the private serde boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NetworkAdapterType {
    /// The provider or older payload supplied no trustworthy classification.
    #[default]
    Unknown,
    /// Wired Ethernet or an equivalent native adapter.
    Ethernet,
    /// Wireless LAN.
    WiFi,
    /// A VPN/tunnel adapter (for example `tun`, `tap`, `wg`, or `ppp`).
    Vpn,
    /// Virtual, bridge, or software-defined interface.
    Virtual,
    /// Host loopback interface.
    Loopback,
    /// A provider-recognized interface outside the shared baseline taxonomy.
    Other,
}

/// Authoritative typed truth for independently fallible network scalars.
///
/// Schema-v1 numerics are confined to the private serde DTO.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct NetworkScalarObservations {
    pub total_rx_bytes: ScalarObservation<u64>,
    pub total_tx_bytes: ScalarObservation<u64>,
    pub rx_bytes_per_sec: ScalarObservation<u64>,
    pub tx_bytes_per_sec: ScalarObservation<u64>,
    pub utilization_pct: ScalarObservation<f32>,
    pub link_speed_mbps: ScalarObservation<u64>,
    /// Current native link/carrier state. A confirmed down link is
    /// `Available(false)`, not a provider failure.
    pub link_up: ScalarObservation<bool>,
}

impl NetworkScalarObservations {
    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        Self {
            total_rx_bytes: self.total_rx_bytes.retain_previous(previous.total_rx_bytes),
            total_tx_bytes: self.total_tx_bytes.retain_previous(previous.total_tx_bytes),
            rx_bytes_per_sec: self
                .rx_bytes_per_sec
                .retain_previous(previous.rx_bytes_per_sec),
            tx_bytes_per_sec: self
                .tx_bytes_per_sec
                .retain_previous(previous.tx_bytes_per_sec),
            utilization_pct: self
                .utilization_pct
                .retain_previous(previous.utilization_pct),
            link_speed_mbps: self
                .link_speed_mbps
                .retain_previous(previous.link_speed_mbps),
            link_up: self.link_up.retain_previous(previous.link_up),
        }
    }

    #[must_use]
    pub fn unavailable(failure: FailureKind) -> Self {
        Self {
            total_rx_bytes: ScalarObservation::unavailable(failure),
            total_tx_bytes: ScalarObservation::unavailable(failure),
            rx_bytes_per_sec: ScalarObservation::unavailable(failure),
            tx_bytes_per_sec: ScalarObservation::unavailable(failure),
            utilization_pct: ScalarObservation::unavailable(failure),
            link_speed_mbps: ScalarObservation::unavailable(failure),
            link_up: ScalarObservation::unavailable(failure),
        }
    }
}

/// Wireless-only fields whose semantic state is independent of freshness.
///
/// `association` uses `Present(true)` for a confirmed association, `Absent`
/// for a confirmed unassociated interface, and `NotApplicable` for a wired
/// adapter. `Present(false)` is not emitted by native providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NetworkWirelessObservations {
    pub association: OptionalObservation<bool>,
    pub ssid: OptionalObservation<Arc<str>>,
    pub signal_dbm: OptionalObservation<i32>,
    /// Access-point BSSID when the adapter is associated.
    #[serde(default)]
    pub bssid: OptionalObservation<Arc<str>>,
    /// Negotiated center frequency, in MHz, when exposed by the provider.
    #[serde(default)]
    pub frequency_mhz: OptionalObservation<u32>,
    /// Channel derived from an observed center frequency when unambiguous.
    #[serde(default)]
    pub channel: OptionalObservation<u32>,
    /// Negotiated receive/transmit bitrates, rounded up to Mbps.
    #[serde(default)]
    pub rx_bitrate_mbps: OptionalObservation<u64>,
    #[serde(default)]
    pub tx_bitrate_mbps: OptionalObservation<u64>,
    /// Best-effort negotiated 802.11 mode, for example `802.11be (Wi-Fi 7)`.
    #[serde(default)]
    pub protocol: OptionalObservation<Arc<str>>,
}

impl NetworkWirelessObservations {
    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        Self {
            association: self.association.retain_previous(previous.association),
            ssid: self.ssid.retain_previous(previous.ssid),
            signal_dbm: self.signal_dbm.retain_previous(previous.signal_dbm),
            bssid: self.bssid.retain_previous(previous.bssid),
            frequency_mhz: self.frequency_mhz.retain_previous(previous.frequency_mhz),
            channel: self.channel.retain_previous(previous.channel),
            rx_bitrate_mbps: self
                .rx_bitrate_mbps
                .retain_previous(previous.rx_bitrate_mbps),
            tx_bitrate_mbps: self
                .tx_bitrate_mbps
                .retain_previous(previous.tx_bitrate_mbps),
            protocol: self.protocol.retain_previous(previous.protocol),
        }
    }

    #[must_use]
    pub fn unavailable(failure: FailureKind) -> Self {
        Self {
            association: OptionalObservation::unavailable(failure),
            ssid: OptionalObservation::unavailable(failure),
            signal_dbm: OptionalObservation::unavailable(failure),
            bssid: OptionalObservation::unavailable(failure),
            frequency_mhz: OptionalObservation::unavailable(failure),
            channel: OptionalObservation::unavailable(failure),
            rx_bitrate_mbps: OptionalObservation::unavailable(failure),
            tx_bitrate_mbps: OptionalObservation::unavailable(failure),
            protocol: OptionalObservation::unavailable(failure),
        }
    }

    #[must_use]
    pub fn not_applicable(observed_at_ms: u64) -> Self {
        Self {
            association: OptionalObservation::not_applicable(observed_at_ms),
            ssid: OptionalObservation::not_applicable(observed_at_ms),
            signal_dbm: OptionalObservation::not_applicable(observed_at_ms),
            bssid: OptionalObservation::not_applicable(observed_at_ms),
            frequency_mhz: OptionalObservation::not_applicable(observed_at_ms),
            channel: OptionalObservation::not_applicable(observed_at_ms),
            rx_bitrate_mbps: OptionalObservation::not_applicable(observed_at_ms),
            tx_bitrate_mbps: OptionalObservation::not_applicable(observed_at_ms),
            protocol: OptionalObservation::not_applicable(observed_at_ms),
        }
    }
}

/// Enhanced Network Metrics
#[derive(Debug, Clone, Default)]
pub struct NetworkMetrics {
    /// Stable identity (MAC preferred, interface name fallback).
    pub device_id: Arc<str>,
    /// Confirmed hot-plug generation for this stable identity. Zero means the
    /// metric has not yet passed through a lifecycle assembler.
    pub device_generation: DeviceGeneration,
    pub device_state: DeviceState,
    pub interface_name: Arc<str>,
    pub ipv4_addr: Option<Arc<str>>,
    pub ipv6_addr: Option<Arc<str>>,
    pub mac_addr: Option<Arc<str>>,
    /// Canonical native adapter classification.
    adapter_type: NetworkAdapterType,
    /// Authoritative total, rate, capacity, carrier, and utilization truth.
    scalar_observations: NetworkScalarObservations,
    /// Authoritative association, SSID, and signal truth. This keeps confirmed
    /// unassociation and wired non-applicability distinct from provider
    /// failures and from old snapshots that contain no typed state.
    wireless_observations: NetworkWirelessObservations,
    /// Native driver or implementation name, when exposed.
    pub driver: Option<Arc<str>>,
    /// Human-readable adapter model or native hardware identifier.
    pub adapter: Option<Arc<str>>,
}

impl NetworkMetrics {
    /// Construct a discovered adapter. Classification and observations enter
    /// through [`Self::apply_observations`].
    #[must_use]
    pub fn new(interface_name: impl Into<Arc<str>>) -> Self {
        Self {
            interface_name: interface_name.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn current_total_rx_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .total_rx_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_total_tx_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .total_tx_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_rx_bytes_per_sec(&self) -> Option<u64> {
        self.scalar_observations
            .rx_bytes_per_sec
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_tx_bytes_per_sec(&self) -> Option<u64> {
        self.scalar_observations
            .tx_bytes_per_sec
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_utilization_pct(&self) -> Option<f32> {
        self.scalar_observations
            .utilization_pct
            .current_value()
            .copied()
            .filter(|value| value.is_finite())
    }

    #[must_use]
    pub fn current_link_speed_mbps(&self) -> Option<u64> {
        self.scalar_observations
            .link_speed_mbps
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_link_up(&self) -> Option<bool> {
        match self.scalar_observations.link_up.availability() {
            ScalarAvailability::Available | ScalarAvailability::Partial(_) => {
                self.scalar_observations.link_up.current_value().copied()
            }
            ScalarAvailability::Unknown
            | ScalarAvailability::Stale(_)
            | ScalarAvailability::Unavailable(_) => None,
        }
    }

    #[must_use]
    pub fn current_signal_dbm(&self) -> Option<i32> {
        self.wireless_observations
            .signal_dbm
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_ssid(&self) -> Option<&str> {
        self.wireless_observations
            .ssid
            .current_value()
            .map(|ssid| ssid.as_ref())
    }

    #[must_use]
    pub fn current_is_associated(&self) -> Option<bool> {
        let observation = &self.wireless_observations.association;
        if observation.availability().is_current() {
            return match observation.last_known_state() {
                OptionalObservationState::Present(value) => Some(*value),
                OptionalObservationState::Absent => Some(false),
                OptionalObservationState::Unknown | OptionalObservationState::NotApplicable => None,
            };
        }
        None
    }

    #[must_use]
    pub fn current_bssid(&self) -> Option<&str> {
        self.wireless_observations
            .bssid
            .current_value()
            .map(Arc::as_ref)
    }

    #[must_use]
    pub fn current_frequency_mhz(&self) -> Option<u32> {
        self.wireless_observations
            .frequency_mhz
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_channel(&self) -> Option<u32> {
        self.wireless_observations.channel.current_value().copied()
    }

    #[must_use]
    pub fn current_rx_bitrate_mbps(&self) -> Option<u64> {
        self.wireless_observations
            .rx_bitrate_mbps
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_tx_bitrate_mbps(&self) -> Option<u64> {
        self.wireless_observations
            .tx_bitrate_mbps
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_protocol(&self) -> Option<&str> {
        self.wireless_observations
            .protocol
            .current_value()
            .map(Arc::as_ref)
    }

    #[must_use]
    pub const fn adapter_type(&self) -> NetworkAdapterType {
        self.adapter_type
    }

    #[must_use]
    pub const fn scalar_observations(&self) -> &NetworkScalarObservations {
        &self.scalar_observations
    }

    #[must_use]
    pub const fn wireless_observations(&self) -> &NetworkWirelessObservations {
        &self.wireless_observations
    }

    /// Replace classification, scalar and wireless truth as one adapter row
    /// assembly. Compatibility mirrors are projected only by serde.
    pub fn apply_observations(
        &mut self,
        adapter_type: NetworkAdapterType,
        scalar_observations: NetworkScalarObservations,
        wireless_observations: NetworkWirelessObservations,
    ) {
        self.adapter_type = adapter_type;
        self.scalar_observations = scalar_observations;
        self.wireless_observations = wireless_observations;
    }
}
