//! Stateful storage-domain assembly over independently fallible enrichments.

use std::collections::HashMap;
use std::time::Instant;

use sysinfo::Disks;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::{DiskMetrics, ScalarObservation};
use taskmanager_core::core::source::SourceOutcome;
use taskmanager_platform_contract::DeviceSourceSnapshot;

use super::super::sources::{DiskstatsObservation, apply_smart, parse_proc_diskstats};
use super::super::{DiskCollectionState, DiskStatsState};
use super::inventory::{collect_storage_inventory, reconcile_storage_snapshot_identity};
use crate::engine::smart;

/// Collect one storage domain refresh with sysfs as its sole discovery source.
///
/// Capacity is read once by the sysfs metadata enrichment. Diskstats and SMART
/// consume that inventory and append their own source statuses; neither can
/// manufacture devices or weaken a successful absence decision.
pub(crate) fn collect_storage_domain(
    disks: &Disks,
    sysfs_root: &std::path::Path,
    state: &mut DiskCollectionState,
    tick: u32,
    now: Instant,
    now_ms: u64,
) -> DeviceSourceSnapshot<Vec<DiskMetrics>> {
    let mut snapshot = collect_storage_inventory(disks, sysfs_root, now_ms);
    let previous_identities = snapshot
        .value
        .iter()
        .filter_map(|disk| {
            let name = disk.name.trim_start_matches("/dev/").to_string();
            state
                .identity_cache
                .get(&name)
                .cloned()
                .map(|identity| (name, identity))
        })
        .collect::<HashMap<_, _>>();
    reconcile_storage_snapshot_identity(&mut snapshot, &mut state.identity_cache);
    reset_changed_identity_rate_baselines(&snapshot.value, &previous_identities, &mut state.stats);
    reset_rate_baselines_after_discovery(snapshot.discovery().outcome, &mut state.stats);

    let diskstats = parse_proc_diskstats();
    apply_diskstats_rates(
        &mut snapshot.value,
        &diskstats,
        &mut state.stats,
        now,
        now_ms,
    );
    snapshot.enrichments.push(diskstats.source.clone());

    let scheduled_smart_refresh = tick == 1 || tick.is_multiple_of(5);
    let mut smart_sources = Vec::with_capacity(snapshot.value.len());
    for disk in &mut snapshot.value {
        let physical_name = disk.name.trim_start_matches("/dev/");
        let needs_refresh =
            scheduled_smart_refresh || !state.smart_cache.contains_key(&disk.device_id);
        if needs_refresh {
            let previous = state
                .smart_cache
                .get(&disk.device_id)
                .map(|cached| cached.state)
                .unwrap_or_default();
            let observation = state
                .smart_providers
                .observe(physical_name, disk.connection());
            let mut smart_value = observation.value;
            taskmanager_core::core::smart::refresh_state(previous, &mut smart_value, now_ms);
            apply_smart(disk, &smart_value);
            state
                .smart_source_cache
                .insert(disk.device_id.clone(), observation.source);
            state
                .smart_cache
                .insert(disk.device_id.clone(), smart_value);
        } else if let Some(cached) = state.smart_cache.get(&disk.device_id) {
            apply_smart(disk, cached);
        }
        if let Some(source) = state.smart_source_cache.get(&disk.device_id) {
            smart_sources.push(source.clone());
        }
    }
    snapshot
        .enrichments
        .extend(smart::provider::aggregate_smart_sources(smart_sources));
    snapshot
        .enrichments
        .sort_by(|left, right| left.provider.cmp(&right.provider));
    snapshot
}

fn reset_rate_baselines_after_discovery(
    outcome: SourceOutcome,
    stats: &mut HashMap<String, DiskStatsState>,
) {
    if !matches!(outcome, SourceOutcome::Available | SourceOutcome::Empty) {
        // Discovery could not prove which native names still belong to the
        // same attachment set. Retain last-known scalar truth later, but force
        // every recovered counter stream to establish a fresh baseline.
        stats.clear();
    }
}

fn reset_changed_identity_rate_baselines(
    metrics: &[DiskMetrics],
    previous_identities: &HashMap<String, DeviceId>,
    stats: &mut HashMap<String, DiskStatsState>,
) {
    for disk in metrics {
        let name = disk.name.trim_start_matches("/dev/");
        if previous_identities
            .get(name)
            .is_some_and(|previous| previous.as_str() != disk.device_id)
        {
            // The same kernel slot now represents a different stable device.
            // Never derive its first diskstats rate from the old generation.
            stats.remove(name);
        }
    }
}

fn apply_diskstats_rates(
    metrics: &mut [DiskMetrics],
    current: &DiskstatsObservation,
    previous: &mut HashMap<String, DiskStatsState>,
    now: Instant,
    now_ms: u64,
) {
    for disk in metrics {
        let physical_name = disk.name.trim_start_matches("/dev/");
        let Some(current) = current.get(physical_name) else {
            // A failed interval cannot remain part of a later rate divisor.
            // Retained scalar truth is merged after lifecycle generation is
            // assigned; the raw counter baseline must restart.
            previous.remove(physical_name);
            apply_rate_failure(disk, current.failure_for(physical_name));
            continue;
        };
        let observed = previous
            .get(physical_name)
            .and_then(|baseline| baseline.timestamp.map(|timestamp| (baseline, timestamp)))
            .map_or(
                Err(FailureKind::TemporarilyUnavailable),
                |(baseline, timestamp)| {
                    now.checked_duration_since(timestamp)
                        .filter(|elapsed| !elapsed.is_zero())
                        .ok_or(FailureKind::TemporarilyUnavailable)
                        .and_then(|elapsed| {
                            DiskRateSample::between(baseline, current, elapsed.as_secs_f64())
                        })
                },
            );
        let next_baseline = DiskStatsState {
            reads_completed: current.reads_completed,
            sectors_read: current.sectors_read,
            writes_completed: current.writes_completed,
            sectors_written: current.sectors_written,
            io_time_ms: current.io_time_ms,
            weighted_time_ms: current.weighted_time_ms,
            timestamp: Some(now),
        };
        if let Some(baseline) = previous.get_mut(physical_name) {
            *baseline = next_baseline;
        } else {
            previous.insert(physical_name.to_string(), next_baseline);
        }
        match observed {
            Ok(sample) => apply_rate_sample(disk, sample, now_ms),
            Err(failure) => apply_rate_failure(disk, failure),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DiskRateSample {
    read_bytes_per_sec: u64,
    write_bytes_per_sec: u64,
    iops: u64,
    active_time_pct: f32,
    response_time_ms: Option<f32>,
}

impl DiskRateSample {
    fn between(
        previous: &DiskStatsState,
        current: &DiskStatsState,
        elapsed_secs: f64,
    ) -> Result<Self, FailureKind> {
        let reads = current
            .reads_completed
            .checked_sub(previous.reads_completed)
            .ok_or(FailureKind::IdentityChanged)?;
        let writes = current
            .writes_completed
            .checked_sub(previous.writes_completed)
            .ok_or(FailureKind::IdentityChanged)?;
        let read_bytes = current
            .sectors_read
            .checked_sub(previous.sectors_read)
            .and_then(|sectors| sectors.checked_mul(512))
            .ok_or(FailureKind::IdentityChanged)?;
        let write_bytes = current
            .sectors_written
            .checked_sub(previous.sectors_written)
            .and_then(|sectors| sectors.checked_mul(512))
            .ok_or(FailureKind::IdentityChanged)?;
        let io_time_ms = current
            .io_time_ms
            .checked_sub(previous.io_time_ms)
            .ok_or(FailureKind::IdentityChanged)?;
        let weighted_time_ms = current
            .weighted_time_ms
            .checked_sub(previous.weighted_time_ms)
            .ok_or(FailureKind::IdentityChanged)?;
        let operations = reads
            .checked_add(writes)
            .ok_or(FailureKind::ProviderFault)?;
        let active_time_pct =
            ((io_time_ms as f64 / (elapsed_secs * 1_000.0)) * 100.0).clamp(0.0, 100.0) as f32;
        Ok(Self {
            read_bytes_per_sec: (read_bytes as f64 / elapsed_secs) as u64,
            write_bytes_per_sec: (write_bytes as f64 / elapsed_secs) as u64,
            iops: (operations as f64 / elapsed_secs) as u64,
            active_time_pct,
            // Average response time per I/O = weighted time / operations. Weighted
            // time (field 14) counts each interval × its in-flight count, so it
            // captures queueing + service; the old code divided io_time_ms (field
            // 13, busy/wall time) which is bounded by the wall clock and
            // under-reported latency by roughly the queue depth.
            response_time_ms: (operations > 0)
                .then(|| (weighted_time_ms as f64 / operations as f64) as f32),
        })
    }
}

fn apply_rate_sample(disk: &mut DiskMetrics, sample: DiskRateSample, now_ms: u64) {
    let mut observations = *disk.scalar_observations();
    observations.read_bytes_per_sec =
        ScalarObservation::available(sample.read_bytes_per_sec, now_ms);
    observations.write_bytes_per_sec =
        ScalarObservation::available(sample.write_bytes_per_sec, now_ms);
    observations.iops = ScalarObservation::available(sample.iops, now_ms);
    observations.active_time_pct = ScalarObservation::available(sample.active_time_pct, now_ms);
    observations.response_time_ms = sample.response_time_ms.map_or_else(
        || ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        |response_time_ms| ScalarObservation::available(response_time_ms, now_ms),
    );
    disk.apply_scalar_observations(observations);
}

fn apply_rate_failure(disk: &mut DiskMetrics, failure: FailureKind) {
    let mut observations = *disk.scalar_observations();
    observations.read_bytes_per_sec = ScalarObservation::unavailable(failure);
    observations.write_bytes_per_sec = ScalarObservation::unavailable(failure);
    observations.iops = ScalarObservation::unavailable(failure);
    observations.active_time_pct = ScalarObservation::unavailable(failure);
    observations.response_time_ms = ScalarObservation::unavailable(failure);
    disk.apply_scalar_observations(observations);
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_collector_disks_domain_tests.rs"]
mod tests;
