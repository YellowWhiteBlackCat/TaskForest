//! Linux/procfs/sysfs source adapters used by the collector.

use super::*;

const DISKSTATS_PROVIDER: ProviderId = ProviderId::borrowed("linux.storage.proc.diskstats");

pub(super) struct DiskstatsObservation {
    stats: HashMap<String, DiskStatsState>,
    failed_devices: HashMap<String, FailureKind>,
    pub(super) source: SourceStatus,
}

impl std::ops::Deref for DiskstatsObservation {
    type Target = HashMap<String, DiskStatsState>;

    fn deref(&self) -> &Self::Target {
        &self.stats
    }
}

impl DiskstatsObservation {
    pub(super) fn failure_for(&self, device_name: &str) -> FailureKind {
        if let Some(failure) = self.failed_devices.get(device_name) {
            return *failure;
        }
        match self.source.outcome {
            SourceOutcome::Unavailable(failure) => failure,
            SourceOutcome::Available | SourceOutcome::Empty | SourceOutcome::Partial(_) => {
                FailureKind::TemporarilyUnavailable
            }
        }
    }
}

/// Parse `/proc/meminfo` text into a `key -> bytes` map. The CPU/memory
/// provenance probe owns the I/O result and delegates successful content here
/// so read failure cannot be confused with an authoritative all-zero sample.
/// Values are kB in the file → multiplied by 1024 to bytes; any line whose
/// value token isn't a u64, or whose kB→bytes conversion would overflow, is
/// silently dropped so the consumer sees typed absence for that key.
pub(super) fn parse_meminfo_lines(text: &str) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(key), Some(val_str)) = (parts.next(), parts.next()) {
            let key_clean = key.trim_end_matches(':');
            if let Some(bytes) = val_str
                .parse::<u64>()
                .ok()
                .and_then(|kib| kib.checked_mul(1024))
            {
                map.insert(key_clean.to_string(), bytes); // Convert kB to Bytes
            }
        }
    }
    map
}

#[cfg(target_os = "linux")]
pub(super) fn parse_proc_diskstats() -> DiskstatsObservation {
    read_proc_diskstats_from(Path::new("/proc/diskstats"))
}

#[cfg(any(target_os = "linux", test))]
fn read_proc_diskstats_from(path: &Path) -> DiskstatsObservation {
    match fs::read_to_string(path) {
        Ok(content) => parse_diskstats_observation(&content),
        Err(error) => DiskstatsObservation {
            stats: HashMap::new(),
            failed_devices: HashMap::new(),
            source: SourceStatus {
                provider: DISKSTATS_PROVIDER,
                outcome: SourceOutcome::Unavailable(diskstats_io_failure(&error)),
                item_count: 0,
            },
        },
    }
}

/// Parse `/proc/diskstats` text into one typed source observation.
///
/// Lines with fewer than 14 whitespace fields (headers, partition stubs,
/// truncated rows) are dropped. A malformed numeric counter drops the whole
/// row rather than inventing a zero-valued counter.
pub(super) fn parse_diskstats_observation(text: &str) -> DiskstatsObservation {
    let mut map = HashMap::new();
    let mut failed_devices = HashMap::new();
    let mut malformed_rows = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let parsed = (|| {
            if parts.len() < 14
                || parts[2].is_empty()
                || parts[0].parse::<u64>().is_err()
                || parts[1].parse::<u64>().is_err()
            {
                return None;
            }
            Some((
                parts[2].to_string(),
                DiskStatsState {
                    reads_completed: parts[3].parse::<u64>().ok()?,
                    sectors_read: parts[5].parse::<u64>().ok()?,
                    writes_completed: parts[7].parse::<u64>().ok()?,
                    sectors_written: parts[9].parse::<u64>().ok()?,
                    io_time_ms: parts[12].parse::<u64>().ok()?,
                    weighted_time_ms: parts[13].parse::<u64>().ok()?,
                    timestamp: None,
                },
            ))
        })();
        let Some((device_name, state)) = parsed else {
            malformed_rows = malformed_rows.saturating_add(1);
            if let Some(device_name) = parts.get(2).filter(|name| !name.is_empty()) {
                map.remove(*device_name);
                failed_devices.insert((*device_name).to_owned(), FailureKind::ProviderFault);
            }
            continue;
        };
        if failed_devices.contains_key(&device_name)
            || map.insert(device_name.clone(), state).is_some()
        {
            map.remove(&device_name);
            failed_devices.insert(device_name, FailureKind::ProviderFault);
            malformed_rows = malformed_rows.saturating_add(1);
        }
    }
    let item_count = map.len();
    let outcome = match (item_count, malformed_rows) {
        (0, 0) => SourceOutcome::Empty,
        (_, 0) => SourceOutcome::Available,
        (0, _) => SourceOutcome::Unavailable(FailureKind::ProviderFault),
        (_, _) => SourceOutcome::Partial(FailureKind::ProviderFault),
    };
    DiskstatsObservation {
        stats: map,
        failed_devices,
        source: SourceStatus {
            provider: DISKSTATS_PROVIDER,
            outcome,
            item_count,
        },
    }
}

fn diskstats_io_failure(error: &std::io::Error) -> FailureKind {
    match error.kind() {
        std::io::ErrorKind::NotFound => FailureKind::Unsupported,
        std::io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        std::io::ErrorKind::TimedOut => FailureKind::TimedOut,
        _ => FailureKind::ProviderFault,
    }
}

/// Copy parsed NVMe or ATA/SATA SMART fields onto a disk's `DiskMetrics`. Keeps
/// the collector loop body free of field-by-field plumbing.
pub(super) fn apply_smart(d: &mut DiskMetrics, s: &taskmanager_core::core::smart::DiskSmart) {
    d.smart_availability = s.availability;
    d.smart_state = s.state;
    d.smart_provider.clone_from(&s.provider);
    d.smart_failure = s.failure;
    d.smart_temperature_c = s.temperature_c;
    d.smart_critical_warning = s.critical_warning;
    d.smart_temp_critical_c = s.temp_critical_c;
    d.smart_percent_used = s.percent_used;
    d.smart_power_on_hours = s.power_on_hours;
}

// ── macOS / Windows stubs ────────────────────────────────────────────────────
// `/proc/diskstats` is Linux-only. CPU/memory/network provenance keeps its own
// typed source observations.
#[cfg(not(target_os = "linux"))]
pub(super) fn parse_proc_diskstats() -> DiskstatsObservation {
    DiskstatsObservation {
        stats: HashMap::new(),
        failed_devices: HashMap::new(),
        source: SourceStatus {
            provider: DISKSTATS_PROVIDER,
            outcome: SourceOutcome::Unavailable(FailureKind::Unsupported),
            item_count: 0,
        },
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_collector_sources_diskstats_source_tests.rs"]
mod diskstats_source_tests;
