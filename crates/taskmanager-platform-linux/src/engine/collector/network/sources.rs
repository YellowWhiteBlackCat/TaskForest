//! Independently fallible Linux network sources.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use taskmanager_core::core::device_state::stable_network_id;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::source::SourceOutcome;

use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};
const IW_LINK_TIMEOUT: Duration = Duration::from_secs(2);
const IW_INFO_TIMEOUT: Duration = Duration::from_secs(2);

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
    pub(super) driver: Option<Arc<str>>,
    pub(super) adapter: Option<Arc<str>>,
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
            driver: metadata
                .capture(read_driver(&base.join("device/driver")))
                .map(Arc::from),
            adapter: metadata
                .capture(read_adapter(&base.join("device")))
                .map(Arc::from),
        });
        metadata_item_count += usize::from(metadata.values > 0);
        if let Some(failure) = metadata.failure {
            metadata_failure = Some(select_failure(metadata_failure, failure));
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

pub(super) fn read_proc_wireless(path: &Path) -> SourceObservation<HashMap<String, i32>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) => {
            return SourceObservation {
                value: HashMap::new(),
                outcome: SourceOutcome::Unavailable(io_failure(&error)),
            };
        }
    };
    let parsed = parse_proc_wireless(&content);
    let outcome = match (parsed.signals.is_empty(), parsed.malformed_rows) {
        (true, 0) => SourceOutcome::Empty,
        (false, 0) => SourceOutcome::Available,
        (true, _) => SourceOutcome::Unavailable(FailureKind::ProviderFault),
        (false, _) => SourceOutcome::Partial(FailureKind::ProviderFault),
    };
    SourceObservation {
        value: parsed.signals,
        outcome,
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ParsedWireless {
    signals: HashMap<String, i32>,
    malformed_rows: usize,
}

fn parse_proc_wireless(content: &str) -> ParsedWireless {
    let mut parsed = ParsedWireless::default();
    for line in content.lines() {
        let Some((name, fields)) = line.trim_start().split_once(':') else {
            continue;
        };
        let mut fields = fields.split_whitespace();
        let level = fields
            .nth(2)
            .and_then(|value| value.trim_end_matches('.').parse::<i32>().ok());
        match level {
            Some(level @ -200..=-1) => {
                parsed.signals.insert(name.trim().to_owned(), level);
            }
            Some(0) => {}
            Some(_) | None => parsed.malformed_rows += 1,
        }
    }
    parsed
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum IwLinkResult {
    /// Associated, carrying the SSID plus the parsed `signal:` dBm (modern
    /// mac80211 exposes signal strength via `iw dev <iface> link` but NOT via
    /// `/proc/net/wireless`, which is empty on most current drivers — so the
    /// signal is carried here as a fallback for the wireless observation).
    /// `tx_bitrate_mbps` is the ceiling of the `tx bitrate:` MBit/s token; it
    /// backfills `link_speed_mbps` when `/sys/class/net/<iface>/speed` is
    /// absent or zero (the common WiFi case).
    Associated {
        bssid: Option<String>,
        ssid: String,
        signal_dbm: Option<i32>,
        frequency_mhz: Option<u32>,
        channel: Option<u32>,
        rx_bitrate_mbps: Option<u64>,
        tx_bitrate_mbps: Option<u64>,
        /// Protocol labels come from a fixed vocabulary. Borrowing the
        /// static label keeps the `iw` parser allocation-free; the owned
        /// `Arc<str>` projection is created only when the observation is
        /// assembled for the wire model.
        protocol: Option<&'static str>,
    },
    NotAssociated,
    Failed(FailureKind),
}

pub(super) fn read_iw_links(
    interfaces: Vec<Arc<str>>,
) -> SourceObservation<HashMap<Arc<str>, IwLinkResult>> {
    if interfaces.is_empty() {
        return SourceObservation::from_value(HashMap::new(), 0);
    }

    let mut results = Vec::with_capacity(interfaces.len());
    for interface in interfaces {
        let result = read_iw_link(interface.as_ref());
        let missing_tool = matches!(result, IwLinkResult::Failed(FailureKind::MissingDependency));
        results.push((interface, result));
        if missing_tool {
            break;
        }
    }
    summarize_iw_results(results)
}

fn summarize_iw_results(
    results: Vec<(Arc<str>, IwLinkResult)>,
) -> SourceObservation<HashMap<Arc<str>, IwLinkResult>> {
    let mut observations = HashMap::with_capacity(results.len());
    let mut failure = None;
    let mut success_count = 0;
    for (interface, result) in results {
        match &result {
            IwLinkResult::Associated { .. } | IwLinkResult::NotAssociated => success_count += 1,
            IwLinkResult::Failed(candidate) => {
                failure = Some(select_failure(failure, *candidate));
            }
        }
        observations.insert(interface, result);
    }
    let outcome = match (success_count, failure) {
        (0, None) => SourceOutcome::Empty,
        (_, None) => SourceOutcome::Available,
        (0, Some(failure)) => SourceOutcome::Unavailable(failure),
        (_, Some(failure)) => SourceOutcome::Partial(failure),
    };
    SourceObservation {
        value: observations,
        outcome,
    }
}

fn read_iw_link(interface: &str) -> IwLinkResult {
    let output = match run_with_timeout(
        Command::new("iw").args(["dev", interface, "link"]),
        IW_LINK_TIMEOUT,
    ) {
        Ok(output) => output,
        Err(BoundedCommandError::Spawn(error)) => {
            return IwLinkResult::Failed(command_spawn_failure(&error));
        }
        Err(BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut) => {
            return IwLinkResult::Failed(FailureKind::TimedOut);
        }
        Err(
            BoundedCommandError::ReaderStart(_)
            | BoundedCommandError::ReaderFailed
            | BoundedCommandError::ProcessTree
            | BoundedCommandError::OutputTooLarge,
        ) => {
            return IwLinkResult::Failed(FailureKind::ProviderFault);
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        let mut parsed = parse_iw_link(&stdout);
        if let IwLinkResult::Associated {
            frequency_mhz,
            channel,
            ..
        } = &mut parsed
            && let Some((info_channel, info_frequency_mhz)) = read_iw_info(interface)
        {
            if frequency_mhz.is_none() {
                *frequency_mhz = Some(info_frequency_mhz);
            }
            if channel.is_none() {
                *channel = Some(info_channel);
            }
        }
        return parsed;
    }
    if stdout.contains("Not connected") || stderr.contains("Not connected") {
        IwLinkResult::NotAssociated
    } else {
        IwLinkResult::Failed(iw_output_failure(&stderr))
    }
}

fn read_iw_info(interface: &str) -> Option<(u32, u32)> {
    let output = run_with_timeout(
        Command::new("iw").args(["dev", interface, "info"]),
        IW_INFO_TIMEOUT,
    )
    .ok()?;
    output
        .status
        .success()
        .then(|| parse_iw_info(&String::from_utf8_lossy(&output.stdout)))?
}

fn parse_iw_info(output: &str) -> Option<(u32, u32)> {
    output.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix("channel ")?;
        let channel = rest
            .split_whitespace()
            .next()?
            .parse::<u32>()
            .ok()
            .filter(|channel| *channel > 0)?;
        let frequency_mhz = rest
            .split('(')
            .nth(1)?
            .split_whitespace()
            .next()?
            .parse::<u32>()
            .ok()
            .filter(|frequency| *frequency > 0)?;
        Some((channel, frequency_mhz))
    })
}

fn parse_iw_link(output: &str) -> IwLinkResult {
    if output.lines().any(|line| line.trim() == "Not connected.") {
        return IwLinkResult::NotAssociated;
    }
    // Scan the WHOLE `iw dev <iface> link` block: SSID, then the trailing
    // `signal:` and `tx bitrate:` lines. The old code returned at the SSID line
    // and discarded the signal it had already fetched.
    let mut bssid = None;
    let mut ssid = None;
    let mut signal_dbm = None;
    let mut frequency_mhz = None;
    let mut rx_bitrate_mbps = None;
    let mut tx_bitrate_mbps = None;
    let mut protocol = None;
    for line in output.lines() {
        let line = line.trim();
        if bssid.is_none()
            && let Some(rest) = line.strip_prefix("Connected to ")
            && let Some(candidate) = rest.split_whitespace().next()
        {
            bssid = parse_bssid(candidate);
        }
        if ssid.is_none()
            && let Some(s) = line.strip_prefix("SSID:").map(str::trim)
            && !s.is_empty()
        {
            ssid = Some(s.to_owned());
        }
        // "signal: -52 dBm"
        if signal_dbm.is_none()
            && let Some(rest) = line.strip_prefix("signal:")
            && let Some(tok) = rest.split_whitespace().next()
            && let Ok(dbm) = tok.parse::<i32>()
        {
            signal_dbm = Some(dbm);
        }
        if frequency_mhz.is_none()
            && let Some(rest) = line.strip_prefix("freq:")
            && let Some(tok) = rest.split_whitespace().next()
            && let Ok(frequency) = tok.parse::<u32>()
            && frequency > 0
        {
            frequency_mhz = Some(frequency);
        }
        if let Some(rest) = line.strip_prefix("rx bitrate:") {
            let (bitrate, candidate_protocol) = parse_bitrate(rest);
            if rx_bitrate_mbps.is_none() {
                rx_bitrate_mbps = bitrate;
            }
            if protocol.is_none() {
                protocol = candidate_protocol;
            }
        }
        // "tx bitrate: 866.7 MBit/s" — ceiling to u64 Mbps (866.7 → 867). The
        // unit suffix varies (MBit/s, GBit/s on some regs) so only the leading
        // numeric token is trusted; the caller treats None as "no fallback".
        if let Some(rest) = line.strip_prefix("tx bitrate:") {
            let (bitrate, candidate_protocol) = parse_bitrate(rest);
            if tx_bitrate_mbps.is_none() {
                tx_bitrate_mbps = bitrate;
            }
            if protocol.is_none() {
                protocol = candidate_protocol;
            }
        }
    }
    let channel = frequency_mhz.and_then(channel_from_frequency);
    match ssid {
        Some(ssid) => IwLinkResult::Associated {
            bssid,
            ssid,
            signal_dbm,
            frequency_mhz,
            channel,
            rx_bitrate_mbps,
            tx_bitrate_mbps,
            protocol,
        },
        None => IwLinkResult::Failed(FailureKind::ProviderFault),
    }
}

fn parse_bssid(value: &str) -> Option<String> {
    let is_mac = value.len() == 17
        && value.split(':').count() == 6
        && value
            .split(':')
            .all(|octet| octet.len() == 2 && octet.bytes().all(|byte| byte.is_ascii_hexdigit()));
    is_mac.then(|| value.to_ascii_lowercase())
}

fn parse_bitrate(rest: &str) -> (Option<u64>, Option<&'static str>) {
    let mut tokens = rest.split_whitespace();
    let bitrate = tokens
        .next()
        .and_then(|token| token.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .and_then(|value| {
            let unit = tokens.next().unwrap_or_default().to_ascii_lowercase();
            let multiplier = if unit.starts_with('g') {
                1_000.0
            } else if unit.starts_with('k') {
                0.001
            } else {
                1.0
            };
            let mbps = value * multiplier;
            mbps.is_finite()
                .then_some(mbps)
                .filter(|value| *value > 0.0)
        })
        .map(f64::ceil)
        .and_then(|value| u64::try_from(value as u128).ok());
    let protocol = [
        ("EHT-MCS", "802.11be (Wi-Fi 7)"),
        ("HE-MCS", "802.11ax (Wi-Fi 6/6E)"),
        ("VHT-MCS", "802.11ac (Wi-Fi 5)"),
        ("HT-MCS", "802.11n (Wi-Fi 4)"),
    ]
    .iter()
    .find_map(|(needle, label)| rest.contains(needle).then_some(*label));
    (bitrate, protocol)
}

fn channel_from_frequency(frequency_mhz: u32) -> Option<u32> {
    if frequency_mhz == 2484 {
        return Some(14);
    }
    if (2412..=2472).contains(&frequency_mhz) && (frequency_mhz - 2412).is_multiple_of(5) {
        return Some((frequency_mhz - 2407) / 5);
    }
    if (5005..=5895).contains(&frequency_mhz) && (frequency_mhz - 5000).is_multiple_of(5) {
        return Some((frequency_mhz - 5000) / 5);
    }
    if (5955..=7115).contains(&frequency_mhz) && (frequency_mhz - 5950).is_multiple_of(5) {
        return Some((frequency_mhz - 5950) / 5);
    }
    None
}

fn iw_output_failure(stderr: &str) -> FailureKind {
    let stderr = stderr.to_ascii_lowercase();
    if stderr.contains("permission denied") || stderr.contains("operation not permitted") {
        FailureKind::PermissionDenied
    } else if stderr.contains("no such device") {
        FailureKind::IdentityChanged
    } else if stderr.contains("not supported") || stderr.contains("operation not supported") {
        FailureKind::Unsupported
    } else {
        FailureKind::TemporarilyUnavailable
    }
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
