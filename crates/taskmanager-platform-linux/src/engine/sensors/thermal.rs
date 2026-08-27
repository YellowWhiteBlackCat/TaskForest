//! Linux thermal-zone ABI provider.
//!
//! Thermal zones are not guaranteed to have a matching hwmon attachment.
//! This provider keeps their inventory, metadata, and temperature reads
//! independently fallible and lets the sensor composition layer own the one
//! lifecycle decision for the union of hwmon and thermal devices.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use taskmanager_core::{
    DeviceId, FailureKind, ProviderId, SensorDescriptor, SensorMagnitude,
    SensorMeasurementObservation, SensorReading, SensorScale, SourceOutcome, SourceStatus,
    ThermalCoolingDeviceStatus, ThermalZoneStatus,
};

mod control;

pub(super) const DISCOVERY_PROVIDER: ProviderId =
    ProviderId::borrowed("linux.sensor.thermal.discovery");
const METADATA_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.thermal.metadata");
const READING_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.thermal.readings");
const ZONE_STATUS_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.thermal.zone-status");
const TRIP_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.thermal.trip-points");
const COOLING_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.thermal.cooling-status");

#[derive(Debug)]
pub(super) struct ThermalSourceSnapshot {
    pub readings: Vec<SensorReading>,
    pub zones: Vec<ThermalZoneStatus>,
    pub cooling_devices: Vec<ThermalCoolingDeviceStatus>,
    pub discovered_devices: Vec<DeviceId>,
    pub discovery: SourceStatus,
    pub enrichments: Vec<SourceStatus>,
}

struct ZoneCandidate {
    directory: PathBuf,
    attachment: String,
    type_name: Result<String, FailureKind>,
    mirrored_device_id: Option<DeviceId>,
}

struct CoolingCandidate {
    directory: PathBuf,
    attachment: String,
    type_name: Result<String, FailureKind>,
}

pub(super) fn collect(
    root: &Path,
    mirrored_zones: &HashMap<PathBuf, DeviceId>,
    now_ms: u64,
) -> ThermalSourceSnapshot {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => return unavailable_snapshot(io_failure(error.kind(), true)),
    };

    let mut zones = Vec::new();
    let mut cooling_devices = Vec::new();
    let mut discovery_failure = None;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                record_failure(&mut discovery_failure, io_failure(error.kind(), false));
                continue;
            }
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            record_failure(&mut discovery_failure, FailureKind::ProviderFault);
            continue;
        };
        if !is_thermal_zone_name(name) && !is_cooling_device_name(name) {
            continue;
        }
        let directory = entry.path();
        if !is_directory_attachment(&entry, &directory, &mut discovery_failure) {
            continue;
        }
        let canonical = fs::canonicalize(&directory).unwrap_or_else(|_| directory.clone());
        let type_name = read_nonempty(directory.join("type"));
        if is_thermal_zone_name(name) {
            zones.push(ZoneCandidate {
                attachment: canonical.to_string_lossy().into_owned(),
                mirrored_device_id: mirrored_zones.get(&canonical).cloned(),
                type_name,
                directory,
            });
        } else {
            cooling_devices.push(CoolingCandidate {
                attachment: canonical.to_string_lossy().into_owned(),
                type_name,
                directory,
            });
        }
    }
    zones.sort_by(|left, right| left.attachment.cmp(&right.attachment));
    cooling_devices.sort_by(|left, right| left.attachment.cmp(&right.attachment));

    let mut zone_type_counts = HashMap::<&str, usize>::new();
    for candidate in &zones {
        if let Ok(type_name) = &candidate.type_name {
            *zone_type_counts.entry(type_name.as_str()).or_default() += 1;
        }
    }
    let mut cooling_type_counts = HashMap::<&str, usize>::new();
    for candidate in &cooling_devices {
        if let Ok(type_name) = &candidate.type_name {
            *cooling_type_counts.entry(type_name.as_str()).or_default() += 1;
        }
    }

    let mut readings = Vec::with_capacity(zones.len());
    let mut zone_statuses = Vec::with_capacity(zones.len());
    let mut cooling_statuses = Vec::with_capacity(cooling_devices.len());
    let mut devices = Vec::with_capacity(zones.len().saturating_add(cooling_devices.len()));
    let mut metadata_failure = None;
    let mut metadata_count = 0;
    let mut reading_failure = None;
    let mut reading_count = 0;
    let mut expected_readings = 0;
    for candidate in &zones {
        let label = match &candidate.type_name {
            Ok(type_name) => {
                metadata_count += 1;
                type_name.clone()
            }
            Err(failure) => {
                record_failure(&mut metadata_failure, *failure);
                "Linux thermal zone".to_owned()
            }
        };
        let device_id = candidate.mirrored_device_id.clone().unwrap_or_else(|| {
            if !zone_identity_is_stable(candidate, &zone_type_counts) {
                record_failure(&mut discovery_failure, FailureKind::Unsupported);
            }
            DeviceId::new(format!(
                "thermal:{}",
                zone_identity(candidate, &zone_type_counts)
            ))
        });
        let channel_identity = zone_channel_identity(candidate, &zone_type_counts);
        let zone_id = format!("{}:zone:{channel_identity}", device_id.as_str());
        if candidate.mirrored_device_id.is_none() {
            expected_readings += 1;
            let observation = read_temperature(candidate.directory.join("temp"), now_ms);
            if observation.current_value().is_some() {
                reading_count += 1;
            } else if let Some(failure) = observation.failure() {
                record_failure(&mut reading_failure, failure);
            }
            readings.push(SensorReading::from_measurement_observation(
                device_id.clone(),
                format!("{}:zone:{channel_identity}:temperature", device_id.as_str()),
                label,
                observation,
            ));
        }
        zone_statuses.push(control::collect_zone(
            &candidate.directory,
            zone_id,
            device_id.clone(),
            &candidate.type_name,
            now_ms,
        ));
        devices.push(device_id);
    }
    for candidate in &cooling_devices {
        match &candidate.type_name {
            Ok(_) => metadata_count += 1,
            Err(failure) => record_failure(&mut metadata_failure, *failure),
        }
        if !cooling_identity_is_stable(candidate, &cooling_type_counts) {
            record_failure(&mut discovery_failure, FailureKind::Unsupported);
        }
        let device_id = DeviceId::new(format!(
            "cooling:{}",
            cooling_identity(candidate, &cooling_type_counts)
        ));
        let channel_id = format!(
            "{}:channel:{}",
            device_id.as_str(),
            cooling_channel_identity(candidate, &cooling_type_counts)
        );
        cooling_statuses.push(control::collect_cooling_device(
            &candidate.directory,
            channel_id,
            device_id.clone(),
            &candidate.type_name,
            now_ms,
        ));
        devices.push(device_id);
    }
    readings.sort_by(|left, right| left.id().cmp(right.id()));
    zone_statuses.sort_by(|left, right| left.device_id.cmp(&right.device_id));
    cooling_statuses.sort_by(|left, right| left.device_id.cmp(&right.device_id));
    devices.sort();
    devices.dedup();

    let discovery = SourceStatus {
        provider: DISCOVERY_PROVIDER,
        outcome: collection_outcome(devices.len(), devices.len(), discovery_failure),
        item_count: devices.len(),
    };
    let mut enrichments = vec![
        SourceStatus {
            provider: METADATA_PROVIDER,
            outcome: collection_outcome(
                metadata_count,
                zone_statuses.len().saturating_add(cooling_statuses.len()),
                metadata_failure,
            ),
            item_count: metadata_count,
        },
        SourceStatus {
            provider: READING_PROVIDER,
            outcome: collection_outcome(reading_count, expected_readings, reading_failure),
            item_count: reading_count,
        },
    ];
    enrichments.push(zone_status_source(&zone_statuses));
    enrichments.push(trip_source(&zone_statuses));
    enrichments.push(cooling_source(&cooling_statuses));
    ThermalSourceSnapshot {
        readings,
        zones: zone_statuses,
        cooling_devices: cooling_statuses,
        discovered_devices: devices,
        discovery,
        enrichments,
    }
}

/// Canonical thermal-zone directories already represented by hwmon.
///
/// Kernel-created `hwmonN` and `thermal_zoneN` attachment numbers are not
/// identities. Canonical ancestry is used only to suppress duplicate sources;
/// the hwmon provider retains the physical-device identity for the reading.
pub(super) fn mirrored_zone_devices(hwmon_root: &Path) -> HashMap<PathBuf, DeviceId> {
    let Ok(entries) = fs::read_dir(hwmon_root) else {
        return HashMap::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = fs::canonicalize(entry.path()).ok()?;
            let zone = path
                .ancestors()
                .find(|ancestor| {
                    ancestor
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(is_thermal_zone_name)
                })?
                .to_path_buf();
            let chip = read_nonempty(entry.path().join("name"))
                .unwrap_or_else(|_| "unknown-chip".to_owned());
            let identity = super::sensor_device_identity(&entry.path(), &chip);
            Some((zone, DeviceId::new(format!("hwmon:{identity}:{chip}"))))
        })
        .collect()
}

fn zone_identity(candidate: &ZoneCandidate, type_counts: &HashMap<&str, usize>) -> String {
    if let Ok(device) = fs::canonicalize(candidate.directory.join("device")) {
        return format!("device:{}", device.to_string_lossy());
    }
    if let Ok(type_name) = &candidate.type_name
        && type_counts.get(type_name.as_str()) == Some(&1)
    {
        return format!("type:{type_name}");
    }
    format!(
        "attachment:{}:{}",
        candidate.attachment,
        type_label(candidate)
    )
}

fn zone_identity_is_stable(candidate: &ZoneCandidate, type_counts: &HashMap<&str, usize>) -> bool {
    fs::canonicalize(candidate.directory.join("device")).is_ok()
        || candidate
            .type_name
            .as_ref()
            .is_ok_and(|type_name| type_counts.get(type_name.as_str()) == Some(&1))
}

fn zone_channel_identity(candidate: &ZoneCandidate, type_counts: &HashMap<&str, usize>) -> String {
    if let Ok(type_name) = &candidate.type_name
        && type_counts.get(type_name.as_str()) == Some(&1)
    {
        return format!("type:{type_name}");
    }
    format!("attachment:{}", candidate.attachment)
}

fn cooling_identity(candidate: &CoolingCandidate, type_counts: &HashMap<&str, usize>) -> String {
    if let Ok(device) = fs::canonicalize(candidate.directory.join("device")) {
        return format!("device:{}", device.to_string_lossy());
    }
    if let Ok(type_name) = &candidate.type_name
        && type_counts.get(type_name.as_str()) == Some(&1)
    {
        return format!("type:{type_name}");
    }
    format!(
        "attachment:{}:{}",
        candidate.attachment,
        candidate.type_name.as_deref().unwrap_or("unknown-cooling")
    )
}

fn cooling_identity_is_stable(
    candidate: &CoolingCandidate,
    type_counts: &HashMap<&str, usize>,
) -> bool {
    fs::canonicalize(candidate.directory.join("device")).is_ok()
        || candidate
            .type_name
            .as_ref()
            .is_ok_and(|type_name| type_counts.get(type_name.as_str()) == Some(&1))
}

fn cooling_channel_identity(
    candidate: &CoolingCandidate,
    type_counts: &HashMap<&str, usize>,
) -> String {
    if let Ok(type_name) = &candidate.type_name
        && type_counts.get(type_name.as_str()) == Some(&1)
    {
        return format!("type:{type_name}");
    }
    format!("attachment:{}", candidate.attachment)
}

fn type_label(candidate: &ZoneCandidate) -> &str {
    candidate
        .type_name
        .as_deref()
        .unwrap_or("unknown-thermal-zone")
}

fn read_temperature(path: PathBuf, now_ms: u64) -> SensorMeasurementObservation {
    let descriptor = SensorDescriptor::temperature(SensorScale::MILLI);
    match fs::read_to_string(path) {
        Ok(text) => text.trim().parse::<i64>().ok().map_or_else(
            || {
                SensorMeasurementObservation::unavailable(
                    descriptor.clone(),
                    FailureKind::ProviderFault,
                )
            },
            |value| {
                SensorMeasurementObservation::available(
                    descriptor.clone(),
                    SensorMagnitude::Signed(value),
                    now_ms,
                )
                .unwrap_or_else(|_| {
                    SensorMeasurementObservation::unavailable(
                        descriptor.clone(),
                        FailureKind::ProviderFault,
                    )
                })
            },
        ),
        Err(error) => {
            SensorMeasurementObservation::unavailable(descriptor, io_failure(error.kind(), false))
        }
    }
}

fn read_nonempty(path: PathBuf) -> Result<String, FailureKind> {
    fs::read_to_string(path)
        .map_err(|error| io_failure(error.kind(), false))
        .and_then(|text| {
            let value = text.trim().to_owned();
            (!value.is_empty())
                .then_some(value)
                .ok_or(FailureKind::ProviderFault)
        })
}

fn is_directory_attachment(
    entry: &fs::DirEntry,
    directory: &Path,
    failure: &mut Option<FailureKind>,
) -> bool {
    match entry.file_type() {
        Ok(kind) if kind.is_dir() => true,
        Ok(kind) if kind.is_symlink() => match fs::metadata(directory) {
            Ok(metadata) => metadata.is_dir(),
            Err(error) => {
                record_failure(failure, io_failure(error.kind(), false));
                false
            }
        },
        Ok(_) => false,
        Err(error) => {
            record_failure(failure, io_failure(error.kind(), false));
            false
        }
    }
}

fn is_thermal_zone_name(name: &str) -> bool {
    name.strip_prefix("thermal_zone").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn is_cooling_device_name(name: &str) -> bool {
    name.strip_prefix("cooling_device").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn unavailable_snapshot(failure: FailureKind) -> ThermalSourceSnapshot {
    ThermalSourceSnapshot {
        readings: Vec::new(),
        zones: Vec::new(),
        cooling_devices: Vec::new(),
        discovered_devices: Vec::new(),
        discovery: SourceStatus {
            provider: DISCOVERY_PROVIDER,
            outcome: SourceOutcome::Unavailable(failure),
            item_count: 0,
        },
        enrichments: vec![
            SourceStatus {
                provider: METADATA_PROVIDER,
                outcome: SourceOutcome::Unavailable(failure),
                item_count: 0,
            },
            SourceStatus {
                provider: READING_PROVIDER,
                outcome: SourceOutcome::Unavailable(failure),
                item_count: 0,
            },
            SourceStatus {
                provider: ZONE_STATUS_PROVIDER,
                outcome: SourceOutcome::Unavailable(failure),
                item_count: 0,
            },
            SourceStatus {
                provider: TRIP_PROVIDER,
                outcome: SourceOutcome::Unavailable(failure),
                item_count: 0,
            },
            SourceStatus {
                provider: COOLING_PROVIDER,
                outcome: SourceOutcome::Unavailable(failure),
                item_count: 0,
            },
        ],
    }
}

fn collection_outcome(
    current: usize,
    expected: usize,
    failure: Option<FailureKind>,
) -> SourceOutcome {
    match (current, expected, failure) {
        (0, 0, None) => SourceOutcome::Empty,
        (current, expected, None) if current == expected => SourceOutcome::Available,
        (0, _, Some(failure)) => SourceOutcome::Unavailable(failure),
        (_, _, Some(failure)) => SourceOutcome::Partial(failure),
        _ => SourceOutcome::Unavailable(FailureKind::ProviderFault),
    }
}

fn io_failure(kind: ErrorKind, root: bool) -> FailureKind {
    match kind {
        ErrorKind::NotFound if root => FailureKind::Unsupported,
        ErrorKind::NotFound
        | ErrorKind::Interrupted
        | ErrorKind::WouldBlock
        | ErrorKind::TimedOut => FailureKind::TemporarilyUnavailable,
        ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        _ => FailureKind::ProviderFault,
    }
}

fn record_failure(current: &mut Option<FailureKind>, observed: FailureKind) {
    if current.is_none_or(|failure| failure_priority(observed) > failure_priority(failure)) {
        *current = Some(observed);
    }
}

fn zone_status_source(zones: &[ThermalZoneStatus]) -> SourceStatus {
    let (current, failure) = summarize_entities(
        zones
            .iter()
            .map(|zone| summarize_fields([zone.mode.availability(), zone.policy.availability()])),
    );
    SourceStatus {
        provider: ZONE_STATUS_PROVIDER,
        outcome: collection_outcome(current, zones.len(), failure),
        item_count: current,
    }
}

fn trip_source(zones: &[ThermalZoneStatus]) -> SourceStatus {
    let mut expected = 0;
    let mut current = 0;
    let mut failure = None;
    for zone in zones {
        if let Some(points) = zone.trip_points.current_points() {
            expected += points.len();
            for point in points {
                let (complete, observed_failure) = summarize_fields([
                    point.kind.availability(),
                    point.temperature_millicelsius.availability(),
                    point.hysteresis_millicelsius.availability(),
                ]);
                current += usize::from(complete);
                if let Some(observed_failure) = observed_failure {
                    record_failure(&mut failure, observed_failure);
                }
            }
        } else if let Some(observed_failure) = zone.trip_points.availability.failure() {
            record_failure(&mut failure, observed_failure);
        }
    }
    SourceStatus {
        provider: TRIP_PROVIDER,
        outcome: collection_outcome(current, expected, failure),
        item_count: current,
    }
}

fn cooling_source(devices: &[ThermalCoolingDeviceStatus]) -> SourceStatus {
    let (current, failure) = summarize_entities(devices.iter().map(|device| {
        summarize_fields([
            device.kind.availability(),
            device.current_state.availability(),
            device.maximum_state.availability(),
            device.activity.availability(),
        ])
    }));
    SourceStatus {
        provider: COOLING_PROVIDER,
        outcome: collection_outcome(current, devices.len(), failure),
        item_count: current,
    }
}

fn summarize_entities(
    entities: impl Iterator<Item = (bool, Option<FailureKind>)>,
) -> (usize, Option<FailureKind>) {
    let mut current = 0;
    let mut failure = None;
    for (complete, observed_failure) in entities {
        current += usize::from(complete);
        if let Some(observed_failure) = observed_failure {
            record_failure(&mut failure, observed_failure);
        }
    }
    (current, failure)
}

fn summarize_fields<const N: usize>(
    fields: [taskmanager_core::ScalarAvailability; N],
) -> (bool, Option<FailureKind>) {
    let complete = fields.iter().all(|field| field.is_current());
    let failure = fields
        .into_iter()
        .filter_map(taskmanager_core::ScalarAvailability::failure)
        .reduce(select_failure);
    (complete, failure)
}

const fn select_failure(left: FailureKind, right: FailureKind) -> FailureKind {
    if failure_priority(right) > failure_priority(left) {
        right
    } else {
        left
    }
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

#[cfg(test)]
#[path = "../../../tests/headless/engine/sensors/thermal.rs"]
mod tests;
