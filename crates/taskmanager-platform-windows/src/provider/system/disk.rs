//! Disk capacity telemetry for the Windows system domain.
//!
//! `sysinfo` supplies the safe disk list, capacity values, and bounded
//! `Disk::usage()` deltas. IOPS, active-time, and response-time remain typed
//! `Unsupported` until a source with those exact semantics is selected; no
//! command interpreter is used.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use taskmanager_core::{DiskMetrics, FailureKind, ScalarObservation, StorageTelemetryObservation};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::StorageTelemetryProvider;

use super::{STORAGE_TELEMETRY_PROVIDER, available_source, unavailable_source};

/// Capacity, throughput, IOPS, active-time, and response-time telemetry for Windows disks.
pub struct WinStorageTelemetryProvider {
    disks: sysinfo::Disks,
    last_refresh: Option<Instant>,
    io_ready: HashSet<String>,
    perf_samples: HashMap<String, (taskmanager_windows_api::WindowsDiskPerformance, Instant)>,
    lifecycles: taskmanager_core::DeviceLifecycleRegistry,
}

impl WinStorageTelemetryProvider {
    pub fn new() -> Self {
        Self {
            disks: sysinfo::Disks::new(),
            last_refresh: None,
            io_ready: HashSet::new(),
            perf_samples: HashMap::new(),
            lifecycles: taskmanager_core::DeviceLifecycleRegistry::new(
                taskmanager_core::DEFAULT_DEVICE_ABSENCE_RETENTION_MS,
            ),
        }
    }
}

impl Default for WinStorageTelemetryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageTelemetryProvider for WinStorageTelemetryProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<StorageTelemetryObservation, ProviderFailure> {
        self.disks.refresh(true);
        self.lifecycles.begin_refresh();
        let now = Instant::now();
        let elapsed_secs = self.last_refresh.and_then(|last| {
            now.checked_duration_since(last)
                .filter(|duration| !duration.is_zero())
                .map(|duration| duration.as_secs_f64())
        });
        self.last_refresh = Some(now);
        let mut metrics = Vec::with_capacity(self.disks.list().len());
        for disk in self.disks.list() {
            let usage = disk.usage();
            let device_id = format!("windows:disk:{}", disk.mount_point().display());
            let device_state = taskmanager_core::DeviceState::healthy(observed_at_ms);
            let lifecycle =
                self.lifecycles
                    .observe(device_id.as_str(), device_state, observed_at_ms);
            let device_generation = lifecycle.generation;
            if usage.total_read_bytes > 0 || usage.total_written_bytes > 0 {
                self.io_ready.insert(device_id.clone());
            }
            let io_ready = self.io_ready.contains(&device_id);
            let mount_point = disk.mount_point().display().to_string();
            let fs_type = disk.file_system().to_string_lossy().into_owned();
            let total_bytes = disk.total_space();
            let available_bytes = disk.available_space();
            let used_bytes = total_bytes.saturating_sub(available_bytes);

            let mut partition = taskmanager_core::DiskPartition::new(mount_point.clone());
            partition.device_id =
                taskmanager_core::DiskPartition::stable_id(&device_id, &mount_point);
            partition.parent_device_id = device_id.clone();
            partition.device_state = taskmanager_core::DeviceState::healthy(observed_at_ms);
            partition.mount_point = mount_point.clone();
            partition.fs_type = fs_type.clone();
            partition.apply_scalar_observations(
                taskmanager_core::DiskPartitionScalarObservations {
                    capacity_bytes: ScalarObservation::available(total_bytes, observed_at_ms),
                    free_bytes: ScalarObservation::available(available_bytes, observed_at_ms),
                    used_bytes: ScalarObservation::available(used_bytes, observed_at_ms),
                },
            );

            let drive_letter = mount_point.trim_end_matches('\\').trim_end_matches('/');
            let native_perf = taskmanager_windows_api::query_disk_performance(drive_letter).ok();
            let native_device = taskmanager_windows_api::query_disk_device_info(drive_letter).ok();
            let native_smart = taskmanager_windows_api::query_disk_smart_info(drive_letter).ok();

            let (iops_obs, active_time_obs, response_time_obs) = if let Some(curr_perf) =
                native_perf
            {
                if let Some((prev_perf, prev_time)) = self.perf_samples.get(&device_id) {
                    let dt = now.saturating_duration_since(*prev_time).as_secs_f64();
                    if dt > 0.0 {
                        let delta_reads = curr_perf.read_count.saturating_sub(prev_perf.read_count);
                        let delta_writes =
                            curr_perf.write_count.saturating_sub(prev_perf.write_count);
                        let total_ops = u64::from(delta_reads) + u64::from(delta_writes);
                        let iops = (total_ops as f64 / dt) as u64;

                        let curr_io_time = curr_perf
                            .read_time_100ns
                            .saturating_add(curr_perf.write_time_100ns);
                        let prev_io_time = prev_perf
                            .read_time_100ns
                            .saturating_add(prev_perf.write_time_100ns);
                        let delta_io_time = curr_io_time.saturating_sub(prev_io_time);
                        let delta_query_time = curr_perf
                            .query_time_100ns
                            .saturating_sub(prev_perf.query_time_100ns);

                        let active_pct = if delta_query_time > 0 {
                            ((delta_io_time as f64 / delta_query_time as f64) * 100.0)
                                .clamp(0.0, 100.0) as f32
                        } else {
                            0.0
                        };

                        let resp_ms = if total_ops > 0 {
                            ((curr_io_time.saturating_sub(prev_io_time) as f64 / 10_000.0)
                                / total_ops as f64) as f32
                        } else {
                            0.0
                        };

                        (
                            ScalarObservation::available(iops, observed_at_ms),
                            ScalarObservation::available(active_pct, observed_at_ms),
                            ScalarObservation::available(resp_ms, observed_at_ms),
                        )
                    } else {
                        (
                            ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                            ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                            ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                        )
                    }
                } else {
                    (
                        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
                    )
                }
            } else {
                (
                    ScalarObservation::unavailable(FailureKind::Unsupported),
                    ScalarObservation::unavailable(FailureKind::Unsupported),
                    ScalarObservation::unavailable(FailureKind::Unsupported),
                )
            };

            if let Some(curr_perf) = native_perf {
                self.perf_samples
                    .insert(device_id.clone(), (curr_perf, now));
            }

            let (model, disk_type, connection, is_removable) = if let Some(ref dev) = native_device
            {
                use taskmanager_core::storage::{
                    StorageConnection, StorageDeviceKind, StorageInterconnect, StorageProtocol,
                };
                use taskmanager_windows_api::{WindowsDiskBusType, WindowsDiskMediaType};

                let (protocol, interconnect) = match dev.bus_type {
                    WindowsDiskBusType::Nvme => (StorageProtocol::Nvme, StorageInterconnect::Pcie),
                    WindowsDiskBusType::Sata => (StorageProtocol::Ata, StorageInterconnect::Sata),
                    WindowsDiskBusType::Usb => (StorageProtocol::Ata, StorageInterconnect::Usb),
                    WindowsDiskBusType::Scsi => {
                        (StorageProtocol::Scsi, StorageInterconnect::Platform)
                    }
                    WindowsDiskBusType::Sas => (StorageProtocol::Scsi, StorageInterconnect::Sas),
                    WindowsDiskBusType::Mmc => (StorageProtocol::Mmc, StorageInterconnect::Mmc),
                    WindowsDiskBusType::Sd => (StorageProtocol::Sd, StorageInterconnect::Sd),
                    WindowsDiskBusType::Virtual => {
                        (StorageProtocol::Other, StorageInterconnect::Virtio)
                    }
                    WindowsDiskBusType::Raid => {
                        (StorageProtocol::Scsi, StorageInterconnect::Platform)
                    }
                    WindowsDiskBusType::Other => {
                        (StorageProtocol::Other, StorageInterconnect::Other)
                    }
                };

                let conn =
                    StorageConnection::new(protocol, interconnect, StorageDeviceKind::Physical);
                let disk_type = match (dev.media_type, dev.bus_type) {
                    (WindowsDiskMediaType::Ssd, WindowsDiskBusType::Nvme) => "NVMe SSD".to_string(),
                    (WindowsDiskMediaType::Ssd, _) => "SSD".to_string(),
                    (WindowsDiskMediaType::Hdd, _) => "HDD".to_string(),
                    _ => disk_kind_label(disk.kind()).to_string(),
                };

                let model = dev
                    .product_id
                    .clone()
                    .unwrap_or_else(|| disk.name().to_string_lossy().into_owned());

                (model, disk_type, conn, dev.is_removable)
            } else {
                (
                    disk.name().to_string_lossy().into_owned(),
                    disk_kind_label(disk.kind()).to_string(),
                    taskmanager_core::storage::StorageConnection::default(),
                    disk.is_removable(),
                )
            };

            let mut row = DiskMetrics::new(disk.name().to_string_lossy().into_owned());
            row.device_id = device_id;
            row.device_generation = device_generation;
            row.device_state = device_state;
            row.model = model;
            row.disk_type = disk_type;
            row.mount_point = mount_point;
            row.fs_type = fs_type;
            row.partitions = vec![partition];
            row.apply_connection(connection);
            row.apply_attachment_capabilities(Some(is_removable), None);
            if let Some(smart) = native_smart {
                row.smart_temperature_c = smart.temperature_c;
                row.smart_percent_used = smart.percentage_used.map(|p| p as f32);
                row.smart_critical_warning = Some(smart.critical_warning != 0);
                row.smart_availability = taskmanager_core::SmartAvailability::Available;
                row.smart_provider = Some(taskmanager_core::ProviderId::borrowed(
                    "windows.storage.smart",
                ));
            }
            row.apply_scalar_observations(taskmanager_core::DiskScalarObservations {
                capacity_bytes: ScalarObservation::available(disk.total_space(), observed_at_ms),
                available_bytes: ScalarObservation::available(
                    disk.available_space(),
                    observed_at_ms,
                ),
                read_bytes_per_sec: rate_observation(
                    usage.read_bytes,
                    elapsed_secs,
                    io_ready,
                    observed_at_ms,
                ),
                write_bytes_per_sec: rate_observation(
                    usage.written_bytes,
                    elapsed_secs,
                    io_ready,
                    observed_at_ms,
                ),
                iops: iops_obs,
                active_time_pct: active_time_obs,
                response_time_ms: response_time_obs,
            });
            metrics.push(row);
        }

        let outcome = if metrics.is_empty() {
            taskmanager_core::DeviceRefreshOutcome::Unavailable(
                taskmanager_core::DeviceStatus::Stale,
            )
        } else {
            taskmanager_core::DeviceRefreshOutcome::Complete
        };
        let _delta = self.lifecycles.finish_refresh(outcome, observed_at_ms);
        let lifecycles = self
            .lifecycles
            .iter()
            .map(|(id, l)| (taskmanager_core::DeviceId::new(id), *l))
            .collect::<std::collections::BTreeMap<_, _>>();

        let sources = if metrics.is_empty() {
            vec![unavailable_source(
                STORAGE_TELEMETRY_PROVIDER,
                FailureKind::TemporarilyUnavailable,
            )]
        } else {
            vec![available_source(STORAGE_TELEMETRY_PROVIDER, metrics.len())]
        };
        Ok(StorageTelemetryObservation::current(
            metrics,
            observed_at_ms,
            sources,
            Vec::new(),
            lifecycles,
        ))
    }
}

/// Convert one real `sysinfo` counter delta to a byte-per-second observation.
/// The first sample has no interval and therefore remains unavailable; an
/// idle interval with a zero delta is a measured zero.
fn rate_observation(
    delta_bytes: u64,
    elapsed_secs: Option<f64>,
    io_ready: bool,
    observed_at_ms: u64,
) -> ScalarObservation<u64> {
    if !io_ready {
        return ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable);
    }
    let Some(elapsed_secs) = elapsed_secs.filter(|seconds| seconds.is_finite() && *seconds > 0.0)
    else {
        return ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable);
    };
    let rate = delta_bytes as f64 / elapsed_secs;
    if !rate.is_finite() || rate < 0.0 {
        return ScalarObservation::unavailable(FailureKind::ProviderFault);
    }
    let rate = if rate >= u64::MAX as f64 {
        u64::MAX
    } else {
        rate as u64
    };
    ScalarObservation::available(rate, observed_at_ms)
}

fn disk_kind_label(kind: sysinfo::DiskKind) -> &'static str {
    match kind {
        sysinfo::DiskKind::HDD => "HDD",
        sysinfo::DiskKind::SSD => "SSD",
        _ => "Unknown",
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_system_disk.rs"]
mod tests;
