//! Storage-topology projections for the Performance page's disk family.
//!
//! The disk family carries three storage facts a caption cannot carry: the
//! per-partition capacity share (the ring), the device's SMART verdict (the
//! badge) and the wear/temperature gauge. This module owns those PURE folds —
//! the renderer reads them, it never re-derives a fact.
//!
//! The honesty rules are the point of the module:
//!
//! - a partition without a measured capacity has NO ring fill: the ring
//!   stays an empty track and the readout stays the shared dash;
//! - a disk whose SMART provider is missing or unsupported has NO health
//!   verdict: the badge reads "unavailable", never a green shield;
//! - a disk without endurance or temperature facts has NO gauge at all.

use taskforest_core::core::device_state::DeviceStatus;
use taskforest_core::core::metrics::{DiskMetrics, SmartAvailability};
use taskforest_core::core::storage_health::{
    FilesystemHealthSnapshot, FilesystemHealthStatus, is_nvme_wear_critical, is_nvme_wear_warning,
};
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::{
    bytes, device_status_i18n_key, effective_smart_status, missing_value,
};

/// The command a user copies when the SMART tool is missing.
///
/// The shared contract publishes no per-platform install command, so the
/// frontend offers the platform-neutral smartmontools probe every platform
/// ships: it verifies the tool and lists the SMART-capable devices. A
/// per-platform install line would need a new shared contract fact.
pub(in crate::pages::performance) const SMART_TOOL_PROBE_COMMAND: &str = "smartctl --scan";

/// One partition's capacity ring.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::pages::performance) struct PartitionDonut {
    /// The partition's stable key inside its disk: the kernel partition name,
    /// or the projection's position when the name is empty.
    pub(in crate::pages::performance) key: String,
    /// The partition's display name: the mount point and filesystem when the
    /// partition is mounted, the kernel partition name otherwise.
    pub(in crate::pages::performance) name: String,
    /// The used/capacity readout; the shared dash when the partition
    /// published no capacity.
    pub(in crate::pages::performance) usage_text: String,
    /// The observed share of the capacity in use (0.0..=1.0), or `None` when
    /// the partition published no capacity — an empty ring, never a
    /// zero-percent ring.
    pub(in crate::pages::performance) used_fraction: Option<f32>,
}

/// The rings of one disk's partitions, in the projection's order.
pub(in crate::pages::performance) fn partition_donut_models(
    disk: &DiskMetrics,
) -> Vec<PartitionDonut> {
    disk.partitions
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let key = if part.name.is_empty() {
                format!("partition-{index}")
            } else {
                part.name.clone()
            };
            let name = if !part.mount_point.is_empty() {
                format!("{} ({})", part.mount_point, part.fs_type)
            } else {
                part.name.clone()
            };
            match (part.current_used_bytes(), part.current_capacity_bytes()) {
                (Some(used), Some(capacity)) if capacity > 0 => {
                    let fraction = (used as f32 / capacity as f32).clamp(0.0, 1.0);
                    PartitionDonut {
                        key,
                        name,
                        usage_text: format!(
                            "{} / {} ({:.0}%)",
                            bytes(used),
                            bytes(capacity),
                            fraction * 100.0
                        ),
                        used_fraction: Some(fraction),
                    }
                }
                _ => PartitionDonut {
                    key,
                    name,
                    usage_text: missing_value(),
                    used_fraction: None,
                },
            }
        })
        .collect()
}

/// The shape of the SMART health badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::pages::performance) enum SmartHealthBadge {
    /// The provider reported a healthy device: the green plate.
    Healthy,
    /// The provider reported a device whose health cannot be trusted: the
    /// amber triangle.
    Warning,
    /// No verdict exists (the tool is missing, or the device family has no
    /// provider): no plate claims a health state.
    Unavailable,
}

/// The disk's SMART verdict, folded from the shared effective status.
pub(in crate::pages::performance) fn smart_health_badge(disk: &DiskMetrics) -> SmartHealthBadge {
    match effective_smart_status(disk) {
        DeviceStatus::Healthy => SmartHealthBadge::Healthy,
        DeviceStatus::Stale | DeviceStatus::PermissionDenied => SmartHealthBadge::Warning,
        DeviceStatus::MissingTool | DeviceStatus::Unsupported => SmartHealthBadge::Unavailable,
    }
}

/// The badge's label: the shared device-status vocabulary, never a
/// frontend-local wording.
pub(in crate::pages::performance) fn smart_health_label(disk: &DiskMetrics) -> String {
    t(device_status_i18n_key(effective_smart_status(disk))).to_owned()
}

/// The wear band's severity, from the shared NVMe thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::pages::performance) enum WearBand {
    /// The device is inside its rated endurance.
    Good,
    /// The device crossed the shared warning threshold.
    Warning,
    /// The device crossed the shared critical threshold.
    Critical,
}

/// The NVMe wear/temperature gauge's facts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::pages::performance) struct NvmeGauge {
    /// The observed endurance consumption (0..=100+); `None` when the device
    /// published no endurance fact.
    pub(in crate::pages::performance) used_percent: Option<f32>,
    /// The observed device temperature.
    pub(in crate::pages::performance) temperature_c: Option<f32>,
    /// The provider's critical temperature, when it published one.
    pub(in crate::pages::performance) critical_c: Option<f32>,
    /// The band the observations fall in.
    pub(in crate::pages::performance) band: WearBand,
}

/// The gauge of one disk, or `None` when the device published neither an
/// endurance nor a temperature observation.
pub(in crate::pages::performance) fn nvme_gauge_model(disk: &DiskMetrics) -> Option<NvmeGauge> {
    let used = disk.smart_percent_used.filter(|value| value.is_finite());
    let temperature = disk.smart_temperature_c.filter(|value| value.is_finite());
    let critical = disk
        .smart_temp_critical_c
        .filter(|value| value.is_finite() && *value > 0.0);
    if used.is_none() && temperature.is_none() {
        return None;
    }
    let band = match used {
        Some(used) if is_nvme_wear_critical(used) => WearBand::Critical,
        Some(used) if is_nvme_wear_warning(used) => WearBand::Warning,
        _ => match (temperature, critical) {
            (Some(temperature), Some(critical)) if temperature >= critical => WearBand::Critical,
            _ => WearBand::Good,
        },
    };
    Some(NvmeGauge {
        used_percent: used,
        temperature_c: temperature,
        critical_c: critical,
        band,
    })
}

/// A filesystem-level alarm a partition row wears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::pages::performance) enum MountAlert {
    /// The filesystem is mounted read-only.
    ReadOnly,
    /// The driver reported errors on the filesystem.
    Errors,
}

impl MountAlert {
    /// The alarm's label: the shared health vocabulary.
    pub(in crate::pages::performance) fn label(self) -> String {
        match self {
            Self::ReadOnly => t("health.read_only").to_owned(),
            Self::Errors => t("health.errors_reported").to_owned(),
        }
    }
}

/// The alarms of the filesystem mounted at `mount_point`.
///
/// An empty list is the honest answer for a mount the provider did not
/// report: no alarm is claimed, and none is invented.
pub(in crate::pages::performance) fn mount_alerts(
    filesystems: Option<&FilesystemHealthSnapshot>,
    mount_point: &str,
) -> Vec<MountAlert> {
    let Some(snapshot) = filesystems else {
        return Vec::new();
    };
    let Some(filesystem) = snapshot
        .filesystems
        .iter()
        .find(|filesystem| filesystem.mount_point.to_string_lossy() == mount_point)
    else {
        return Vec::new();
    };
    let mut alerts = Vec::new();
    if filesystem.read_only == Some(true) || filesystem.status == FilesystemHealthStatus::ReadOnly {
        alerts.push(MountAlert::ReadOnly);
    }
    if filesystem.status == FilesystemHealthStatus::ErrorsReported
        || filesystem.error_count.is_some_and(|count| count > 0)
    {
        alerts.push(MountAlert::Errors);
    }
    alerts
}

/// The missing-tool guidance card's facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::pages::performance) struct SmartToolGuidance {
    /// The title: the shared "the SMART tool is not installed" line.
    pub(in crate::pages::performance) title: String,
    /// The actionable hint: the shared "install the tool, then refresh" line.
    pub(in crate::pages::performance) hint: String,
    /// The command a user copies to the terminal.
    pub(in crate::pages::performance) command: String,
}

/// The guidance for a disk whose SMART provider tool is missing.
///
/// `None` for every other availability: a healthy or merely unavailable
/// provider has nothing to install, and a card that claims an install step
/// would be a fabricated remediation.
pub(in crate::pages::performance) fn smart_tool_guidance(
    disk: &DiskMetrics,
) -> Option<SmartToolGuidance> {
    if disk.smart_availability != SmartAvailability::MissingTool {
        return None;
    }
    Some(SmartToolGuidance {
        title: t("disk.smart_missing_tool").to_owned(),
        hint: t("device.action_missing_tool").to_owned(),
        command: SMART_TOOL_PROBE_COMMAND.to_owned(),
    })
}
