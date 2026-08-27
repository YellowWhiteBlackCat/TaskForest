//! Linux per-process socket ownership and connection parsing.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;

use serde::{Deserialize, Serialize};

use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::{FailureKind, ProviderId};

use super::{
    ConnectionAddressFamily, ConnectionEndpoint, ConnectionState, ConnectionTransport,
    ProcessConnection, ProcessIdentity, ProcessNetworkSnapshot, state_for_status,
    status_from_io_error,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkByteCounters {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkAccountingFailure {
    Unsupported,
    PermissionDenied,
    /// The AF_PACKET open was denied for lack of `CAP_NET_RAW`, and the gate
    /// confirms [`EscalationFeature::PerProcessNet`] is escalatable — surfaces
    /// as [`FailureKind::RequiresEscalation`] so the UI can offer the prompt.
    /// Collapses to [`DeviceStatus::PermissionDenied`] at the coarse layer.
    RequiresEscalation,
    AbiMismatch,
    Rejected,
    ResourceExhausted,
    MapLost,
    CounterRollback,
    IdentityChanged,
    Unavailable,
}

impl NetworkAccountingFailure {
    const fn failure_kind(self) -> FailureKind {
        match self {
            Self::Unsupported => FailureKind::Unsupported,
            Self::PermissionDenied => FailureKind::PermissionDenied,
            Self::RequiresEscalation => FailureKind::RequiresEscalation,
            Self::AbiMismatch | Self::CounterRollback => FailureKind::ProviderFault,
            Self::Rejected => FailureKind::Rejected,
            Self::ResourceExhausted | Self::MapLost | Self::Unavailable => {
                FailureKind::TemporarilyUnavailable
            }
            Self::IdentityChanged => FailureKind::IdentityChanged,
        }
    }

    const fn device_status(self) -> DeviceStatus {
        match self {
            Self::Unsupported => DeviceStatus::Unsupported,
            Self::PermissionDenied | Self::RequiresEscalation => DeviceStatus::PermissionDenied,
            Self::AbiMismatch
            | Self::Rejected
            | Self::ResourceExhausted
            | Self::MapLost
            | Self::CounterRollback
            | Self::IdentityChanged
            | Self::Unavailable => DeviceStatus::Stale,
        }
    }
}

/// Pluggable cumulative byte-accounting source.
///
/// Linux procfs socket tables prove ownership and endpoints but do not expose
/// bytes per PID, which needs an out-of-band source. The pure-safe-Rust build
/// ships no such source (eBPF was removed to keep the workspace
/// `#![forbid(unsafe_code)]` with zero carve-outs; see the "strict safe-Rust"
/// ADR), so the default backend below reports `Unsupported`. The trait stays as
/// the seam a future audited safe wrapper can fill; connection ownership and
/// aggregate interface counters are unaffected — only per-PID byte attribution
/// is unavailable.
pub trait ProcessNetworkAccountingBackend: Send {
    fn provider(&self) -> Option<ProviderId> {
        None
    }

    fn read_counters(
        &mut self,
        identity: ProcessIdentity,
        now_ms: u64,
    ) -> Result<NetworkByteCounters, NetworkAccountingFailure>;
}

/// Default safe accounting backend: no per-PID byte source is available in the
/// pure-safe-Rust build, so every process reports `Unsupported` traffic.
#[derive(Debug, Default)]
pub struct UnsupportedNetworkAccountingBackend;

impl ProcessNetworkAccountingBackend for UnsupportedNetworkAccountingBackend {
    fn read_counters(
        &mut self,
        _identity: ProcessIdentity,
        _now_ms: u64,
    ) -> Result<NetworkByteCounters, NetworkAccountingFailure> {
        Err(NetworkAccountingFailure::Unsupported)
    }
}

#[derive(Debug, Clone)]
struct PreviousAccounting {
    timestamp_ms: u64,
    counters: NetworkByteCounters,
    state: DeviceState,
    provider: Option<ProviderId>,
}

#[derive(Debug, Default)]
pub struct ProcessNetworkRateTracker {
    previous: HashMap<ProcessIdentity, PreviousAccounting>,
}

impl ProcessNetworkRateTracker {
    /// Drop baselines whose pid is absent from the authoritative live pid set.
    ///
    /// The per-observe retain above only resets a pid's own generation on
    /// reuse; pids the user once inspected but that have since exited are
    /// never revisited, so without this pass their entries would accumulate
    /// without bound. Driven by the provider layer on the same process-list
    /// tick that revalidates the target (every currently live pid stays, so
    /// concurrent multi-target insights do not evict each other).
    pub fn retain_live_pids(&mut self, live_pids: &HashSet<u32>) {
        self.previous
            .retain(|known, _| live_pids.contains(&known.pid));
    }

    pub fn observe(
        &mut self,
        identity: ProcessIdentity,
        now_ms: u64,
        provider: Option<ProviderId>,
        observation: Result<NetworkByteCounters, NetworkAccountingFailure>,
        snapshot: &mut ProcessNetworkSnapshot,
    ) {
        self.previous
            .retain(|known, _| known.pid != identity.pid || *known == identity);
        let previous = self.previous.get(&identity).cloned();
        match observation {
            Ok(counters) => {
                snapshot.traffic_provider = provider.clone();
                snapshot.traffic_failure = None;
                snapshot.traffic_state = previous
                    .as_ref()
                    .map(|sample| sample.state)
                    .unwrap_or_default()
                    .transition(DeviceStatus::Healthy, now_ms);
                if let Some(sample) = previous.as_ref()
                    && now_ms > sample.timestamp_ms
                    && counters.rx_bytes >= sample.counters.rx_bytes
                    && counters.tx_bytes >= sample.counters.tx_bytes
                {
                    let elapsed_ms = now_ms - sample.timestamp_ms;
                    snapshot.rx_bytes_per_sec = Some(rate_per_second(
                        counters.rx_bytes - sample.counters.rx_bytes,
                        elapsed_ms,
                    ));
                    snapshot.tx_bytes_per_sec = Some(rate_per_second(
                        counters.tx_bytes - sample.counters.tx_bytes,
                        elapsed_ms,
                    ));
                }
                self.previous.insert(
                    identity,
                    PreviousAccounting {
                        timestamp_ms: now_ms,
                        counters,
                        state: snapshot.traffic_state,
                        provider,
                    },
                );
            }
            Err(failure) => {
                let status = failure.device_status();
                snapshot.traffic_provider = provider
                    .or_else(|| previous.as_ref().and_then(|sample| sample.provider.clone()));
                snapshot.traffic_failure = Some(failure.failure_kind());
                snapshot.traffic_state = previous
                    .map(|sample| sample.state)
                    .unwrap_or_default()
                    .transition(status, now_ms);
            }
        }
    }
}

fn rate_per_second(delta: u64, elapsed_ms: u64) -> u64 {
    delta.saturating_mul(1_000) / elapsed_ms
}

pub(super) fn collect_from_proc_dir(proc_dir: &Path, now_ms: u64) -> ProcessNetworkSnapshot {
    let fd_dir = proc_dir.join("fd");
    let entries = match std::fs::read_dir(fd_dir) {
        Ok(entries) => entries,
        Err(error) => {
            return ProcessNetworkSnapshot {
                state: state_for_status(status_from_io_error(&error), now_ms),
                traffic_state: state_for_status(DeviceStatus::Unsupported, now_ms),
                ..Default::default()
            };
        }
    };
    let mut socket_inodes = HashSet::new();
    for entry in entries.flatten() {
        if let Ok(target) = std::fs::read_link(entry.path())
            && let Some(inode) = parse_socket_inode(&target.to_string_lossy())
        {
            socket_inodes.insert(inode);
        }
    }

    let mut connections = Vec::new();
    let mut denied = false;
    let mut unavailable = false;
    let mut readable_tables = 0_u8;
    for (file, transport, family) in [
        (
            "tcp",
            ConnectionTransport::Tcp,
            ConnectionAddressFamily::Ipv4,
        ),
        (
            "tcp6",
            ConnectionTransport::Tcp,
            ConnectionAddressFamily::Ipv6,
        ),
        (
            "udp",
            ConnectionTransport::Udp,
            ConnectionAddressFamily::Ipv4,
        ),
        (
            "udp6",
            ConnectionTransport::Udp,
            ConnectionAddressFamily::Ipv6,
        ),
    ] {
        match std::fs::read_to_string(proc_dir.join("net").join(file)) {
            Ok(text) => {
                readable_tables = readable_tables.saturating_add(1);
                connections.extend(
                    parse_socket_table(&text, transport, family)
                        .into_iter()
                        .filter(|connection| {
                            connection
                                .provider_key
                                .as_ref()
                                .and_then(|key| key.as_numeric())
                                .is_some_and(|key| socket_inodes.contains(&key))
                        }),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => denied = true,
            Err(_) => unavailable = true,
        }
    }
    match std::fs::read_to_string(proc_dir.join("net/unix")) {
        Ok(text) => {
            readable_tables = readable_tables.saturating_add(1);
            connections.extend(
                parse_local_socket_table(&text)
                    .into_iter()
                    .filter(|connection| {
                        connection
                            .provider_key
                            .as_ref()
                            .and_then(|key| key.as_numeric())
                            .is_some_and(|key| socket_inodes.contains(&key))
                    }),
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => denied = true,
        Err(_) => unavailable = true,
    }
    connections.sort_by(|left, right| {
        connection_transport_order(&left.transport)
            .cmp(&connection_transport_order(&right.transport))
            .then_with(|| {
                connection_family_order(&left.family).cmp(&connection_family_order(&right.family))
            })
            .then_with(|| left.provider_key.cmp(&right.provider_key))
    });
    let status = if denied {
        DeviceStatus::PermissionDenied
    } else if unavailable || readable_tables == 0 {
        DeviceStatus::Stale
    } else {
        DeviceStatus::Healthy
    };
    ProcessNetworkSnapshot {
        state: state_for_status(status, now_ms),
        connections,
        rx_bytes_per_sec: None,
        tx_bytes_per_sec: None,
        traffic_state: state_for_status(DeviceStatus::Unsupported, now_ms),
        traffic_failure: Some(FailureKind::Unsupported),
        traffic_provider: None,
    }
}

fn parse_socket_inode(target: &str) -> Option<u64> {
    target
        .strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

pub fn parse_socket_table(
    text: &str,
    transport: ConnectionTransport,
    family: ConnectionAddressFamily,
) -> Vec<ProcessConnection> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let columns = line.split_whitespace().collect::<Vec<_>>();
            let local = parse_endpoint(columns.get(1)?, &family)?;
            let remote = parse_endpoint(columns.get(2)?, &family)?;
            let state = parse_connection_state(columns.get(3)?, &transport);
            let provider_key = columns.get(9)?.parse::<u64>().ok()?;
            Some(ProcessConnection {
                transport: transport.clone(),
                family: family.clone(),
                local: local.into(),
                remote: remote.into(),
                state,
                provider_key: Some(provider_key.into()),
            })
        })
        .collect()
}

/// Parse `/proc/<pid>/net/unix` without mapping local sockets onto fake INET
/// wildcard addresses. Named and abstract paths retain their exact procfs text;
/// unnamed sockets use the explicit `Unspecified` endpoint.
pub fn parse_local_socket_table(text: &str) -> Vec<ProcessConnection> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let (flags, raw_state, provider_key, path) = split_local_socket_row(line)?;
            let flags = u64::from_str_radix(flags, 16).ok()?;
            let provider_key = provider_key.parse::<u64>().ok()?;
            let local = path.map_or(ConnectionEndpoint::Unspecified, ConnectionEndpoint::local);
            let state = if flags & 0x0001_0000 != 0 {
                ConnectionState::Listen
            } else {
                match raw_state {
                    "01" => ConnectionState::Unconnected,
                    "03" => ConnectionState::Established,
                    _ => ConnectionState::Unknown,
                }
            };
            Some(ProcessConnection {
                transport: ConnectionTransport::Local,
                family: ConnectionAddressFamily::Local,
                local,
                remote: ConnectionEndpoint::Unspecified,
                state,
                provider_key: Some(provider_key.into()),
            })
        })
        .collect()
}

/// Split the seven fixed `/proc/net/unix` columns while preserving the
/// remainder byte-for-byte as the local path. A Unix socket pathname may
/// contain spaces, so ordinary `split_whitespace()` would truncate it.
fn split_local_socket_row(line: &str) -> Option<(&str, &str, &str, Option<&str>)> {
    let mut rest = line.trim_start();
    let mut columns = [""; 7];
    for column in &mut columns {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        *column = rest.get(..end)?;
        rest = rest.get(end..)?.trim_start_matches(char::is_whitespace);
    }
    let path = (!rest.is_empty()).then_some(rest);
    Some((columns[3], columns[5], columns[6], path))
}

fn parse_endpoint(value: &str, family: &ConnectionAddressFamily) -> Option<SocketAddr> {
    let (address, port) = value.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let ip = match family {
        ConnectionAddressFamily::Ipv4 => {
            let raw = u32::from_str_radix(address, 16).ok()?;
            IpAddr::V4(Ipv4Addr::from(raw.to_le_bytes()))
        }
        ConnectionAddressFamily::Ipv6 => {
            if address.len() != 32 {
                return None;
            }
            let mut bytes = [0u8; 16];
            for index in 0..4 {
                let start = index * 8;
                let word = u32::from_str_radix(address.get(start..start + 8)?, 16).ok()?;
                bytes[start / 2..start / 2 + 4].copy_from_slice(&word.to_le_bytes());
            }
            IpAddr::V6(Ipv6Addr::from(bytes))
        }
        ConnectionAddressFamily::Local
        | ConnectionAddressFamily::Unspecified
        | ConnectionAddressFamily::Other(_) => return None,
    };
    Some(SocketAddr::new(ip, port))
}

fn parse_connection_state(raw: &str, transport: &ConnectionTransport) -> ConnectionState {
    if matches!(transport, ConnectionTransport::Udp) && raw == "07" {
        return ConnectionState::Unconnected;
    }
    match raw {
        "01" => ConnectionState::Established,
        "02" => ConnectionState::SynSent,
        "03" => ConnectionState::SynReceived,
        "04" => ConnectionState::FinWait1,
        "05" => ConnectionState::FinWait2,
        "06" => ConnectionState::TimeWait,
        "07" => ConnectionState::Closed,
        "08" => ConnectionState::CloseWait,
        "09" => ConnectionState::LastAck,
        "0A" => ConnectionState::Listen,
        "0B" => ConnectionState::Closing,
        _ => ConnectionState::Unknown,
    }
}

fn connection_transport_order(transport: &ConnectionTransport) -> u8 {
    match transport {
        ConnectionTransport::Tcp => 0,
        ConnectionTransport::Udp => 1,
        ConnectionTransport::Sctp => 2,
        ConnectionTransport::Local => 3,
        ConnectionTransport::Other(_) => 4,
    }
}

fn connection_family_order(family: &ConnectionAddressFamily) -> u8 {
    match family {
        ConnectionAddressFamily::Ipv4 => 0,
        ConnectionAddressFamily::Ipv6 => 1,
        ConnectionAddressFamily::Local => 2,
        ConnectionAddressFamily::Unspecified => 3,
        ConnectionAddressFamily::Other(_) => 4,
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_process_telemetry_network_tests.rs"]
mod tests;
