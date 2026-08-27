//! Process connection telemetry: provider-neutral transport, address family,
//! endpoint, and connection-state model, the provider correlation key, and the
//! per-process network snapshot with traffic rates and provenance.

use std::borrow::Cow;
use std::fmt;
use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6};

use serde::de::Deserializer;
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;
use crate::core::{FailureKind, ProviderId};

/// Transport semantics, kept independent from endpoint address family.
///
/// Unknown future transports are retained verbatim instead of being collapsed
/// into a generic sentinel or forcing a platform adapter to pretend they are
/// TCP/UDP.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConnectionTransport {
    Tcp,
    Udp,
    Sctp,
    Local,
    Other(String),
}

impl ConnectionTransport {
    fn from_wire(value: String) -> Self {
        match value.as_str() {
            "tcp" | "tcp6" => Self::Tcp,
            "udp" | "udp6" => Self::Udp,
            "sctp" => Self::Sctp,
            "local" | "unix" => Self::Local,
            _ => Self::Other(value),
        }
    }

    fn wire_name(&self) -> &str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
            Self::Sctp => "sctp",
            Self::Local => "local",
            Self::Other(value) => value,
        }
    }
}

impl fmt::Display for ConnectionTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tcp => formatter.write_str("TCP"),
            Self::Udp => formatter.write_str("UDP"),
            Self::Sctp => formatter.write_str("SCTP"),
            Self::Local => formatter.write_str("LOCAL"),
            Self::Other(value) => formatter.write_str(value),
        }
    }
}

impl Serialize for ConnectionTransport {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.wire_name())
    }
}

impl<'de> Deserialize<'de> for ConnectionTransport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from_wire)
    }
}

/// Address family of both connection endpoints.
///
/// `Unspecified` means the provider cannot classify the native endpoint; it is
/// not an IPv4 wildcard. `Other` preserves future platform families verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum ConnectionAddressFamily {
    Ipv4,
    Ipv6,
    Local,
    #[default]
    Unspecified,
    Other(String),
}

impl ConnectionAddressFamily {
    fn from_wire(value: String) -> Self {
        match value.as_str() {
            "ipv4" => Self::Ipv4,
            "ipv6" => Self::Ipv6,
            "local" | "unix" => Self::Local,
            "unspecified" => Self::Unspecified,
            _ => Self::Other(value),
        }
    }

    fn wire_name(&self) -> &str {
        match self {
            Self::Ipv4 => "ipv4",
            Self::Ipv6 => "ipv6",
            Self::Local => "local",
            Self::Unspecified => "unspecified",
            Self::Other(value) => value,
        }
    }
}

impl Serialize for ConnectionAddressFamily {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.wire_name())
    }
}

impl<'de> Deserialize<'de> for ConnectionAddressFamily {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from_wire)
    }
}

/// One provider-neutral connection endpoint.
///
/// IP endpoints retain the legacy JSON socket string. Structured variants let
/// native providers report local paths or opaque endpoints without inventing a
/// `0.0.0.0:0` address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum ConnectionEndpoint {
    Ip(SocketAddr),
    Local {
        path: String,
    },
    Opaque {
        value: String,
    },
    #[default]
    Unspecified,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StructuredConnectionEndpoint {
    Local { path: String },
    Opaque { value: String },
    Unspecified,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ConnectionEndpointWire {
    Ip(SocketAddr),
    Structured(StructuredConnectionEndpoint),
}

impl ConnectionEndpoint {
    #[must_use]
    pub fn local(path: impl Into<String>) -> Self {
        Self::Local { path: path.into() }
    }

    #[must_use]
    pub fn opaque(value: impl Into<String>) -> Self {
        Self::Opaque {
            value: value.into(),
        }
    }

    #[must_use]
    pub const fn as_socket_addr(&self) -> Option<SocketAddr> {
        match self {
            Self::Ip(address) => Some(*address),
            Self::Local { .. } | Self::Opaque { .. } | Self::Unspecified => None,
        }
    }

    fn family_hint(&self) -> Option<ConnectionAddressFamily> {
        match self {
            Self::Ip(SocketAddr::V4(_)) => Some(ConnectionAddressFamily::Ipv4),
            Self::Ip(SocketAddr::V6(_)) => Some(ConnectionAddressFamily::Ipv6),
            Self::Local { .. } => Some(ConnectionAddressFamily::Local),
            Self::Opaque { .. } | Self::Unspecified => None,
        }
    }
}

impl From<SocketAddr> for ConnectionEndpoint {
    fn from(address: SocketAddr) -> Self {
        Self::Ip(address)
    }
}

impl From<SocketAddrV4> for ConnectionEndpoint {
    fn from(address: SocketAddrV4) -> Self {
        Self::Ip(SocketAddr::V4(address))
    }
}

impl From<SocketAddrV6> for ConnectionEndpoint {
    fn from(address: SocketAddrV6) -> Self {
        Self::Ip(SocketAddr::V6(address))
    }
}

impl fmt::Display for ConnectionEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ip(address) => address.fmt(formatter),
            Self::Local { path } => formatter.write_str(path),
            Self::Opaque { value } => formatter.write_str(value),
            Self::Unspecified => formatter.write_str("—"),
        }
    }
}

impl Serialize for ConnectionEndpoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Ip(address) => address.serialize(serializer),
            Self::Local { path } => {
                StructuredConnectionEndpoint::Local { path: path.clone() }.serialize(serializer)
            }
            Self::Opaque { value } => StructuredConnectionEndpoint::Opaque {
                value: value.clone(),
            }
            .serialize(serializer),
            Self::Unspecified => StructuredConnectionEndpoint::Unspecified.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ConnectionEndpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match ConnectionEndpointWire::deserialize(deserializer)? {
            ConnectionEndpointWire::Ip(address) => Ok(Self::Ip(address)),
            ConnectionEndpointWire::Structured(StructuredConnectionEndpoint::Local { path }) => {
                Ok(Self::Local { path })
            }
            ConnectionEndpointWire::Structured(StructuredConnectionEndpoint::Opaque { value }) => {
                Ok(Self::Opaque { value })
            }
            ConnectionEndpointWire::Structured(StructuredConnectionEndpoint::Unspecified) => {
                Ok(Self::Unspecified)
            }
        }
    }
}

/// Provider-private token used to correlate a discovered connection with a
/// native ownership table.
///
/// This is deliberately optional on [`ProcessConnection`] and is never a
/// cross-platform connection identity. Numeric tokens preserve the legacy wire
/// shape; text and composite forms keep future adapters from manufacturing an
/// integer key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConnectionProviderKey {
    Numeric(u64),
    Opaque(String),
    Composite(Vec<String>),
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum ConnectionProviderKeyWire {
    Numeric(u64),
    Opaque(String),
    Composite { parts: Vec<String> },
}

impl ConnectionProviderKey {
    #[must_use]
    pub const fn as_numeric(&self) -> Option<u64> {
        match self {
            Self::Numeric(value) => Some(*value),
            Self::Opaque(_) | Self::Composite(_) => None,
        }
    }
}

impl From<u64> for ConnectionProviderKey {
    fn from(value: u64) -> Self {
        Self::Numeric(value)
    }
}

impl From<String> for ConnectionProviderKey {
    fn from(value: String) -> Self {
        Self::Opaque(value)
    }
}

impl From<&str> for ConnectionProviderKey {
    fn from(value: &str) -> Self {
        Self::Opaque(value.to_string())
    }
}

impl fmt::Display for ConnectionProviderKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Numeric(value) => value.fmt(formatter),
            Self::Opaque(value) => formatter.write_str(value),
            Self::Composite(parts) => formatter.write_str(&parts.join(":")),
        }
    }
}

impl Serialize for ConnectionProviderKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Numeric(value) => value.serialize(serializer),
            Self::Opaque(value) => value.serialize(serializer),
            Self::Composite(parts) => ConnectionProviderKeyWire::Composite {
                parts: parts.clone(),
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ConnectionProviderKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match ConnectionProviderKeyWire::deserialize(deserializer)? {
            ConnectionProviderKeyWire::Numeric(value) => Ok(Self::Numeric(value)),
            ConnectionProviderKeyWire::Opaque(value) => Ok(Self::Opaque(value)),
            ConnectionProviderKeyWire::Composite { parts } => Ok(Self::Composite(parts)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Established,
    SynSent,
    SynReceived,
    FinWait1,
    FinWait2,
    TimeWait,
    Closed,
    CloseWait,
    LastAck,
    Listen,
    Closing,
    Unconnected,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessConnection {
    pub transport: ConnectionTransport,
    pub family: ConnectionAddressFamily,
    pub local: ConnectionEndpoint,
    pub remote: ConnectionEndpoint,
    pub state: ConnectionState,
    pub provider_key: Option<ConnectionProviderKey>,
}

#[derive(Deserialize)]
struct ProcessConnectionWire {
    #[serde(alias = "transport")]
    protocol: String,
    #[serde(default)]
    family: Option<ConnectionAddressFamily>,
    local: ConnectionEndpoint,
    remote: ConnectionEndpoint,
    state: ConnectionState,
    #[serde(default)]
    provider_key: Option<ConnectionProviderKey>,
}

impl ProcessConnection {
    fn legacy_protocol_name(&self) -> Cow<'_, str> {
        match (&self.transport, &self.family) {
            (ConnectionTransport::Tcp, ConnectionAddressFamily::Ipv6) => Cow::Borrowed("tcp6"),
            (ConnectionTransport::Udp, ConnectionAddressFamily::Ipv6) => Cow::Borrowed("udp6"),
            _ => Cow::Borrowed(self.transport.wire_name()),
        }
    }

    fn inferred_family(
        local: &ConnectionEndpoint,
        remote: &ConnectionEndpoint,
    ) -> Option<ConnectionAddressFamily> {
        match (local.family_hint(), remote.family_hint()) {
            (Some(local), Some(remote)) if local == remote => Some(local),
            (Some(family), None) | (None, Some(family)) => Some(family),
            _ => None,
        }
    }
}

impl Serialize for ProcessConnection {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let include_family = !matches!(
            self.family,
            ConnectionAddressFamily::Ipv4 | ConnectionAddressFamily::Ipv6
        );
        let field_count =
            4 + usize::from(include_family) + usize::from(self.provider_key.is_some());
        let mut connection = serializer.serialize_struct("ProcessConnection", field_count)?;
        connection.serialize_field("protocol", &self.legacy_protocol_name())?;
        if include_family {
            connection.serialize_field("family", &self.family)?;
        }
        connection.serialize_field("local", &self.local)?;
        connection.serialize_field("remote", &self.remote)?;
        connection.serialize_field("state", &self.state)?;
        if let Some(provider_key) = &self.provider_key {
            connection.serialize_field("provider_key", provider_key)?;
        }
        connection.end()
    }
}

impl<'de> Deserialize<'de> for ProcessConnection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ProcessConnectionWire::deserialize(deserializer)?;
        let legacy_family = match wire.protocol.as_str() {
            "tcp6" | "udp6" => Some(ConnectionAddressFamily::Ipv6),
            "tcp" | "udp" => Some(ConnectionAddressFamily::Ipv4),
            _ => None,
        };
        let transport = ConnectionTransport::from_wire(wire.protocol);
        let family = wire
            .family
            .or(legacy_family)
            .or_else(|| Self::inferred_family(&wire.local, &wire.remote))
            .unwrap_or_default();
        Ok(Self {
            transport,
            family,
            local: wire.local,
            remote: wire.remote,
            state: wire.state,
            provider_key: wire.provider_key,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProcessNetworkSnapshot {
    pub state: DeviceState,
    pub connections: Vec<ProcessConnection>,
    pub rx_bytes_per_sec: Option<u64>,
    pub tx_bytes_per_sec: Option<u64>,
    pub traffic_state: DeviceState,
    #[serde(default)]
    pub traffic_failure: Option<FailureKind>,
    #[serde(default)]
    pub traffic_provider: Option<ProviderId>,
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_telemetry_connection_snapshot_tests.rs"]
mod snapshot_tests;
