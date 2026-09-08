//! Wireless link enrichment from `/proc/net/wireless` and bounded `iw` calls.

use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::source::SourceOutcome;
use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

use super::{SourceObservation, command_spawn_failure, io_failure, select_failure};

const IW_LINK_TIMEOUT: Duration = Duration::from_secs(2);
const IW_INFO_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) fn read_proc_wireless(
    path: &std::path::Path,
) -> SourceObservation<HashMap<String, i32>> {
    let content = match std::fs::read_to_string(path) {
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
pub(crate) struct ParsedWireless {
    pub(super) signals: HashMap<String, i32>,
    pub(super) malformed_rows: usize,
}

pub(crate) fn parse_proc_wireless(content: &str) -> ParsedWireless {
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
pub(crate) enum IwLinkResult {
    Associated {
        bssid: Option<String>,
        ssid: String,
        signal_dbm: Option<i32>,
        frequency_mhz: Option<u32>,
        channel: Option<u32>,
        channel_width_mhz: Option<u32>,
        rx_bitrate_mbps: Option<u64>,
        tx_bitrate_mbps: Option<u64>,
        protocol: Option<&'static str>,
    },
    NotAssociated,
    Failed(FailureKind),
}

pub(crate) fn read_iw_links(
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

pub(crate) fn summarize_iw_results(
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
            channel_width_mhz,
            ..
        } = &mut parsed
            && let Some((info_channel, info_frequency_mhz, info_width_mhz)) =
                read_iw_info(interface)
        {
            if frequency_mhz.is_none() {
                *frequency_mhz = Some(info_frequency_mhz);
            }
            if channel.is_none() {
                *channel = Some(info_channel);
            }
            if channel_width_mhz.is_none() {
                *channel_width_mhz = info_width_mhz;
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

fn read_iw_info(interface: &str) -> Option<(u32, u32, Option<u32>)> {
    let output = run_with_timeout(
        Command::new("iw").args(["dev", interface, "info"]),
        IW_INFO_TIMEOUT,
    )
    .ok()?;
    output
        .status
        .success()
        .then(|| parse_iw_info_details(&String::from_utf8_lossy(&output.stdout)))?
}

pub(crate) fn parse_iw_info_details(output: &str) -> Option<(u32, u32, Option<u32>)> {
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
        let channel_width_mhz = rest
            .split_once("width:")
            .and_then(|(_, width)| width.split_whitespace().next())
            .and_then(|width| width.parse::<u32>().ok())
            .filter(|width| *width > 0);
        Some((channel, frequency_mhz, channel_width_mhz))
    })
}

pub(crate) fn parse_iw_link(output: &str) -> IwLinkResult {
    if output.lines().any(|line| line.trim() == "Not connected.") {
        return IwLinkResult::NotAssociated;
    }
    let mut bssid = None;
    let mut ssid = None;
    let mut signal_dbm = None;
    let mut frequency_mhz = None;
    let channel_width_mhz = None;
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
            channel_width_mhz,
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

pub(crate) fn iw_output_failure(stderr: &str) -> FailureKind {
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
