//! Complete health groups over typed cached facts; unavailable values stay explicit.
use super::{bytes, device_status_i18n_key, fan_rpm, power_w_precise, temperature_c_precise};
use taskmanager_application::i18n::t;
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::metrics::DiskMetrics;
use taskmanager_core::core::sensors::{SensorCenterSnapshot, SensorQuantity};
use taskmanager_core::core::storage_health::{FilesystemHealthSnapshot, FilesystemHealthStatus};
use taskmanager_core::core::system_health::SmartSelfTestObservation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HealthReviewSection {
    #[default]
    All,
    Storage,
    Sensors,
}
impl HealthReviewSection {
    pub const ALL: [Self; 3] = [Self::All, Self::Storage, Self::Sensors];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthReviewRow {
    pub label: String,
    pub value: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthReviewGroup {
    pub title: String,
    pub rows: Vec<HealthReviewRow>,
}
fn row(label: &'static str, value: impl Into<String>) -> HealthReviewRow {
    HealthReviewRow {
        label: t(label).into(),
        value: value.into(),
    }
}
fn missing() -> String {
    t("health.unavailable").into()
}

pub fn storage_groups(
    filesystems: Option<&FilesystemHealthSnapshot>,
    disks: &[DiskMetrics],
    reports: &[SmartSelfTestObservation],
) -> Vec<HealthReviewGroup> {
    let mut groups = Vec::new();
    if let Some(snapshot) = filesystems {
        for filesystem in &snapshot.filesystems {
            let disk = disks.iter().find(|disk| {
                !disk.mount_point.is_empty()
                    && std::path::Path::new(&disk.mount_point) == filesystem.mount_point
            });
            let space = disk
                .and_then(|disk| {
                    let total = disk.current_capacity_bytes()?;
                    let free = disk.current_available_bytes()?;
                    (total > 0).then(|| {
                        format!(
                            "{:.1}% · {} {}",
                            100.0 * total.saturating_sub(free) as f64 / total as f64,
                            bytes(free),
                            t("health.free")
                        )
                    })
                })
                .unwrap_or_else(missing);
            let status = match filesystem.status {
                FilesystemHealthStatus::Healthy => t("device.healthy"),
                FilesystemHealthStatus::ReadOnly => t("health.read_only"),
                FilesystemHealthStatus::ErrorsReported => t("health.errors_reported"),
            };
            let status = if filesystem.state.status == DeviceStatus::Healthy {
                status.to_owned()
            } else {
                format!(
                    "{status} · {}",
                    t(device_status_i18n_key(filesystem.state.status))
                )
            };
            let inodes = match (filesystem.inode_used, filesystem.inode_total) {
                (Some(used), Some(total)) if total > 0 => format!("{used} / {total}"),
                _ => missing(),
            };
            groups.push(HealthReviewGroup {
                title: filesystem.mount_point.to_string_lossy().into_owned(),
                rows: vec![
                    row("common.status", status),
                    row(
                        "health.source",
                        format!(
                            "{} · {}",
                            filesystem
                                .source
                                .as_ref()
                                .map(|source| source.to_string_lossy().into_owned())
                                .unwrap_or_else(missing),
                            filesystem.fs_type
                        ),
                    ),
                    row("health.space", space),
                    row("health.inodes", inodes),
                    row(
                        "health.read_only",
                        filesystem
                            .read_only
                            .map(|value| {
                                t(if value { "common.yes" } else { "common.no" }).to_owned()
                            })
                            .unwrap_or_else(missing),
                    ),
                    row(
                        "health.errors",
                        filesystem
                            .error_count
                            .map(|value| value.to_string())
                            .unwrap_or_else(missing),
                    ),
                ],
            });
        }
    }
    if groups.is_empty() {
        groups.push(HealthReviewGroup {
            title: t("health.storage").into(),
            rows: vec![row("common.status", t("health.no_filesystems"))],
        });
    }
    for report in reports {
        let observed = disks.iter().any(|disk| {
            disk.device_id == report.device_id.as_str()
                && disk.device_generation == report.device_generation
        });
        if !observed {
            continue;
        }
        groups.push(HealthReviewGroup {
            title: format!("{} · {}", t("health.smart_self_test"), report.display_name),
            rows: vec![
                row(
                    "common.status",
                    format!(
                        "{:?} · {}",
                        report.report.phase,
                        t(device_status_i18n_key(report.report.state.status))
                    ),
                ),
                row(
                    "health.progress",
                    report
                        .report
                        .progress_pct
                        .map(|value| format!("{value:.0}%"))
                        .unwrap_or_else(missing),
                ),
                row(
                    "health.lifetime_hours",
                    report
                        .report
                        .lifetime_hours
                        .map(|value| value.to_string())
                        .unwrap_or_else(missing),
                ),
                row(
                    "health.first_error_lba",
                    report
                        .report
                        .first_error_lba
                        .map(|value| value.to_string())
                        .unwrap_or_else(missing),
                ),
            ],
        });
    }
    groups
}

pub fn sensor_groups(snapshot: Option<&SensorCenterSnapshot>) -> Vec<HealthReviewGroup> {
    let mut groups = Vec::new();
    for (quantity, title) in [
        (SensorQuantity::Temperature, "common.thermal_zones"),
        (SensorQuantity::FanSpeed, "health.fans"),
        (SensorQuantity::Power, "common.power_sensors"),
    ] {
        let readings = snapshot
            .into_iter()
            .flat_map(|snapshot| &snapshot.readings)
            .filter(|reading| reading.quantity() == &quantity)
            .map(|reading| {
                let value = reading
                    .current_number()
                    .map(|value| match quantity {
                        SensorQuantity::Temperature => temperature_c_precise(value as f32),
                        SensorQuantity::FanSpeed => fan_rpm(value as f32),
                        SensorQuantity::Power => power_w_precise(value as f32),
                        _ => missing(),
                    })
                    .unwrap_or_else(missing);
                HealthReviewRow {
                    label: reading.label().into(),
                    value: format!(
                        "{value} · {}",
                        t(device_status_i18n_key(reading.state().status))
                    ),
                }
            })
            .collect::<Vec<_>>();
        groups.push(HealthReviewGroup {
            title: t(title).into(),
            rows: if readings.is_empty() {
                vec![row("common.status", t("health.no_readings"))]
            } else {
                readings
            },
        });
    }
    groups
}

#[cfg(test)]
#[path = "../../tests/headless/presentation_health_review.rs"]
mod tests;
