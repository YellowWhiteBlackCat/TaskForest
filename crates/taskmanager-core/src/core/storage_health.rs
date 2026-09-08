//! Platform-neutral filesystem and storage health contracts and alert thresholds.
//!
//! This module defines the domain models for filesystem mount health observations,
//! integrity monitoring, and the canonical threshold triggers for storage health
//! risk assessment across all frontends:
//!
//! # Storage Health Alert Triggers
//!
//! ## 1. Inode Exhaustion Risk
//!
//! In traditional Unix filesystems (e.g. ext4, XFS, F2FS, UFS), filesystem metadata
//! structures called inodes index each file, directory, and symbolic link. Filesystems
//! often format with a fixed number of inodes or enforce an allocation ceiling.
//!
//! - **Failure Mode**: When inode consumption approaches 100%, file and directory
//!   creations fail with `ENOSPC` ("No space left on device"), even if hundreds of
//!   gigabytes of disk block storage remain available. This causes silent transaction
//!   rollbacks, logging halts, broken sockets/pipes, and container crash loops.
//! - **Warning Trigger ([`DEFAULT_INODE_USAGE_WARNING_PERCENT`])**: Triggered when inode
//!   usage reaches or exceeds **85.0%** (or less than 15% remaining free inodes).
//!   Signals that rapid small-file growth (e.g. temporary caches, unrotated logs,
//!   mail spools, or node_modules/git directories) requires administrative cleanup.
//! - **Critical Trigger ([`DEFAULT_INODE_USAGE_CRITICAL_PERCENT`])**: Triggered when inode
//!   usage reaches or exceeds **95.0%** (or less than 5% free inodes). Signals
//!   imminent system lockup, requiring immediate action before write-dependent
//!   services crash.
//! - **Hysteresis ([`DEFAULT_STORAGE_ALERT_HYSTERESIS_PERCENT`])**: Active inode alerts
//!   clear only when usage drops below `threshold - 2.0%` (e.g., <= 83.0% for warning),
//!   preventing flapping during batched deletions or temporary directory cleanups.
//!
//! ## 2. NVMe Wear Level Thresholds
//!
//! Solid-state NVMe drives rely on NAND flash memory with a finite number of Program/Erase
//! (P/E) cycles, rated in Terabytes Written (TBW) or Drive Writes Per Day (DWPD).
//!
//! - **Specification Mapping**: NVMe Base Specification SMART / Health Information Log
//!   (Log Identifier 02h):
//!   - **Byte 5 ("Percentage Used")**: Contains a vendor-specific estimate of the
//!     percentage of NVM subsystem life consumed based on actual usage and the
//!     manufacturer's endurance model. 100 indicates that estimated rated endurance
//!     has been fully consumed. The value is allowed to exceed 100 (up to 254).
//!   - **Byte 0 ("Critical Warning")**: Hardware flags including available spare space
//!     falling below threshold (Bit 0), temperature critical (Bit 1), reliability
//!     degraded (Bit 2), or forced read-only mode (Bit 3).
//! - **Warning Trigger ([`DEFAULT_NVME_WEAR_WARNING_PERCENT`])**: Triggered when
//!   percentage used reaches or exceeds **90.0%**. Corresponds to the baseline `"smart-wear"`
//!   alert rule ([`crate::core::alerts::AlertMetric::SmartPercentUsed`]). Indicates that
//!   the device is approaching its certified endurance boundary and replacement planning
//!   should begin.
//! - **Critical Trigger ([`DEFAULT_NVME_WEAR_CRITICAL_PERCENT`])**: Triggered when
//!   percentage used reaches or exceeds **100.0%**. Indicates complete exhaustion of
//!   factory-rated endurance (100%+ TBW consumed). Beyond 100%, data retention periods
//!   when unpowered degrade, unrecoverable bit error rates (UBER) escalate, and manufacturer
//!   warranties expire.
//! - **Hardware Critical Warning**: Corresponds to [`crate::core::alerts::AlertMetric::SmartCriticalWarning`],
//!   which triggers immediately at [`crate::core::alerts::AlertSeverity::Critical`] whenever
//!   the controller asserts any critical health bit (such as spare block pool exhaustion).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

/// Default threshold (in percent) for warning-level inode usage exhaustion risk.
///
/// When inode usage on a filesystem exceeds this ratio, an alert is triggered
/// to warn that inode depletion is approaching, which leads to `ENOSPC` write
/// failures even when disk capacity has remaining free blocks.
pub const DEFAULT_INODE_USAGE_WARNING_PERCENT: f32 = 85.0;

/// Default threshold (in percent) for critical-level inode usage exhaustion risk.
///
/// At or above this threshold, inode exhaustion is imminent, risking immediate
/// write, pipe, socket, and logging failures across system processes.
pub const DEFAULT_INODE_USAGE_CRITICAL_PERCENT: f32 = 95.0;

/// Default threshold (in percent) for NVMe wear-level warning.
///
/// Corresponds to the baseline `"smart-wear"` alert rule ([`crate::core::alerts::AlertMetric::SmartPercentUsed`]).
/// At 90% endurance consumed, planned device replacement should be scheduled.
pub const DEFAULT_NVME_WEAR_WARNING_PERCENT: f32 = 90.0;

/// Default threshold (in percent) for NVMe wear-level critical alert.
///
/// According to the NVMe specification (SMART / Health Information Log Byte 5),
/// a value of 100 indicates that the manufacturer-estimated endurance (TBW) has
/// been completely consumed. Operating past 100% significantly increases the risk
/// of unrecoverable media errors and reduces data retention.
pub const DEFAULT_NVME_WEAR_CRITICAL_PERCENT: f32 = 100.0;

/// Conservative fallback warning threshold for the NVMe available-spare pool.
/// A controller-provided threshold takes precedence when it is available.
pub const DEFAULT_NVME_SPARE_WARNING_PERCENT: f32 = 10.0;

/// Critical available-spare threshold for NVMe media. At or below this
/// remaining spare pool, the controller is close to exhausting replacement
/// blocks even if its vendor-specific threshold was not readable.
pub const DEFAULT_NVME_SPARE_CRITICAL_PERCENT: f32 = 5.0;

/// Default hysteresis margin (in percent) applied to storage health threshold rules.
///
/// An active alert clears only when the observed metric drops below
/// `threshold - hysteresis`, preventing alert flapping during transient operations.
pub const DEFAULT_STORAGE_ALERT_HYSTERESIS_PERCENT: f32 = 2.0;

/// Evaluate the inode usage percentage for a filesystem given used and total inode counts.
///
/// Returns `None` if `total_inodes` is zero or the resulting percentage is not finite.
#[must_use]
pub fn inode_usage_percent(used_inodes: u64, total_inodes: u64) -> Option<f32> {
    if total_inodes == 0 {
        return None;
    }
    let pct = (used_inodes as f64 / total_inodes as f64 * 100.0) as f32;
    if pct.is_finite() { Some(pct) } else { None }
}

/// Check whether an observed NVMe wear level (percentage used) has reached or
/// exceeded the warning endurance threshold ([`DEFAULT_NVME_WEAR_WARNING_PERCENT`]).
#[must_use]
pub const fn is_nvme_wear_warning(percent_used: f32) -> bool {
    percent_used >= DEFAULT_NVME_WEAR_WARNING_PERCENT
}

/// Check whether an observed NVMe wear level (percentage used) has reached or
/// exceeded the critical endurance exhaustion threshold ([`DEFAULT_NVME_WEAR_CRITICAL_PERCENT`]).
#[must_use]
pub const fn is_nvme_wear_critical(percent_used: f32) -> bool {
    percent_used >= DEFAULT_NVME_WEAR_CRITICAL_PERCENT
}

/// Whether remaining NVMe spare capacity is at or below the warning threshold.
#[must_use]
pub const fn is_nvme_spare_warning(percent_available: f32) -> bool {
    percent_available <= DEFAULT_NVME_SPARE_WARNING_PERCENT
}

/// Whether remaining NVMe spare capacity is at or below the critical threshold.
#[must_use]
pub const fn is_nvme_spare_critical(percent_available: f32) -> bool {
    percent_available <= DEFAULT_NVME_SPARE_CRITICAL_PERCENT
}

/// Status classification of a mounted filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemHealthStatus {
    /// Filesystem is operating normally with read/write access and no detected errors.
    Healthy,
    /// Filesystem is mounted or was remounted read-only, possibly due to corruption.
    ReadOnly,
    /// Filesystem driver or SMART layer reported corrupt blocks, journal errors, or I/O faults.
    ErrorsReported,
}

/// Provider-neutral backing class for a mounted filesystem. Virtual
/// filesystems and copy-on-write datasets remain distinct from physical block
/// mounts in the UI and export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemBackingKind {
    PhysicalBlock,
    BtrfsSubvolume,
    ZfsDataset,
    Overlay,
    Tmpfs,
    Network,
    #[default]
    Unknown,
}

/// Platform-neutral health observation for a single mounted filesystem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilesystemHealth {
    /// Canonical mount point path (e.g. `/` or `/home`).
    pub mount_point: PathBuf,
    /// Optional underlying block device or virtual source path (e.g. `/dev/nvme0n1p2`).
    pub source: Option<PathBuf>,
    /// Filesystem driver name (e.g. "ext4", "btrfs", "xfs", "apfs", "ntfs").
    pub fs_type: String,
    /// Backing/topology class derived only from the mount source and driver.
    #[serde(default)]
    pub backing_kind: FilesystemBackingKind,
    /// Whether the filesystem is currently mounted read-only.
    pub read_only: Option<bool>,
    /// Driver-reported error count from sysfs or filesystem telemetry.
    pub error_count: Option<u64>,
    /// Number of allocated inodes observed by the provider, when the
    /// filesystem exposes a trustworthy statvfs-style count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inode_used: Option<u64>,
    /// Total inode capacity reported by the provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inode_total: Option<u64>,
    /// Inode consumption percentage, derived from `inode_used / inode_total`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inode_usage_percent: Option<f32>,
    /// Overall health status classification.
    pub status: FilesystemHealthStatus,
    /// Provider device state for the mount.
    pub state: DeviceState,
    /// Integrity state for the filesystem integrity telemetry.
    pub integrity_state: DeviceState,
}

/// Snapshot of all discovered filesystem mount health observations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FilesystemHealthSnapshot {
    /// Aggregate state of the filesystem health provider.
    pub state: DeviceState,
    /// Collection of individual filesystem health observations.
    pub filesystems: Vec<FilesystemHealth>,
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_storage_health_tests.rs"]
mod tests;
