//! Typed Linux thermal control-field readers.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use taskmanager_core::{
    DeviceId, FailureKind, ScalarObservation, ThermalCoolingActivity, ThermalCoolingDeviceStatus,
    ThermalCoolingKind, ThermalPolicy, ThermalTripKind, ThermalTripPoint, ThermalTripPointSet,
    ThermalZoneMode, ThermalZoneStatus,
};

use super::{io_failure, read_nonempty, record_failure};

pub(super) fn collect_zone(
    directory: &Path,
    id: String,
    device_id: DeviceId,
    type_name: &Result<String, FailureKind>,
    now_ms: u64,
) -> ThermalZoneStatus {
    ThermalZoneStatus {
        id,
        device_id,
        device_generation: Default::default(),
        label: observed_result(type_name, now_ms),
        mode: read_field(directory.join("mode"), now_ms, parse_mode),
        policy: read_field(directory.join("policy"), now_ms, parse_policy),
        trip_points: collect_trip_points(directory, now_ms),
    }
}

pub(super) fn collect_cooling_device(
    directory: &Path,
    id: String,
    device_id: DeviceId,
    type_name: &Result<String, FailureKind>,
    now_ms: u64,
) -> ThermalCoolingDeviceStatus {
    let kind = type_name
        .as_ref()
        .map(|value| parse_cooling_kind(value))
        .map_or_else(
            |failure| ScalarObservation::unavailable(*failure),
            |value| ScalarObservation::available(value, now_ms),
        );
    let current_state = read_field(directory.join("cur_state"), now_ms, parse_u64);
    let maximum_state = read_field(directory.join("max_state"), now_ms, parse_u64);
    let activity = cooling_activity(&current_state, &maximum_state, now_ms);
    ThermalCoolingDeviceStatus {
        id,
        device_id,
        device_generation: Default::default(),
        kind,
        current_state,
        maximum_state,
        activity,
    }
}

fn collect_trip_points(directory: &Path, now_ms: u64) -> ThermalTripPointSet {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            return ThermalTripPointSet::unavailable(io_failure(error.kind(), false));
        }
    };
    let mut indices = BTreeSet::new();
    let mut enumeration_failure = None;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                record_failure(&mut enumeration_failure, io_failure(error.kind(), false));
                continue;
            }
        };
        if let Some(index) = trip_index(&entry.file_name().to_string_lossy()) {
            indices.insert(index);
        }
    }
    let points = indices
        .into_iter()
        .map(|index| collect_trip_point(directory, index, now_ms))
        .collect::<Vec<_>>();
    match enumeration_failure {
        Some(failure) if points.is_empty() => ThermalTripPointSet::unavailable(failure),
        Some(failure) => ThermalTripPointSet::partial(points, now_ms, failure),
        None => ThermalTripPointSet::available(points, now_ms),
    }
}

fn collect_trip_point(directory: &Path, index: u32, now_ms: u64) -> ThermalTripPoint {
    let prefix = directory.join(format!("trip_point_{index}"));
    ThermalTripPoint {
        id: format!("trip:{index}"),
        kind: read_field(with_suffix(&prefix, "_type"), now_ms, parse_trip_kind),
        temperature_millicelsius: read_field(
            with_suffix(&prefix, "_temp"),
            now_ms,
            parse_temperature,
        ),
        hysteresis_millicelsius: read_field(with_suffix(&prefix, "_hyst"), now_ms, parse_u64),
    }
}

fn with_suffix(prefix: &Path, suffix: &str) -> PathBuf {
    let mut value = prefix.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn trip_index(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("trip_point_")?;
    let (index, field) = rest.split_once('_')?;
    matches!(field, "type" | "temp" | "hyst")
        .then(|| index.parse().ok())
        .flatten()
}

fn read_field<T>(
    path: PathBuf,
    now_ms: u64,
    parse: impl FnOnce(&str) -> Result<T, FailureKind>,
) -> ScalarObservation<T> {
    match read_nonempty(path).and_then(|value| parse(&value)) {
        Ok(value) => ScalarObservation::available(value, now_ms),
        Err(failure) => ScalarObservation::unavailable(failure),
    }
}

fn observed_result(result: &Result<String, FailureKind>, now_ms: u64) -> ScalarObservation<String> {
    result.as_ref().map_or_else(
        |failure| ScalarObservation::unavailable(*failure),
        |value| ScalarObservation::available(value.clone(), now_ms),
    )
}

fn parse_mode(value: &str) -> Result<ThermalZoneMode, FailureKind> {
    Ok(match value {
        "enabled" => ThermalZoneMode::Enabled,
        "disabled" => ThermalZoneMode::Disabled,
        value => ThermalZoneMode::Other(value.to_owned()),
    })
}

fn parse_policy(value: &str) -> Result<ThermalPolicy, FailureKind> {
    Ok(match value {
        "power_allocator" => ThermalPolicy::PowerAllocator,
        "user_space" => ThermalPolicy::UserSpace,
        "step_wise" => ThermalPolicy::StepWise,
        "bang_bang" => ThermalPolicy::BangBang,
        "fair_share" => ThermalPolicy::FairShare,
        value => ThermalPolicy::Other(value.to_owned()),
    })
}

fn parse_trip_kind(value: &str) -> Result<ThermalTripKind, FailureKind> {
    Ok(match value {
        "active" => ThermalTripKind::Active,
        "passive" => ThermalTripKind::Passive,
        "hot" => ThermalTripKind::Hot,
        "critical" => ThermalTripKind::Critical,
        value => ThermalTripKind::Other(value.to_owned()),
    })
}

fn parse_cooling_kind(value: &str) -> ThermalCoolingKind {
    match value {
        "Fan" | "fan" => ThermalCoolingKind::Fan,
        "Processor" | "processor" => ThermalCoolingKind::Processor,
        "CHRG" | "charger" => ThermalCoolingKind::Charger,
        "iwlwifi" | "radio" => ThermalCoolingKind::Radio,
        "intel_powerclamp" | "powerclamp" => ThermalCoolingKind::PowerClamp,
        "TCC Offset" | "temperature_offset" => ThermalCoolingKind::TemperatureOffset,
        value => ThermalCoolingKind::Other(value.to_owned()),
    }
}

fn parse_temperature(value: &str) -> Result<i64, FailureKind> {
    let value = value
        .parse::<i64>()
        .map_err(|_| FailureKind::ProviderFault)?;
    (value >= -273_150)
        .then_some(value)
        .ok_or(FailureKind::ProviderFault)
}

fn parse_u64(value: &str) -> Result<u64, FailureKind> {
    value.parse().map_err(|_| FailureKind::ProviderFault)
}

fn cooling_activity(
    current: &ScalarObservation<u64>,
    maximum: &ScalarObservation<u64>,
    now_ms: u64,
) -> ScalarObservation<ThermalCoolingActivity> {
    match (current.current_value(), maximum.current_value()) {
        (Some(current), Some(maximum)) if current <= maximum => ScalarObservation::available(
            if *current == 0 {
                ThermalCoolingActivity::Inactive
            } else {
                ThermalCoolingActivity::Active
            },
            now_ms,
        ),
        (Some(_), Some(_)) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        _ => ScalarObservation::unavailable(
            [
                current.availability().failure(),
                maximum.availability().failure(),
            ]
            .into_iter()
            .flatten()
            .reduce(super::select_failure)
            .unwrap_or(FailureKind::TemporarilyUnavailable),
        ),
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/engine/sensors/thermal/control.rs"]
mod tests;
