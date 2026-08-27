//! macOS storage-domain providers built on bounded `std::process` shell-outs
//! (ADR-019).
//!
//! Filesystem health comes from `sysinfo` (read-only flag). SMART attribute
//! telemetry runs `smartctl --json=c --all` per physical disk discovered by
//! `diskutil list -plist` (safe plist parsing) — the same smartctl pattern
//! the Linux adapter uses, with an honest MissingTool when smartmontools is
//! absent. Self-test start/poll use `smartctl -t` and the selftest log JSON.
//!
//! Per-disk IOPS comes from a single long-running `iostat -d -w 1 -K` child
//! (disk mode, 1 KiB units, 1 s sample interval) spawned once at provider
//! construction and drained by a background reader thread, so the per-refresh
//! path never blocks on a 1 s sample. `iostat` only emits a COMBINED
//! read+write throughput in KiB/s with no read/write split, no busy-time, and
//! no latency — so we honestly project ONLY its transfers-per-second onto
//! `DiskScalarObservations::iops` and leave `read_bytes_per_sec`,
//! `write_bytes_per_sec`, `active_time_pct`, and `response_time_ms` as
//! `Unavailable(Unsupported)` rather than fabricate a split. Disks are
//! matched to `iostat` rows through the `diskutil list` mount-point →
//! whole-disk map. On hosts without `iostat` (Linux CI) the spawn fails at
//! construction and `refresh()` falls back to `Unavailable(MissingDependency)`
//! for IOPS; the sampler child is killed and the reader joined in `Drop`, so
//! no zombie/orphan escapes.

mod iostat;

pub(crate) use iostat::{DiskRates, spawn_iostat_sampler};
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", allow(unused_imports))]
use iostat::{is_iostat_disk_header_line, parse_iostat_data_line, parse_iostat_excerpt};

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use taskmanager_application::{
    DirectoryUsageRequest, SmartControlRequest, SmartObservationRequest, StorageHealthRequest,
};
use taskmanager_core::{
    DeviceState, DeviceStatus, FailureKind, FilesystemHealth, FilesystemHealthSnapshot,
    FilesystemHealthStatus, ProviderId, SmartSelfTestFailure, SmartSelfTestIntent,
    SmartSelfTestKind, SmartSelfTestPhase, SmartSelfTestReport, StorageDeviceTarget,
};
use taskmanager_platform_contract::{
    CompositeSourceSnapshot, ProviderFailure, SourceOutcome, SourceStatus,
};
use taskmanager_platform_provider::{
    DirectoryUsageProvider, FilesystemHealthProvider, SmartSelfTestControlProvider,
    SmartSelfTestObservationProvider,
};
use taskmanager_platform_runtime::{
    ProviderRegistration, StorageExecutors, StorageProviderBindings,
};

use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

const STORAGE_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("macos.system.storage.sysinfo");
const FILESYSTEM_HEALTH_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.storage.filesystem.sysinfo");
const SMART_OBSERVATION_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.storage.smart.observation.smartctl");

/// Filesystem health from `sysinfo`: mount point, fs type and the read-only
/// flag are real. Error counts and integrity state have no safe accessor yet
/// and stay absent (recorded in ADR-019).
pub struct MacFilesystemHealthProvider;

impl MacFilesystemHealthProvider {
    pub fn new() -> Self {
        Self
    }
}

impl FilesystemHealthProvider for MacFilesystemHealthProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<CompositeSourceSnapshot<FilesystemHealthSnapshot>, ProviderFailure> {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let filesystems: Vec<FilesystemHealth> = disks
            .list()
            .iter()
            .map(|disk| {
                let read_only = disk.is_read_only();
                FilesystemHealth {
                    mount_point: PathBuf::from(disk.mount_point()),
                    source: None,
                    fs_type: disk.file_system().to_string_lossy().into_owned(),
                    read_only: Some(read_only),
                    error_count: None,
                    status: if read_only {
                        FilesystemHealthStatus::ReadOnly
                    } else {
                        FilesystemHealthStatus::Healthy
                    },
                    state: DeviceState::healthy(observed_at_ms),
                    integrity_state: DeviceState::default(),
                }
            })
            .collect();
        let outcome = if filesystems.is_empty() {
            SourceOutcome::Unavailable(FailureKind::TemporarilyUnavailable)
        } else {
            SourceOutcome::Available
        };
        Ok(CompositeSourceSnapshot::new(
            FilesystemHealthSnapshot {
                state: DeviceState::healthy(observed_at_ms),
                filesystems: filesystems.clone(),
            },
            vec![SourceStatus {
                provider: FILESYSTEM_HEALTH_PROVIDER,
                outcome,
                item_count: filesystems.len(),
            }],
        ))
    }
}

/// Mount point → physical whole-disk identifier from `diskutil list -plist`
/// (safe plist parsing). APFS volumes map through their physical stores.
fn diskutil_mount_to_whole_disk() -> HashMap<String, String> {
    let mut result = HashMap::new();
    let mut command = std::process::Command::new("diskutil");
    command.args(["list", "-plist"]);
    let Ok(output) = run_with_timeout(&mut command, Duration::from_secs(3)) else {
        return result;
    };
    if !output.status.success() {
        return result;
    }
    let Ok(root) = plist::Value::from_reader(std::io::Cursor::new(&output.stdout)) else {
        return result;
    };
    let Some(dict) = root.as_dictionary() else {
        return result;
    };
    // AllDisksAndPartitions: whole disks with their (non-APFS) partitions.
    if let Some(all) = dict.get("AllDisksAndPartitions").and_then(|v| v.as_array()) {
        for disk in all {
            let Some(disk_dict) = disk.as_dictionary() else {
                continue;
            };
            let Some(whole) = disk_dict
                .get("DeviceIdentifier")
                .and_then(|v| v.as_string())
            else {
                continue;
            };
            if let Some(partitions) = disk_dict.get("Partitions").and_then(|v| v.as_array()) {
                for partition in partitions {
                    if let Some(partition_dict) = partition.as_dictionary()
                        && let Some(mount) =
                            partition_dict.get("MountPoint").and_then(|v| v.as_string())
                    {
                        result.insert(mount.to_string(), whole.to_string());
                    }
                }
            }
        }
    }
    // APFSVolumes: mount points on APFS containers.
    if let Some(volumes) = dict.get("APFSVolumes").and_then(|v| v.as_array()) {
        for volume in volumes {
            let Some(volume_dict) = volume.as_dictionary() else {
                continue;
            };
            let Some(mount) = volume_dict.get("MountPoint").and_then(|v| v.as_string()) else {
                continue;
            };
            let whole = volume_dict
                .get("APFSPhysicalStores")
                .and_then(|v| v.as_array())
                .and_then(|stores| stores.first())
                .and_then(|store| store.as_dictionary())
                .and_then(|store| store.get("DeviceIdentifier"))
                .and_then(|v| v.as_string())
                .map(ToString::to_string);
            if let Some(whole) = whole {
                result.insert(mount.to_string(), whole);
            }
        }
    }
    result
}

fn smartctl_json(device: &str) -> Result<serde_json::Value, ProviderFailure> {
    let mut command = std::process::Command::new("smartctl");
    command.args(["--json=c", "--all", device]);
    match run_with_timeout(&mut command, Duration::from_secs(5)) {
        Ok(output) if smartctl_exit_allows_command(output.status.code()) => {
            serde_json::from_slice(&output.stdout).map_err(|_| ProviderFailure::Rejected)
        }
        Ok(_) => Err(ProviderFailure::Rejected),
        Err(BoundedCommandError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(ProviderFailure::MissingDependency)
        }
        Err(BoundedCommandError::Spawn(error))
            if error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            Err(ProviderFailure::PermissionDenied)
        }
        Err(_) => Err(ProviderFailure::TemporarilyUnavailable),
    }
}

fn smartctl_exit_allows_command(exit_code: Option<i32>) -> bool {
    exit_code.is_some_and(|exit_code| exit_code & 0b111 == 0)
}

/// Disk capacity + SMART telemetry, plus per-disk IOPS from a background
/// `iostat` sampler. Capacity rows come from `sysinfo`; SMART attributes
/// (temperature / percent used / power-on hours / critical warning) come from
/// `smartctl --json=c` per physical disk, with an honest `MissingDependency` when
/// smartmontools is not installed (ADR-019). IOPS comes from the background
/// `iostat` child; read/write bytes-per-second, active-time, and
/// response-time stay `Unavailable(Unsupported)` because `iostat` cannot
/// split combined throughput and exposes no busy-time or latency.
pub struct MacStorageTelemetryProvider {
    /// Latest per-disk sample parsed by the reader thread (`None` until the
    /// first data row arrives, or forever if the sampler is disabled).
    latest_sample: Arc<Mutex<Option<HashMap<String, DiskRates>>>>,
    /// Long-running `iostat` child; killed and reaped on drop. `None` when
    /// the binary could not be spawned (Linux CI) or the child has exited.
    child: Option<Child>,
    /// Reader thread that drains the child's stdout; joined on drop.
    reader: Option<JoinHandle<()>>,
}

impl MacStorageTelemetryProvider {
    pub fn new() -> Self {
        let latest_sample = Arc::new(Mutex::new(None::<HashMap<String, DiskRates>>));
        match spawn_iostat_sampler(latest_sample.clone()) {
            Some((child, reader)) => Self {
                latest_sample,
                child: Some(child),
                reader: Some(reader),
            },
            None => Self {
                latest_sample,
                child: None,
                reader: None,
            },
        }
    }

    /// Reap a finished iostat child so `refresh` does not lie about sampler
    /// activity. If the child has exited (e.g. `iostat` rejected its flags on
    /// a non-macOS host that nevertheless ships a same-named binary), drop it
    /// here; the reader thread will have seen stdout close and exited too.
    fn reap_dead_child(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        if matches!(child.try_wait(), Ok(Some(_))) {
            self.child = None;
        }
    }
}

impl Drop for MacStorageTelemetryProvider {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            _ = child.kill();
            _ = child.wait();
        }
        if let Some(handle) = self.reader.take() {
            _ = handle.join();
        }
    }
}

impl taskmanager_platform_provider::StorageTelemetryProvider for MacStorageTelemetryProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<taskmanager_core::StorageTelemetryObservation, ProviderFailure> {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let whole_disks = diskutil_mount_to_whole_disk();
        // Reap any finished iostat child so we don't claim sampler activity
        // when it is dead, then snapshot the latest per-disk rate sample
        // (cloned under the lock so the reader thread is not blocked for the
        // disk iteration).
        self.reap_dead_child();
        let sampler_active = self.child.is_some();
        let latest_rates = self
            .latest_sample
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or(None);
        let mut metrics = Vec::with_capacity(disks.list().len());
        for disk in disks.list() {
            let mut row =
                taskmanager_core::DiskMetrics::new(disk.name().to_string_lossy().into_owned());
            row.device_id = format!("macos:disk:{}", disk.mount_point().display());
            row.disk_type = disk_kind_label(disk.kind()).to_string();
            row.mount_point = disk.mount_point().display().to_string();
            row.fs_type = disk.file_system().to_string_lossy().into_owned();
            row.apply_attachment_capabilities(Some(disk.is_removable()), None);
            // iostat emits a COMBINED read+write throughput it cannot split, so
            // read/write bytes-per-second stay Unsupported; it also exposes no
            // busy-time or latency, so active-time and response-time stay
            // Unsupported. Only transfers-per-second is projected onto iops.
            // The sampler matches a sysinfo disk to its iostat row through the
            // diskutil mount-point -> whole-disk identifier (disk0, disk1, ...).
            let iops = if sampler_active {
                let whole = whole_disks.get(&row.mount_point);
                match whole.and_then(|id| latest_rates.as_ref().and_then(|map| map.get(id))) {
                    Some(rates) => {
                        taskmanager_core::ScalarObservation::available(rates.iops, observed_at_ms)
                    }
                    None => taskmanager_core::ScalarObservation::unavailable(
                        FailureKind::TemporarilyUnavailable,
                    ),
                }
            } else {
                taskmanager_core::ScalarObservation::unavailable(FailureKind::MissingDependency)
            };
            row.apply_scalar_observations(taskmanager_core::DiskScalarObservations {
                capacity_bytes: taskmanager_core::ScalarObservation::available(
                    disk.total_space(),
                    observed_at_ms,
                ),
                available_bytes: taskmanager_core::ScalarObservation::available(
                    disk.available_space(),
                    observed_at_ms,
                ),
                read_bytes_per_sec: taskmanager_core::ScalarObservation::unavailable(
                    FailureKind::Unsupported,
                ),
                write_bytes_per_sec: taskmanager_core::ScalarObservation::unavailable(
                    FailureKind::Unsupported,
                ),
                iops,
                active_time_pct: taskmanager_core::ScalarObservation::unavailable(
                    FailureKind::Unsupported,
                ),
                response_time_ms: taskmanager_core::ScalarObservation::unavailable(
                    FailureKind::Unsupported,
                ),
            });
            metrics.push(row);
        }

        // SMART enrichment: for each mounted volume, smartctl its whole disk.
        let mut smart_failures = 0usize;
        let mut smart_rows = 0usize;
        for row in &mut metrics {
            let Some(whole) = whole_disks.get(&row.mount_point) else {
                continue;
            };
            let device = format!("/dev/{whole}");
            match smartctl_json(&device) {
                Ok(json) => {
                    apply_smart_json(row, &json, observed_at_ms);
                    smart_rows += 1;
                }
                Err(ProviderFailure::MissingDependency) => {
                    row.smart_availability =
                        taskmanager_core::metrics::SmartAvailability::MissingTool;
                    smart_failures += 1;
                }
                Err(ProviderFailure::PermissionDenied) => {
                    row.smart_availability =
                        taskmanager_core::metrics::SmartAvailability::PermissionDenied;
                    smart_failures += 1;
                }
                Err(_) => {
                    smart_failures += 1;
                }
            }
        }

        let mut sources = Vec::new();
        if metrics.is_empty() {
            sources.push(unavailable_source(
                STORAGE_TELEMETRY_PROVIDER,
                FailureKind::TemporarilyUnavailable,
            ));
        } else {
            sources.push(available_source(STORAGE_TELEMETRY_PROVIDER, metrics.len()));
        }
        if smart_rows > 0 {
            sources.push(available_source(SMART_OBSERVATION_PROVIDER, smart_rows));
        } else if smart_failures > 0 {
            sources.push(unavailable_source(
                SMART_OBSERVATION_PROVIDER,
                FailureKind::MissingDependency,
            ));
        }
        Ok(taskmanager_core::StorageTelemetryObservation::current(
            metrics,
            observed_at_ms,
            sources,
            Vec::new(),
            Default::default(),
        ))
    }
}

fn disk_kind_label(kind: sysinfo::DiskKind) -> &'static str {
    match kind {
        sysinfo::DiskKind::HDD => "HDD",
        sysinfo::DiskKind::SSD => "SSD",
        _ => "Unknown",
    }
}

fn available_source(provider: ProviderId, item_count: usize) -> SourceStatus {
    SourceStatus {
        provider,
        outcome: SourceOutcome::Available,
        item_count,
    }
}

fn unavailable_source(provider: ProviderId, failure: FailureKind) -> SourceStatus {
    SourceStatus {
        provider,
        outcome: SourceOutcome::Unavailable(failure),
        item_count: 0,
    }
}

/// Map smartctl JSON onto the DiskMetrics smart_* projections. ATA and NVMe
/// schemas differ; missing sections stay absent, never fabricated.
fn apply_smart_json(
    row: &mut taskmanager_core::DiskMetrics,
    json: &serde_json::Value,
    observed_at_ms: u64,
) {
    row.smart_availability = taskmanager_core::metrics::SmartAvailability::Available;
    row.smart_state = DeviceState::healthy(observed_at_ms);
    row.smart_provider = Some(SMART_OBSERVATION_PROVIDER);
    row.smart_temperature_c = json
        .pointer("/temperature/current")
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32);
    row.smart_power_on_hours = json
        .pointer("/power_on_time/hours")
        .and_then(serde_json::Value::as_u64);
    row.smart_percent_used = json
        .pointer("/nvme_smart_health_information_log/percentage_used")
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32);
    let critical_warning = json
        .pointer("/nvme_smart_health_information_log/critical_warning")
        .and_then(serde_json::Value::as_u64);
    if let Some(warning) = critical_warning {
        row.smart_critical_warning = Some(warning != 0);
    }
    row.smart_temp_critical_c = json
        .pointer("/temperature/drive_trip")
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32);
}

fn smartctl_token(kind: SmartSelfTestKind) -> &'static str {
    match kind {
        SmartSelfTestKind::Short => "short",
        SmartSelfTestKind::Extended => "long",
        SmartSelfTestKind::Conveyance => "conveyance",
    }
}

/// Start a SMART self-test through `smartctl -t <kind> <device>`.
pub struct MacSmartSelfTestControlProvider;

impl SmartSelfTestControlProvider for MacSmartSelfTestControlProvider {
    fn start(
        &mut self,
        intent: &SmartSelfTestIntent,
        _observed_at_ms: u64,
    ) -> Result<SmartSelfTestReport, ProviderFailure> {
        let device = intent.device_key.as_str();
        let mut command = std::process::Command::new("smartctl");
        command.args(["-t", smartctl_token(intent.kind), device]);
        match run_with_timeout(&mut command, Duration::from_secs(5)) {
            Ok(output) if smartctl_exit_allows_command(output.status.code()) => {
                Ok(SmartSelfTestReport {
                    state: DeviceState::healthy(intent.device_generation.get()),
                    phase: SmartSelfTestPhase::Running,
                    kind: Some(intent.kind),
                    progress_pct: Some(0.0),
                    ..SmartSelfTestReport::default()
                })
            }
            Ok(_) => Err(ProviderFailure::Rejected),
            Err(BoundedCommandError::Spawn(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                Err(ProviderFailure::MissingDependency)
            }
            Err(_) => Err(ProviderFailure::TemporarilyUnavailable),
        }
    }
}

/// Poll SMART self-test state through `smartctl --json=c --all` (simplified
/// single-attempt mirror of the Linux strategy machine).
pub struct MacSmartSelfTestObservationProvider;

impl SmartSelfTestObservationProvider for MacSmartSelfTestObservationProvider {
    fn refresh(
        &mut self,
        target: &StorageDeviceTarget,
        previous: DeviceState,
        observed_at_ms: u64,
    ) -> Result<SmartSelfTestReport, ProviderFailure> {
        let json = smartctl_json(target.locator.as_str())?;
        let status_text = json
            .pointer("/ata_smart_data/self_test/status/string")
            .or_else(|| json.pointer("/nvme_self_test_log/current_self_test_operation/string"))
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);
        let progress_pct = json
            .pointer("/ata_smart_data/self_test/status/remaining_percent")
            .or_else(|| json.pointer("/nvme_self_test_log/current_self_test_completion_percent"))
            .and_then(serde_json::Value::as_f64)
            .map(|value| {
                if status_text
                    .as_deref()
                    .is_some_and(|status| status.to_ascii_lowercase().contains("remaining"))
                {
                    100.0 - value
                } else {
                    value
                }
            })
            .filter(|value| (0.0..=100.0).contains(value))
            .map(|value| value as f32);
        let phase = status_text
            .as_deref()
            .map(parse_phase)
            .unwrap_or(SmartSelfTestPhase::Idle);
        let table = json
            .pointer("/ata_smart_self_test_log/standard/table")
            .or_else(|| json.pointer("/nvme_self_test_log/table"))
            .and_then(serde_json::Value::as_array);
        let latest = table.and_then(|entries| entries.first());
        let lifetime_hours = latest
            .and_then(|entry| entry.get("lifetime_hours"))
            .and_then(serde_json::Value::as_u64);
        let first_error_lba = latest
            .and_then(|entry| entry.get("lba_of_first_error"))
            .and_then(serde_json::Value::as_u64);
        let mut report = SmartSelfTestReport {
            state: previous.transition(DeviceStatus::Healthy, observed_at_ms),
            phase,
            kind: None,
            progress_pct,
            lifetime_hours,
            first_error_lba,
            failure: None,
        };
        if status_text.is_none() && table.is_none() {
            report.failure = Some(SmartSelfTestFailure::ProviderUnavailable);
        }
        Ok(report)
    }
}

/// Directory-usage scans are implemented in [`crate::provider::directory_usage`]
/// (pure safe `std::fs`, chunked, cancellable, symlink-safe; ADR-019 route-C).
/// The type is re-exported here so the existing storage composition and the
/// `pub use storage::{..., MacDirectoryUsageProvider, ...}` facade stay stable.
pub use crate::provider::directory_usage::MacDirectoryUsageProvider;

fn parse_phase(status: &str) -> SmartSelfTestPhase {
    let lower = status.to_ascii_lowercase();
    if lower.contains("completed") {
        SmartSelfTestPhase::Completed
    } else if lower.contains("aborted") {
        SmartSelfTestPhase::Aborted
    } else if lower.contains("failed") {
        SmartSelfTestPhase::Failed
    } else if lower.contains("remaining") || lower.contains("in progress") {
        SmartSelfTestPhase::Running
    } else {
        SmartSelfTestPhase::Unknown
    }
}

pub struct MacStorageProviders {
    filesystems: ProviderRegistration<StorageHealthRequest, Box<dyn FilesystemHealthProvider>>,
    smart_observation:
        ProviderRegistration<SmartObservationRequest, Box<dyn SmartSelfTestObservationProvider>>,
    smart_control: ProviderRegistration<SmartControlRequest, Box<dyn SmartSelfTestControlProvider>>,
    directory_usage: ProviderRegistration<DirectoryUsageRequest, Box<dyn DirectoryUsageProvider>>,
}

impl MacStorageProviders {
    #[must_use]
    pub fn new<F, O, C, D>(
        filesystems: ProviderRegistration<StorageHealthRequest, F>,
        smart_observation: ProviderRegistration<SmartObservationRequest, O>,
        smart_control: ProviderRegistration<SmartControlRequest, C>,
        directory_usage: ProviderRegistration<DirectoryUsageRequest, D>,
    ) -> Self
    where
        F: FilesystemHealthProvider,
        O: SmartSelfTestObservationProvider,
        C: SmartSelfTestControlProvider,
        D: DirectoryUsageProvider,
    {
        Self {
            filesystems: filesystems
                .map_provider(|provider| Box::new(provider) as Box<dyn FilesystemHealthProvider>),
            smart_observation: smart_observation.map_provider(|provider| {
                Box::new(provider) as Box<dyn SmartSelfTestObservationProvider>
            }),
            smart_control: smart_control.map_provider(|provider| {
                Box::new(provider) as Box<dyn SmartSelfTestControlProvider>
            }),
            directory_usage: directory_usage
                .map_provider(|provider| Box::new(provider) as Box<dyn DirectoryUsageProvider>),
        }
    }

    pub(crate) fn runtime_bindings(&self) -> StorageProviderBindings {
        StorageProviderBindings::from_registrations(
            &self.filesystems,
            &self.smart_observation,
            &self.smart_control,
        )
        .with_directory_usage(&self.directory_usage)
    }

    pub(crate) fn into_runtime(self) -> StorageExecutors {
        let Self {
            filesystems,
            smart_observation,
            smart_control,
            directory_usage,
        } = self;
        let mut filesystems = filesystems.into_provider();
        let mut smart_observation = smart_observation.into_provider();
        let mut smart_control = smart_control.into_provider();
        let mut directory_usage = directory_usage.into_provider();
        StorageExecutors::new(
            move |observed_at_ms| filesystems.refresh(observed_at_ms),
            move |target, previous, observed_at_ms| {
                smart_observation.refresh(target, previous, observed_at_ms)
            },
            move |intent, observed_at_ms| smart_control.start(intent, observed_at_ms),
        )
        .with_directory_usage(move |spec, control, observed_at_ms| {
            directory_usage.scan_chunk(spec, control, observed_at_ms)
        })
    }
}

#[cfg(test)]
#[path = "../../tests/headless/macos_provider_storage.rs"]
mod tests;
