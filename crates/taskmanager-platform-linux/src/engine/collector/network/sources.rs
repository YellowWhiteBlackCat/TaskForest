//! Independently fallible Linux network sources.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::sync::Arc;

use taskmanager_core::core::device_state::stable_network_id;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::source::SourceOutcome;

mod wireless;
pub(super) use wireless::{IwLinkResult, read_iw_links, read_proc_wireless};

#[derive(Clone, Debug)]
pub(super) struct SourceObservation<T> {
    pub(super) value: T,
    pub(super) outcome: SourceOutcome,
}

impl<T> SourceObservation<T> {
    fn from_value(value: T, item_count: usize) -> Self {
        Self {
            value,
            outcome: if item_count == 0 {
                SourceOutcome::Empty
            } else {
                SourceOutcome::Available
            },
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SysfsInterface {
    pub(super) name: Arc<str>,
    /// Precomputed once per inventory so the counter and snapshot paths share
    /// one stable identity instead of re-running the identity sanitizer and
    /// format per tick.
    pub(super) stable_id: Arc<str>,
    pub(super) arp_type: Option<u64>,
    pub(super) mac_addr: Option<Arc<str>>,
    pub(super) link_speed: ScalarObservation<u64>,
    pub(super) link_up: ScalarObservation<bool>,
    pub(super) mtu_bytes: ScalarObservation<u32>,
    pub(super) tx_queue_len: ScalarObservation<u32>,
    pub(super) rx_drops: ScalarObservation<u64>,
    pub(super) tx_drops: ScalarObservation<u64>,
    pub(super) rx_errors: ScalarObservation<u64>,
    pub(super) tx_errors: ScalarObservation<u64>,
    pub(super) rx_overruns: ScalarObservation<u64>,
    pub(super) tx_overruns: ScalarObservation<u64>,
    pub(super) driver: Option<Arc<str>>,
    pub(super) adapter: Option<Arc<str>>,
    pub(super) master_interface: Option<Arc<str>>,
    pub(super) peer_interface: Option<Arc<str>>,
    pub(super) ifindex: Option<u32>,
    pub(super) iflink: Option<u32>,
}

#[derive(Clone, Debug)]
pub(super) struct SysfsInventoryObservation {
    pub(super) value: Vec<SysfsInterface>,
    pub(super) discovery_outcome: SourceOutcome,
    pub(super) metadata_outcome: SourceOutcome,
    pub(super) metadata_item_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct InterfaceAddresses {
    pub(super) ipv4: Option<String>,
    pub(super) ipv6: Option<String>,
}

pub(super) fn read_sysfs_inventory(root: &Path, now_ms: u64) -> SysfsInventoryObservation {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            let failure = io_failure(&error);
            return SysfsInventoryObservation {
                value: Vec::new(),
                discovery_outcome: SourceOutcome::Unavailable(failure),
                metadata_outcome: SourceOutcome::Unavailable(failure),
                metadata_item_count: 0,
            };
        }
    };

    let mut interfaces = Vec::new();
    let mut discovery_failure = None;
    let mut metadata_failure = None;
    let mut metadata_item_count = 0;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                discovery_failure = Some(io_failure(&error));
                continue;
            }
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            discovery_failure = Some(FailureKind::ProviderFault);
            continue;
        };
        // Keep every `/sys/class/net` entry in the authoritative inventory.
        // Mission Center exposes separate visibility controls for VPN and
        // virtual adapters; dropping them here would make those controls a
        // UI-only fiction and would also prevent hot-plug lifecycle tracking.
        let base = entry.path();
        let mut metadata = MetadataAudit::default();
        let link_speed = metadata.capture_scalar(read_link_speed(&base.join("speed"), now_ms));
        let link_up = metadata.capture_scalar(read_link_up(&base.join("carrier"), now_ms));
        let mtu_bytes = metadata.capture_scalar(read_mtu(&base.join("mtu"), now_ms));
        let tx_queue_len =
            metadata.capture_scalar(read_tx_queue_len(&base.join("tx_queue_len"), now_ms));
        let statistics = base.join("statistics");
        let rx_drops =
            metadata.capture_scalar(read_counter(&statistics.join("rx_dropped"), now_ms));
        let tx_drops =
            metadata.capture_scalar(read_counter(&statistics.join("tx_dropped"), now_ms));
        let rx_errors =
            metadata.capture_scalar(read_counter(&statistics.join("rx_errors"), now_ms));
        let tx_errors =
            metadata.capture_scalar(read_counter(&statistics.join("tx_errors"), now_ms));
        let rx_overruns =
            metadata.capture_scalar(read_counter(&statistics.join("rx_over_errors"), now_ms));
        let tx_overruns =
            metadata.capture_scalar(read_counter(&statistics.join("tx_over_errors"), now_ms));
        let mac_addr = metadata
            .capture(read_mac(&base.join("address")))
            .map(Arc::from);
        interfaces.push(SysfsInterface {
            stable_id: Arc::from(stable_network_id(&name, mac_addr.as_deref())),
            name: Arc::from(name),
            arp_type: metadata.capture(read_u64(&base.join("type"))),
            mac_addr,
            link_speed,
            link_up,
            mtu_bytes,
            tx_queue_len,
            rx_drops,
            tx_drops,
            rx_errors,
            tx_errors,
            rx_overruns,
            tx_overruns,
            driver: metadata
                .capture(read_driver(&base.join("device/driver")))
                .map(Arc::from),
            adapter: metadata
                .capture(read_adapter(&base.join("device")))
                .map(Arc::from),
            master_interface: metadata
                .capture(read_optional_link_name(&base.join("master")))
                .map(Arc::from),
            peer_interface: None,
            ifindex: metadata.capture(read_u32(&base.join("ifindex"))),
            iflink: metadata.capture(read_u32(&base.join("iflink"))),
        });
        metadata_item_count += usize::from(metadata.values > 0);
        if let Some(failure) = metadata.failure {
            metadata_failure = Some(select_failure(metadata_failure, failure));
        }
    }
    let by_ifindex = interfaces
        .iter()
        .filter_map(|interface| {
            interface
                .ifindex
                .map(|index| (index, interface.name.clone()))
        })
        .collect::<HashMap<_, _>>();
    for interface in &mut interfaces {
        if let (Some(ifindex), Some(iflink)) = (interface.ifindex, interface.iflink)
            && ifindex != iflink
        {
            interface.peer_interface = by_ifindex.get(&iflink).cloned();
        }
    }
    interfaces.sort_by(|left, right| left.name.cmp(&right.name));

    let discovery_outcome = match discovery_failure {
        Some(failure) if interfaces.is_empty() => SourceOutcome::Unavailable(failure),
        Some(failure) => SourceOutcome::Partial(failure),
        None if interfaces.is_empty() => SourceOutcome::Empty,
        None => SourceOutcome::Available,
    };
    let metadata_outcome = match (interfaces.is_empty(), metadata_failure) {
        (true, None) => SourceOutcome::Empty,
        (true, Some(failure)) => SourceOutcome::Unavailable(failure),
        (false, None) => SourceOutcome::Available,
        (false, Some(failure)) if metadata_item_count == 0 => SourceOutcome::Unavailable(failure),
        (false, Some(failure)) => SourceOutcome::Partial(failure),
    };
    SysfsInventoryObservation {
        value: interfaces,
        discovery_outcome,
        metadata_outcome,
        metadata_item_count,
    }
}

#[derive(Default)]
struct MetadataAudit {
    values: usize,
    failure: Option<FailureKind>,
}

impl MetadataAudit {
    fn capture<T>(&mut self, observed: Result<Option<T>, FailureKind>) -> Option<T> {
        match observed {
            Ok(Some(value)) => {
                self.values += 1;
                Some(value)
            }
            Ok(None) => None,
            Err(failure) => {
                self.failure = Some(select_failure(self.failure, failure));
                None
            }
        }
    }

    fn capture_scalar<T>(&mut self, observed: ScalarObservation<T>) -> ScalarObservation<T> {
        if observed.availability().is_current() {
            self.values += 1;
        } else if let Some(failure) = observed.availability().failure() {
            self.failure = Some(select_failure(self.failure, failure));
        }
        observed
    }
}

fn read_trimmed(path: &Path) -> Result<String, FailureKind> {
    fs::read_to_string(path)
        .map_err(|error| io_failure(&error))
        .and_then(|value| {
            let value = value.trim().to_owned();
            if value.is_empty() {
                Err(FailureKind::ProviderFault)
            } else {
                Ok(value)
            }
        })
}

fn read_u64(path: &Path) -> Result<Option<u64>, FailureKind> {
    read_trimmed(path)
        .and_then(|value| value.parse().map_err(|_| FailureKind::ProviderFault))
        .map(Some)
}

fn read_u32(path: &Path) -> Result<Option<u32>, FailureKind> {
    read_trimmed(path)
        .and_then(|value| value.parse::<u32>().map_err(|_| FailureKind::ProviderFault))
        .map(Some)
}

fn read_mac(path: &Path) -> Result<Option<String>, FailureKind> {
    read_trimmed(path)
        .map(|value| value.to_ascii_lowercase())
        .map(|value| (value != "00:00:00:00:00:00").then_some(value))
}

fn read_link_speed(path: &Path, now_ms: u64) -> ScalarObservation<u64> {
    match read_trimmed(path)
        .and_then(|value| value.parse::<i64>().map_err(|_| FailureKind::ProviderFault))
    {
        Ok(value) if value > 0 => match u64::try_from(value) {
            Ok(value) => ScalarObservation::available(value, now_ms),
            Err(_) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        },
        Ok(_) => ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        Err(failure) => ScalarObservation::unavailable(failure),
    }
}

fn read_link_up(path: &Path, now_ms: u64) -> ScalarObservation<bool> {
    match read_trimmed(path) {
        Ok(value) if value == "1" => ScalarObservation::available(true, now_ms),
        Ok(value) if value == "0" => ScalarObservation::available(false, now_ms),
        Ok(_) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        Err(failure) => ScalarObservation::unavailable(failure),
    }
}

fn read_mtu(path: &Path, now_ms: u64) -> ScalarObservation<u32> {
    match read_trimmed(path)
        .and_then(|value| value.parse::<u64>().map_err(|_| FailureKind::ProviderFault))
    {
        Ok(value) if (1..=u64::from(u32::MAX)).contains(&value) => {
            ScalarObservation::available(value as u32, now_ms)
        }
        Ok(_) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        Err(failure) => ScalarObservation::unavailable(failure),
    }
}

fn read_tx_queue_len(path: &Path, now_ms: u64) -> ScalarObservation<u32> {
    match read_trimmed(path)
        .and_then(|value| value.parse::<u64>().map_err(|_| FailureKind::ProviderFault))
    {
        Ok(value) if value <= u64::from(u32::MAX) => {
            ScalarObservation::available(value as u32, now_ms)
        }
        Ok(_) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        Err(failure) => ScalarObservation::unavailable(failure),
    }
}

fn read_counter(path: &Path, now_ms: u64) -> ScalarObservation<u64> {
    match read_trimmed(path)
        .and_then(|value| value.parse::<u64>().map_err(|_| FailureKind::ProviderFault))
    {
        Ok(value) => ScalarObservation::available(value, now_ms),
        Err(failure) => ScalarObservation::unavailable(failure),
    }
}

fn read_driver(path: &Path) -> Result<Option<String>, FailureKind> {
    fs::read_link(path)
        .map_err(|error| io_failure(&error))
        .and_then(|driver| {
            driver
                .file_name()
                .map(|name| Some(name.to_string_lossy().into_owned()))
                .ok_or(FailureKind::ProviderFault)
        })
}

fn read_optional_link_name(path: &Path) -> Result<Option<String>, FailureKind> {
    match fs::read_link(path) {
        Ok(link) => link
            .file_name()
            .map(|name| Some(name.to_string_lossy().into_owned()))
            .ok_or(FailureKind::ProviderFault),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_failure(&error)),
    }
}

fn read_adapter(device_path: &Path) -> Result<Option<String>, FailureKind> {
    match read_trimmed(&device_path.join("model")) {
        Ok(model) => return Ok(Some(model)),
        Err(FailureKind::Unsupported) => {}
        Err(failure) => return Err(failure),
    }
    let vendor = read_trimmed(&device_path.join("vendor"))?;
    let device = read_trimmed(&device_path.join("device"))?;
    Ok(Some(format!("{vendor}:{device}")))
}

#[cfg(target_os = "linux")]
pub(super) fn enumerate_addresses() -> SourceObservation<HashMap<String, InterfaceAddresses>> {
    let addresses = match nix::ifaddrs::getifaddrs() {
        Ok(addresses) => addresses,
        Err(error) => {
            return SourceObservation {
                value: HashMap::new(),
                outcome: SourceOutcome::Unavailable(errno_failure(error)),
            };
        }
    };

    let mut by_interface = HashMap::<String, InterfaceAddresses>::new();
    let mut item_count = 0;
    for address in addresses {
        let Some(storage) = address.address else {
            continue;
        };
        let target = by_interface.entry(address.interface_name).or_default();
        if let Some(socket) = storage.as_sockaddr_in() {
            let candidate = socket.ip();
            if prefer_ipv4(target.ipv4.as_deref(), candidate) {
                target.ipv4 = Some(candidate.to_string());
            }
            item_count += 1;
        } else if let Some(socket) = storage.as_sockaddr_in6() {
            let candidate = socket.ip();
            if prefer_ipv6(target.ipv6.as_deref(), candidate) {
                target.ipv6 = Some(candidate.to_string());
            }
            item_count += 1;
        }
    }
    SourceObservation::from_value(by_interface, item_count)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn enumerate_addresses() -> SourceObservation<HashMap<String, InterfaceAddresses>> {
    SourceObservation {
        value: HashMap::new(),
        outcome: SourceOutcome::Unavailable(FailureKind::Unsupported),
    }
}

fn prefer_ipv4(current: Option<&str>, candidate: Ipv4Addr) -> bool {
    current.is_none()
        || (!candidate.is_loopback() && current.is_some_and(|value| value == "127.0.0.1"))
}

fn prefer_ipv6(current: Option<&str>, candidate: Ipv6Addr) -> bool {
    current.is_none()
        || (!candidate.is_unicast_link_local()
            && current.is_some_and(|value| value.starts_with("fe80:")))
}

fn select_failure(current: Option<FailureKind>, candidate: FailureKind) -> FailureKind {
    match current {
        Some(current) if failure_priority(current) >= failure_priority(candidate) => current,
        _ => candidate,
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 8,
        FailureKind::PermissionDenied => 7,
        FailureKind::MissingDependency => 6,
        FailureKind::TimedOut => 5,
        FailureKind::ProviderFault => 4,
        FailureKind::TemporarilyUnavailable => 3,
        FailureKind::Unsupported => 2,
        FailureKind::IdentityChanged | FailureKind::Rejected => 1,
    }
}

fn io_failure(error: &io::Error) -> FailureKind {
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::Unsupported => FailureKind::Unsupported,
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        io::ErrorKind::InvalidData => FailureKind::ProviderFault,
        io::ErrorKind::TimedOut => FailureKind::TimedOut,
        _ => FailureKind::TemporarilyUnavailable,
    }
}

fn command_spawn_failure(error: &io::Error) -> FailureKind {
    match error.kind() {
        io::ErrorKind::NotFound => FailureKind::MissingDependency,
        _ => io_failure(error),
    }
}

#[cfg(target_os = "linux")]
fn errno_failure(error: nix::errno::Errno) -> FailureKind {
    match error {
        nix::errno::Errno::EACCES | nix::errno::Errno::EPERM => FailureKind::PermissionDenied,
        nix::errno::Errno::ENOSYS => FailureKind::Unsupported,
        _ => FailureKind::TemporarilyUnavailable,
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/engine/collector/network/sources.rs"]
mod tests;
