//! Linux industrial I/O (IIO) ABI channel classification, scale parsing, and
//! fixture-driven collection — the non-hwmon sensor provider.
//!
//! IIO exposes per-device sysfs trees under `/sys/bus/iio/devices/iio:deviceN`
//! with `in_<type>[<index>][_<name>]_raw` integer samples and per-channel or
//! shared `_scale` decimal files. Unlike hwmon's fixed-point ABI, the scale is
//! device-specific, so a raw count is only converted to SI when a parseable
//! `_scale` file exists; otherwise the channel is retained as an honest opaque
//! raw reading instead of guessing a scale.

use taskmanager_core::{
    SensorDescriptor, SensorMagnitude, SensorQuantity, SensorScale, SensorUnit,
};
use taskmanager_platform_contract::{
    DeviceSourceSnapshot, FailureKind, ProviderId, SourceOutcome, SourceStatus,
};

use super::*;

const IIO_DISCOVERY_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.iio.discovery");
const IIO_READING_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.iio.readings");

/// Collect one IIO inventory snapshot from `root` (fixture-friendly).
pub(super) fn collect_iio_source_from(
    root: &Path,
    now_ms: u64,
) -> DeviceSourceSnapshot<SensorCenterSnapshot> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            // A missing IIO bus is kernel absence, not a failure: an optional
            // provider without CONFIG_IIO reports an honest Empty so the
            // composite sensor center stays Available on such hosts. Any other
            // enumeration error is a real failure.
            if error.kind() == std::io::ErrorKind::NotFound {
                return DeviceSourceSnapshot::from_source_status(
                    SensorCenterSnapshot {
                        state: DeviceState::default().transition(DeviceStatus::Unsupported, now_ms),
                        timestamp_ms: now_ms,
                        readings: Vec::new(),
                        thermal_control: Default::default(),
                        device_lifecycles: Default::default(),
                    },
                    Vec::new(),
                    SourceStatus {
                        provider: IIO_DISCOVERY_PROVIDER,
                        outcome: SourceOutcome::Empty,
                        item_count: 0,
                    },
                    Vec::new(),
                );
            }
            let (status, failure) = if error.kind() == std::io::ErrorKind::PermissionDenied {
                (
                    DeviceStatus::PermissionDenied,
                    FailureKind::PermissionDenied,
                )
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
                    provider: IIO_DISCOVERY_PROVIDER,
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
        let name = read_trimmed(directory.join("name"))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "unknown-iio".to_owned());
        let (device_identity, identity_is_stable) =
            sensor_device_identity_with_quality(&directory, &name);
        if !identity_is_stable {
            enumeration_failure = Some(stronger_failure(
                enumeration_failure,
                FailureKind::Unsupported,
            ));
        }
        let device_id = DeviceId::new(format!("iio:{device_identity}:{name}"));
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
            let Some(channel) = parse_iio_channel(&input_name) else {
                continue;
            };
            let Some(descriptor) = finalize_descriptor(&channel, &directory) else {
                continue;
            };
            let label = read_trimmed(
                directory.join(format!("{}{}_label", channel.type_token, channel.channel)),
            )
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| format!("{name} {}{}", channel.type_token, channel.channel));
            readings.push(read_sensor_with(
                device_id.clone(),
                format!("{}:{}", device_id.as_str(), channel_identity(&channel)),
                label,
                descriptor,
                std::fs::read_to_string(sensor_entry.path()),
                now_ms,
                iio_parse_magnitude,
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
            provider: IIO_DISCOVERY_PROVIDER,
            outcome: discovery_outcome,
            item_count: discovered_count,
        },
        vec![SourceStatus {
            provider: IIO_READING_PROVIDER,
            outcome: reading_outcome,
            item_count: successful_readings,
        }],
    )
}

/// Reading id shape: `voltage0`, `accel_x`, `voltage0_x`.
fn channel_identity(channel: &IioChannel) -> String {
    format!("{}{}", channel.type_token, channel.channel)
}

/// Resolve the device-specific scale: per-channel `_scale`, then shared
/// `in_<type>_scale`. A known quantity without a parseable scale degrades to an
/// opaque raw reading instead of fabricating a scale.
fn finalize_descriptor(channel: &IioChannel, directory: &Path) -> Option<SensorDescriptor> {
    let (quantity, unit) = match channel.descriptor.quantity() {
        SensorQuantity::Opaque(_) => return Some(channel.descriptor.clone()),
        quantity => (quantity.clone(), channel.descriptor.unit().clone()),
    };
    let mut resolved = None;
    if let Some(per_channel) = channel.scale_file.as_deref()
        && let Some(text) = read_trimmed(directory.join(per_channel))
    {
        resolved = scale_from_text(&text);
    }
    if resolved.is_none() {
        let shared = format!("in_{}_scale", channel.type_token);
        if shared != channel.scale_file.clone().unwrap_or_default()
            && let Some(text) = read_trimmed(directory.join(&shared))
        {
            resolved = scale_from_text(&text);
        }
    }
    match resolved {
        Some(scale) => SensorDescriptor::try_new(quantity, unit, Some(scale)).ok(),
        None => SensorDescriptor::opaque(
            format!("iio_{}", channel.type_token),
            SensorUnit::Opaque(format!("raw_iio_{}", channel.type_token)),
            Some(SensorScale::IDENTITY),
        )
        .ok(),
    }
}

fn scale_from_text(text: &str) -> Option<SensorScale> {
    let (numerator, denominator) = parse_decimal_scale(text)?;
    SensorScale::ratio(numerator, denominator).ok()
}

/// IIO `_raw` samples are signed integers; scaling is applied from `_scale`.
fn iio_parse_magnitude(_descriptor: &SensorDescriptor, raw: &str) -> Option<SensorMagnitude> {
    raw.trim().parse::<i64>().ok().map(SensorMagnitude::Signed)
}

/// One classified IIO sample channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IioChannel {
    /// Channel type token (`voltage`, `temp`, `accel`, …).
    pub type_token: String,
    /// Index or axis identity used for stable reading ids and labels.
    pub channel: String,
    pub descriptor: SensorDescriptor,
    /// Per-channel `_scale` file to probe first (stem minus `_raw` plus
    /// `_scale`), then the shared `in_<type>_scale` fallback.
    pub scale_file: Option<String>,
}

/// Parse a `_raw` file name into a channel. Non-channel files
/// (`in_timestamp_raw`, `*_scale`, `*_offset`, `name`, …) yield `None`.
pub(super) fn parse_iio_channel(name: &str) -> Option<IioChannel> {
    if name == "in_timestamp_raw" {
        return None;
    }
    let stem = name.strip_suffix("_raw")?.strip_prefix("in_")?;
    let type_len = stem
        .char_indices()
        .find(|(_, character)| !character.is_ascii_lowercase())
        .map_or(stem.len(), |(index, _)| index);
    let (type_token, rest) = stem.split_at(type_len);
    if type_token.is_empty() {
        return None;
    }
    let digits = rest
        .char_indices()
        .take_while(|(_, character)| character.is_ascii_digit())
        .map(|(index, _)| index)
        .last()
        .map_or("", |last| &rest[..=last]);
    let axis = rest[digits.len()..].strip_prefix('_');
    let (channel, scale_file) = match (digits.is_empty(), axis) {
        (true, Some(axis)) => (format!("_{axis}"), format!("in_{type_token}_{axis}_scale")),
        (false, Some(axis)) => (
            format!("{digits}_{axis}"),
            format!("in_{type_token}{digits}_{axis}_scale"),
        ),
        (false, None) => (digits.to_owned(), format!("in_{type_token}{digits}_scale")),
        (true, None) => return None,
    };

    let descriptor = match type_token {
        "temp" => SensorDescriptor::temperature(scale_for(type_token)),
        "voltage" => SensorDescriptor::voltage(scale_for(type_token)),
        "current" => SensorDescriptor::current(scale_for(type_token)),
        "power" => SensorDescriptor::power(scale_for(type_token)),
        "energy" => SensorDescriptor::energy(scale_for(type_token)),
        "humidityrelative" => SensorDescriptor::relative_humidity(scale_for(type_token)),
        _ => SensorDescriptor::opaque(
            format!("iio_{type_token}"),
            SensorUnit::Opaque(format!("raw_iio_{type_token}")),
            Some(SensorScale::IDENTITY),
        )
        .ok()?,
    };
    Some(IioChannel {
        type_token: type_token.to_owned(),
        channel: channel.to_owned(),
        descriptor,
        scale_file: Some(scale_file),
    })
}

/// Known quantities still need a real scale from the device; the provisional
/// `IDENTITY` placeholder is replaced during collection when `_scale` exists,
/// and the channel degrades to opaque otherwise.
const fn scale_for(_type_token: &str) -> SensorScale {
    SensorScale::IDENTITY
}

/// Parse an IIO `_scale` decimal into an exact rational scale. Supports plain
/// decimals and `e`-notation with positive magnitudes only; anything else
/// (negative, zero, non-numeric, overflow) yields `None` so the caller falls
/// back to an opaque raw reading rather than fabricating a scale.
pub(super) fn parse_decimal_scale(text: &str) -> Option<(u64, u64)> {
    let text = text.trim();
    if text.is_empty() || text.starts_with('-') {
        return None;
    }
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(index) => (&text[..index], text[index + 1..].parse::<i32>().ok()?),
        None => (text, 0),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (mantissa, ""),
    };
    if !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{whole}{fraction}");
    if digits.bytes().all(|byte| byte == b'0') {
        return None;
    }
    let fraction_len = fraction.len() as i32 - exponent;
    if fraction_len >= 0 {
        let numerator = digits.parse::<u64>().ok()?;
        let denominator = 10_u64.checked_pow(fraction_len as u32)?;
        Some((numerator, denominator))
    } else {
        let numerator = digits.parse::<u64>().ok()?;
        let numerator = numerator.checked_mul(10_u64.checked_pow((-fraction_len) as u32)?)?;
        Some((numerator, 1))
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_sensors_iio_tests.rs"]
mod tests;
