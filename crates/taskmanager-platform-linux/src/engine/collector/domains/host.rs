//! Linux host-runtime telemetry: uptime, process count, and thread count from `/proc`.
//!
//! `LinuxHostTelemetryCollector` runs the host domain independently of the CPU
//! and process-list lanes and retains the last good facts when a scan degrades
//! to partial or stale.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use taskmanager_core::{
    FailureKind, HostRuntimeFacts, HostRuntimeObservation, ProviderId, ScalarObservation,
    SourceOutcome, SourceStatus, SystemLoadAverage,
};

use super::{LinuxSystemDomainCollector, SourceQuality, source_quality, stronger_failure};

const UPTIME_PROVIDER: ProviderId = ProviderId::borrowed("linux.host.proc-uptime");
const PROCESS_PROVIDER: ProviderId = ProviderId::borrowed("linux.host.proc-processes");
const THREAD_PROVIDER: ProviderId = ProviderId::borrowed("linux.host.proc-threads");
const LOAD_PROVIDER: ProviderId = ProviderId::borrowed("linux.host.proc-loadavg");

/// Host runtime facts collected independently from CPU and process-list lanes.
pub(crate) struct LinuxHostTelemetryCollector {
    proc_root: PathBuf,
    logical_processors: usize,
    physical_processors: Option<usize>,
    last_facts: HostRuntimeFacts,
    last_value: Option<(HostRuntimeFacts, u64)>,
}

impl LinuxHostTelemetryCollector {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::with_proc_root(PathBuf::from("/proc"))
    }

    fn with_proc_root(proc_root: PathBuf) -> Self {
        Self::with_proc_root_and_processor_counts(
            proc_root,
            logical_processor_count(),
            physical_processor_count(),
        )
    }

    fn with_proc_root_and_processor_counts(
        proc_root: PathBuf,
        logical_processors: usize,
        physical_processors: Option<usize>,
    ) -> Self {
        Self {
            proc_root,
            logical_processors,
            physical_processors,
            last_facts: HostRuntimeFacts::default(),
            last_value: None,
        }
    }

    pub(crate) fn observe(&mut self, now_ms: u64) -> HostRuntimeObservation {
        <Self as LinuxSystemDomainCollector>::observe(self, Instant::now(), now_ms)
    }

    fn observe_facts(&mut self, now_ms: u64) -> (HostRuntimeFacts, Vec<SourceStatus>) {
        let uptime = observe_uptime(&self.proc_root.join("uptime"), now_ms);
        let process_scan = observe_processes(&self.proc_root, now_ms);
        let psi = super::psi::observe_psi(&self.proc_root.join("pressure"), now_ms);
        let load_average = observe_load_average(
            &self.proc_root.join("loadavg"),
            self.logical_processors,
            self.physical_processors,
            now_ms,
        );
        let facts = HostRuntimeFacts {
            uptime_secs: uptime.scalar.retain_previous(self.last_facts.uptime_secs),
            processes: process_scan
                .processes
                .scalar
                .retain_previous(self.last_facts.processes),
            threads: process_scan
                .threads
                .scalar
                .retain_previous(self.last_facts.threads),
            pressure: psi.snapshot.retain_previous(self.last_facts.pressure),
            load_average: load_average
                .scalar
                .retain_previous(self.last_facts.load_average),
        };
        self.last_facts = facts.clone();
        (
            facts,
            vec![
                uptime.source,
                process_scan.processes.source,
                process_scan.threads.source,
                psi.source,
                load_average.source,
            ],
        )
    }
}

impl Default for LinuxHostTelemetryCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSystemDomainCollector for LinuxHostTelemetryCollector {
    type Observation = HostRuntimeObservation;

    fn observe(&mut self, _now: Instant, now_ms: u64) -> Self::Observation {
        let (facts, sources) = self.observe_facts(now_ms);
        match source_quality(&sources) {
            SourceQuality::Current => {
                self.last_value = Some((facts.clone(), now_ms));
                HostRuntimeObservation::current(facts, now_ms, sources)
            }
            SourceQuality::Partial(failure) => {
                self.last_value = Some((facts.clone(), now_ms));
                HostRuntimeObservation::partial(facts, now_ms, failure, sources)
            }
            SourceQuality::Unavailable(failure) => self.last_value.as_ref().map_or_else(
                || HostRuntimeObservation::unavailable(failure, sources.clone()),
                |(last_value, last_success_ms)| {
                    HostRuntimeObservation::stale(
                        last_value.clone(),
                        *last_success_ms,
                        failure,
                        sources.clone(),
                    )
                },
            ),
        }
    }
}

struct ScalarSource<T> {
    scalar: ScalarObservation<T>,
    source: SourceStatus,
}

struct ProcessScan {
    processes: ScalarSource<u64>,
    threads: ScalarSource<u64>,
}

fn logical_processor_count() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
}

fn physical_processor_count() -> Option<usize> {
    let root = Path::new("/sys/devices/system/cpu");
    let mut identities = std::collections::BTreeSet::new();
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(cpu) = name
            .strip_prefix("cpu")
            .and_then(|value| value.parse::<usize>().ok())
        else {
            continue;
        };
        let topology = root.join(format!("cpu{cpu}/topology"));
        let Some(package) = fs::read_to_string(topology.join("physical_package_id"))
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
        else {
            continue;
        };
        let Some(core) = fs::read_to_string(topology.join("core_id"))
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
        else {
            continue;
        };
        identities.insert((package, core));
    }
    (!identities.is_empty()).then_some(identities.len())
}

fn observe_uptime(path: &Path, now_ms: u64) -> ScalarSource<u64> {
    let observation = fs::read_to_string(path)
        .map_err(|error| io_failure(&error))
        .and_then(|content| parse_uptime_secs(&content));
    match observation {
        Ok(value) => ScalarSource {
            scalar: ScalarObservation::available(value, now_ms),
            source: SourceStatus {
                provider: UPTIME_PROVIDER,
                outcome: SourceOutcome::Available,
                item_count: 1,
            },
        },
        Err(failure) => ScalarSource {
            scalar: ScalarObservation::unavailable(failure),
            source: SourceStatus {
                provider: UPTIME_PROVIDER,
                outcome: SourceOutcome::Unavailable(failure),
                item_count: 0,
            },
        },
    }
}

fn parse_uptime_secs(content: &str) -> Result<u64, FailureKind> {
    let token = content
        .split_whitespace()
        .next()
        .ok_or(FailureKind::ProviderFault)?;
    let whole_seconds = token.split_once('.').map_or(token, |(whole, _)| whole);
    whole_seconds
        .parse::<u64>()
        .map_err(|_| FailureKind::ProviderFault)
}

fn observe_load_average(
    path: &Path,
    logical_processors: usize,
    physical_processors: Option<usize>,
    now_ms: u64,
) -> ScalarSource<SystemLoadAverage> {
    let observation = fs::read_to_string(path)
        .map_err(|error| io_failure(&error))
        .and_then(|content| parse_load_average(&content, logical_processors, physical_processors));
    match observation {
        Ok(value) => ScalarSource {
            scalar: ScalarObservation::available(value, now_ms),
            source: SourceStatus {
                provider: LOAD_PROVIDER,
                outcome: SourceOutcome::Available,
                item_count: 1,
            },
        },
        Err(failure) => ScalarSource {
            scalar: ScalarObservation::unavailable(failure),
            source: SourceStatus {
                provider: LOAD_PROVIDER,
                outcome: SourceOutcome::Unavailable(failure),
                item_count: 0,
            },
        },
    }
}

fn parse_load_average(
    content: &str,
    logical_processors: usize,
    physical_processors: Option<usize>,
) -> Result<SystemLoadAverage, FailureKind> {
    let mut fields = content.split_whitespace();
    let one_minute = fields
        .next()
        .ok_or(FailureKind::ProviderFault)?
        .parse::<f32>()
        .map_err(|_| FailureKind::ProviderFault)?;
    let five_minutes = fields
        .next()
        .ok_or(FailureKind::ProviderFault)?
        .parse::<f32>()
        .map_err(|_| FailureKind::ProviderFault)?;
    let fifteen_minutes = fields
        .next()
        .ok_or(FailureKind::ProviderFault)?
        .parse::<f32>()
        .map_err(|_| FailureKind::ProviderFault)?;
    SystemLoadAverage::from_raw_with_physical(
        one_minute,
        five_minutes,
        fifteen_minutes,
        logical_processors,
        physical_processors,
    )
    .ok_or(FailureKind::ProviderFault)
}

fn observe_processes(proc_root: &Path, now_ms: u64) -> ProcessScan {
    let entries = match fs::read_dir(proc_root) {
        Ok(entries) => entries,
        Err(error) => {
            let failure = io_failure(&error);
            return ProcessScan {
                processes: unavailable_scalar(PROCESS_PROVIDER, failure),
                threads: unavailable_scalar(THREAD_PROVIDER, failure),
            };
        }
    };
    let mut processes = 0_u64;
    let mut threads = 0_u64;
    let mut process_failure = None;
    let mut thread_failure = None;
    let mut thread_samples = 0_u64;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let failure = io_failure(&error);
                process_failure = Some(stronger_failure(process_failure, failure));
                thread_failure = Some(stronger_failure(thread_failure, failure));
                continue;
            }
        };
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            process_failure = Some(stronger_failure(
                process_failure,
                FailureKind::ProviderFault,
            ));
            continue;
        };
        if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        processes = processes.saturating_add(1);
        match fs::read_to_string(entry.path().join("stat"))
            .map_err(|error| io_failure(&error))
            .and_then(|content| parse_num_threads(&content))
        {
            Ok(count) => {
                threads = threads.saturating_add(count);
                thread_samples = thread_samples.saturating_add(1);
            }
            Err(failure) => {
                thread_failure = Some(stronger_failure(thread_failure, failure));
            }
        }
    }

    ProcessScan {
        processes: observed_count(
            PROCESS_PROVIDER,
            processes,
            processes,
            process_failure,
            now_ms,
        ),
        threads: observed_count(
            THREAD_PROVIDER,
            threads,
            thread_samples,
            thread_failure,
            now_ms,
        ),
    }
}

fn parse_num_threads(stat: &str) -> Result<u64, FailureKind> {
    let close = stat.rfind(')').ok_or(FailureKind::ProviderFault)?;
    let value = stat[close.saturating_add(1)..]
        .split_whitespace()
        .nth(17)
        .ok_or(FailureKind::ProviderFault)?;
    value.parse::<u64>().map_err(|_| FailureKind::ProviderFault)
}

fn observed_count(
    provider: ProviderId,
    value: u64,
    successful_samples: u64,
    failure: Option<FailureKind>,
    now_ms: u64,
) -> ScalarSource<u64> {
    let item_count = usize::try_from(successful_samples).unwrap_or(usize::MAX);
    match (successful_samples, failure) {
        (_, None) => ScalarSource {
            scalar: ScalarObservation::available(value, now_ms),
            source: SourceStatus {
                provider,
                outcome: if successful_samples == 0 {
                    SourceOutcome::Empty
                } else {
                    SourceOutcome::Available
                },
                item_count,
            },
        },
        (0, Some(failure)) => unavailable_scalar(provider, failure),
        (_, Some(failure)) => ScalarSource {
            scalar: ScalarObservation::partial(value, now_ms, failure),
            source: SourceStatus {
                provider,
                outcome: SourceOutcome::Partial(failure),
                item_count,
            },
        },
    }
}

fn unavailable_scalar<T>(provider: ProviderId, failure: FailureKind) -> ScalarSource<T> {
    ScalarSource {
        scalar: ScalarObservation::unavailable(failure),
        source: SourceStatus {
            provider,
            outcome: SourceOutcome::Unavailable(failure),
            item_count: 0,
        },
    }
}

fn io_failure(error: &io::Error) -> FailureKind {
    match error.kind() {
        io::ErrorKind::NotFound => FailureKind::TemporarilyUnavailable,
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        io::ErrorKind::TimedOut => FailureKind::TimedOut,
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted => {
            FailureKind::TemporarilyUnavailable
        }
        _ => FailureKind::ProviderFault,
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_collector_domains_host_tests.rs"]
mod tests;
