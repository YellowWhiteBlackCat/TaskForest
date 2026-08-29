//! Stable storage identity reconciliation across metadata degradation.

use std::collections::HashMap;

use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::{DiskMetrics, StorageIdentityStability};
use taskmanager_core::core::source::SourceOutcome;

/// Keep stable disk identity across transient sysfs metadata degradation.
///
/// Kernel block names are the discovery correlation key for one device's
/// continuous presence. A partial metadata refresh may temporarily lose WWID
/// or serial access; in that case a lower-quality fallback must not turn the
/// same `/sys/block/<name>` entry into a new lifecycle identity. Stronger or
/// equally strong successfully observed identities still replace the cache,
/// allowing a real same-slot hot-swap to surface.
///
/// The returned IDs exactly match the rewritten metrics and are suitable for
/// `DeviceSourceSnapshot::discovered_devices`. Cache expiry remains the
/// lifecycle owner's responsibility after confirmed absence.
pub(crate) fn reconcile_storage_identity(
    metrics: &mut [DiskMetrics],
    metadata_outcome: SourceOutcome,
    identity_cache: &mut HashMap<String, DeviceId>,
) -> Vec<DeviceId> {
    let metadata_is_authoritative = matches!(
        metadata_outcome,
        SourceOutcome::Available | SourceOutcome::Empty
    );
    let mut discovered_devices = Vec::with_capacity(metrics.len());

    for metric in metrics {
        let kernel_name = metric.name.trim_start_matches("/dev/").to_string();
        let observed = DeviceId::new(metric.device_id.clone());
        let observed_rank = disk_identity_rank(observed.as_str());
        let selected = match identity_cache.get(&kernel_name) {
            Some(cached)
                if !metadata_is_authoritative
                    && disk_identity_rank(cached.as_str()) > observed_rank =>
            {
                cached.clone()
            }
            _ => {
                identity_cache.insert(kernel_name, observed.clone());
                observed
            }
        };
        metric.device_id = selected.as_str().to_string();
        for partition in &mut metric.partitions {
            partition.parent_device_id = selected.as_str().to_string();
            partition.device_id = taskmanager_core::core::metrics::DiskPartition::stable_id(
                selected.as_str(),
                &partition.name,
            );
        }
        metric.identity_stability = if disk_identity_rank(selected.as_str()) > 1 {
            StorageIdentityStability::Persistent
        } else {
            StorageIdentityStability::Attachment
        };
        discovered_devices.push(selected);
    }

    discovered_devices.sort();
    discovered_devices.dedup();
    discovered_devices
}

fn disk_identity_rank(identity: &str) -> u8 {
    if identity.starts_with("disk:wwid:") {
        3
    } else if identity.starts_with("disk:serial:") {
        2
    } else {
        1
    }
}
