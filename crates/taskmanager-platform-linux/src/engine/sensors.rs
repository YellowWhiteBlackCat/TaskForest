//! Linux hwmon sensor and thermal-throttle providers.

#[cfg(target_os = "linux")]
mod composition;
mod hwmon;
mod iio;
mod thermal;
pub mod trend;

#[cfg(test)]
#[path = "../../tests/headless/engine/sensors/composition_tests.rs"]
mod composition_tests;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::{DeviceId, ProviderId};
use taskmanager_core::core::sensors::{
    SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
    SensorReading, ThermalControlSnapshot,
};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_platform_contract::DeviceSourceSnapshot;

const HWMON_DISCOVERY_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.hwmon.discovery");
const HWMON_READING_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.hwmon.readings");
#[cfg(target_os = "linux")]
const SYSFS_INVENTORY_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.sysfs-inventory");

#[cfg(all(target_os = "linux", feature = "test-support"))]
pub fn collect_sensor_center(now_ms: u64) -> SensorCenterSnapshot {
    collect_sensor_center_source(now_ms).value
}

#[cfg(all(not(target_os = "linux"), feature = "test-support"))]
pub fn collect_sensor_center(now_ms: u64) -> SensorCenterSnapshot {
    collect_sensor_center_source(now_ms).value
}

#[cfg(target_os = "linux")]
pub(crate) fn collect_sensor_center_source(
    now_ms: u64,
) -> DeviceSourceSnapshot<SensorCenterSnapshot> {
    composition::collect_sensor_center_source_from_roots(
        Path::new("/sys/class/hwmon"),
        Path::new("/sys/class/thermal"),
        Path::new("/sys/devices/system/cpu"),
        Path::new("/sys/bus/iio/devices"),
        now_ms,
    )
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn collect_sensor_center_source(
    now_ms: u64,
) -> DeviceSourceSnapshot<SensorCenterSnapshot> {
    DeviceSourceSnapshot::from_source_status(
        SensorCenterSnapshot {
            timestamp_ms: now_ms,
            ..Default::default()
        },
        Vec::new(),
        SourceStatus {
            provider: HWMON_DISCOVERY_PROVIDER,
            outcome: SourceOutcome::Unavailable(FailureKind::Unsupported),
            item_count: 0,
        },
        Vec::new(),
    )
}

#[cfg(all(target_os = "linux", any(test, feature = "test-support")))]
pub fn collect_sensor_center_from(root: &Path, now_ms: u64) -> SensorCenterSnapshot {
    collect_sensor_center_source_from(root, now_ms).value
}

#[cfg(target_os = "linux")]
fn collect_sensor_center_source_from(
    root: &Path,
    now_ms: u64,
) -> DeviceSourceSnapshot<SensorCenterSnapshot> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            let (status, failure) = if error.kind() == std::io::ErrorKind::PermissionDenied {
                (
                    DeviceStatus::PermissionDenied,
                    FailureKind::PermissionDenied,
                )
            } else if error.kind() == std::io::ErrorKind::NotFound {
                (DeviceStatus::Unsupported, FailureKind::Unsupported)
            } else {
                (DeviceStatus::Stale, FailureKind::ProviderFault)
            };
            return DeviceSourceSnapshot::from_source_status(
                SensorCenterSnapshot {
                    state: DeviceState::default().transition(status, now_ms),
                    timestamp_ms: now_ms,
                    readings: Vec::new(),
                    thermal_control: Default::default(),
                    device_lifecycles: Default::default(),
                },
                Vec::new(),
                SourceStatus {
                    provider: HWMON_DISCOVERY_PROVIDER,
                    outcome: SourceOutcome::Unavailable(failure),
                    item_count: 0,
                },
                Vec::new(),
            );
        }
    };
    let mut readings = Vec::new();
    let mut enumeration_failure = None;
    let mut discovered_devices = HashSet::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                record_enumeration_failure(&mut enumeration_failure, &error);
                continue;
            }
        };
        let directory = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                record_enumeration_failure(&mut enumeration_failure, &error);
                continue;
            }
        };
        if !file_type.is_dir() {
            if !file_type.is_symlink() {
                continue;
            }
            match std::fs::metadata(&directory) {
                Ok(metadata) if metadata.is_dir() => {}
                Ok(_) => continue,
                Err(error) => {
                    record_enumeration_failure(&mut enumeration_failure, &error);
                    continue;
                }
            }
        }
        let chip = read_trimmed(directory.join("name"))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "unknown-chip".to_owned());
        let (device_identity, identity_is_stable) =
            sensor_device_identity_with_quality(&directory, &chip);
        if !identity_is_stable {
            enumeration_failure = Some(stronger_failure(
                enumeration_failure,
                FailureKind::Unsupported,
            ));
        }
        let device_id = DeviceId::new(format!("hwmon:{device_identity}:{chip}"));
        // The directory/attachment is discovery authority. Supported channel
        // families affect only reading enrichment, never physical presence.
        discovered_devices.insert(device_id.clone());
        let sensor_entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                record_enumeration_failure(&mut enumeration_failure, &error);
                continue;
            }
        };
        for sensor_entry in sensor_entries {
            let sensor_entry = match sensor_entry {
                Ok(entry) => entry,
                Err(error) => {
                    record_enumeration_failure(&mut enumeration_failure, &error);
                    continue;
                }
            };
            let input_name = sensor_entry.file_name().to_string_lossy().into_owned();
            let Some(channel) = hwmon::parse_channel(&input_name) else {
                continue;
            };
            let label = read_trimmed(
                directory.join(format!("{}{}_label", channel.prefix, channel.channel)),
            )
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| format!("{chip} {}{}", channel.prefix, channel.channel));
            readings.push(read_sensor(
                device_id.clone(),
                format!(
                    "{}:{}{}",
                    device_id.as_str(),
                    channel.prefix,
                    channel.channel
                ),
                label,
                channel.descriptor,
                std::fs::read_to_string(sensor_entry.path()),
                now_ms,
            ));
        }
    }
    readings.sort_by(|left, right| left.id().cmp(right.id()));
    let status = if let Some(failure) = enumeration_failure {
        DeviceStatus::from_failure(failure)
    } else if readings
        .iter()
        .any(|reading| reading.current_measurement().is_some())
    {
        DeviceStatus::Healthy
    } else if readings.iter().any(|reading| {
        reading.measurement_observation().failure() == Some(FailureKind::PermissionDenied)
    }) {
        DeviceStatus::PermissionDenied
    } else if readings.is_empty() {
        DeviceStatus::Healthy
    } else {
        DeviceStatus::Stale
    };
    let successful_readings = readings
        .iter()
        .filter(|reading| reading.current_measurement().is_some())
        .count();
    let reading_failure = readings
        .iter()
        .filter_map(|reading| reading.measurement_observation().failure())
        .chain(enumeration_failure)
        .max_by_key(|failure| failure_priority(*failure));
    let reading_outcome = match (successful_readings, readings.len(), reading_failure) {
        (0, _, Some(failure)) => SourceOutcome::Unavailable(failure),
        (successful, _, Some(failure)) if successful > 0 => SourceOutcome::Partial(failure),
        (0, 0, None) => SourceOutcome::Empty,
        (successful, total, None) if successful == total => SourceOutcome::Available,
        _ => SourceOutcome::Unavailable(FailureKind::ProviderFault),
    };
    let mut discovered_devices = discovered_devices.into_iter().collect::<Vec<_>>();
    discovered_devices.sort();
    let discovery_outcome = match enumeration_failure {
        Some(failure) if discovered_devices.is_empty() => SourceOutcome::Unavailable(failure),
        Some(failure) => SourceOutcome::Partial(failure),
        None if discovered_devices.is_empty() => SourceOutcome::Empty,
        None => SourceOutcome::Available,
    };
    let discovered_count = discovered_devices.len();
    DeviceSourceSnapshot::from_source_status(
        SensorCenterSnapshot {
            state: DeviceState::default().transition(status, now_ms),
            timestamp_ms: now_ms,
            readings,
            thermal_control: Default::default(),
            device_lifecycles: Default::default(),
        },
        discovered_devices,
        SourceStatus {
            provider: HWMON_DISCOVERY_PROVIDER,
            outcome: discovery_outcome,
            item_count: discovered_count,
        },
        vec![SourceStatus {
            provider: HWMON_READING_PROVIDER,
            outcome: reading_outcome,
            item_count: successful_readings,
        }],
    )
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::MissingDependency => 7,
        FailureKind::TimedOut => 6,
        FailureKind::ProviderFault => 5,
        FailureKind::TemporarilyUnavailable => 4,
        FailureKind::Unsupported => 3,
        FailureKind::IdentityChanged | FailureKind::Rejected => 1,
    }
}

fn record_enumeration_failure(failure: &mut Option<FailureKind>, error: &std::io::Error) {
    let observed = sensor_io_failure(error);
    *failure = Some(stronger_failure(*failure, observed));
}

const fn stronger_failure(current: Option<FailureKind>, candidate: FailureKind) -> FailureKind {
    match current {
        Some(current) if failure_priority(current) >= failure_priority(candidate) => current,
        Some(_) | None => candidate,
    }
}

fn sensor_device_identity(directory: &Path, chip: &str) -> String {
    sensor_device_identity_with_quality(directory, chip).0
}

fn sensor_device_identity_with_quality(directory: &Path, chip: &str) -> (String, bool) {
    canonical_physical_device(directory)
        .map(|path| (path.to_string_lossy().into_owned(), true))
        .unwrap_or_else(|| (attachment_scoped_identity(directory, chip), false))
}

fn canonical_physical_device(directory: &Path) -> Option<PathBuf> {
    if let Ok(device) = std::fs::canonicalize(directory.join("device")) {
        return Some(device);
    }
    if !std::fs::symlink_metadata(directory)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
    {
        return None;
    }
    let canonical = std::fs::canonicalize(directory).ok()?;
    let dynamic_hwmon_name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.strip_prefix("hwmon").is_some_and(|index| {
                !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())
            })
        });
    let hwmon_parent = canonical
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        == Some("hwmon");
    (dynamic_hwmon_name && hwmon_parent)
        .then(|| canonical.parent()?.parent().map(Path::to_path_buf))
        .flatten()
}

fn attachment_scoped_identity(directory: &Path, chip: &str) -> String {
    if let Ok(target) = std::fs::read_link(directory.join("device")) {
        let root = directory.parent().unwrap_or(directory);
        return format!(
            "attachment-symlink:{}=>{}",
            root.to_string_lossy(),
            target.to_string_lossy()
        );
    }
    // `hwmonN` is not a physical identity and can change after re-enumeration.
    // It is nevertheless collision-safe within this attachment snapshot and
    // more honest than merging two same-name chips into one fake device.
    format!("attachment-entry:{}:{chip}", directory.to_string_lossy())
}

fn read_sensor(
    device_id: DeviceId,
    id: String,
    label: String,
    descriptor: SensorDescriptor,
    result: std::io::Result<String>,
    now_ms: u64,
) -> SensorReading {
    read_sensor_with(
        device_id,
        id,
        label,
        descriptor,
        result,
        now_ms,
        hwmon::parse_magnitude,
    )
}

/// Shared reading construction for every sysfs sensor provider; the magnitude
/// parser is provider-specific (hwmon fixed-point vs IIO signed raw).
fn read_sensor_with(
    device_id: DeviceId,
    id: String,
    label: String,
    descriptor: SensorDescriptor,
    result: std::io::Result<String>,
    now_ms: u64,
    parse: impl Fn(&SensorDescriptor, &str) -> Option<SensorMagnitude>,
) -> SensorReading {
    let observation = match result {
        Ok(text) => parse(&descriptor, text.trim()).map_or_else(
            || {
                SensorMeasurementObservation::unavailable(
                    descriptor.clone(),
                    FailureKind::ProviderFault,
                )
            },
            |value| {
                SensorMeasurementObservation::available(descriptor.clone(), value, now_ms)
                    .unwrap_or_else(|_| {
                        SensorMeasurementObservation::unavailable(
                            descriptor.clone(),
                            FailureKind::ProviderFault,
                        )
                    })
            },
        ),
        Err(error) => {
            SensorMeasurementObservation::unavailable(descriptor, sensor_io_failure(&error))
        }
    };
    SensorReading::from_measurement_observation(device_id, id, label, observation)
}

fn sensor_io_failure(error: &std::io::Error) -> FailureKind {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        std::io::ErrorKind::NotFound
        | std::io::ErrorKind::Interrupted
        | std::io::ErrorKind::WouldBlock
        | std::io::ErrorKind::TimedOut => FailureKind::TemporarilyUnavailable,
        _ => FailureKind::ProviderFault,
    }
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
}

#[cfg(test)]
#[path = "../../tests/headless/linux_engine_sensors_tests.rs"]
mod tests;
