//! Linux hwmon ABI channel classification and fixed-point parsing.

use taskmanager_core::{
    SensorDescriptor, SensorMagnitude, SensorQuantity, SensorScale, SensorUnit,
};

const HWMON_PWM_MAXIMUM: u32 = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HwmonChannel {
    pub prefix: String,
    pub channel: String,
    pub descriptor: SensorDescriptor,
}

pub(super) fn parse_channel(name: &str) -> Option<HwmonChannel> {
    if let Some(channel) = indexed_suffix(name, "pwm", "", false) {
        return Some(HwmonChannel {
            prefix: "pwm".into(),
            channel,
            descriptor: SensorDescriptor::pwm_duty_cycle(),
        });
    }
    if let Some(channel) = indexed_suffix(name, "intrusion", "_alarm", true) {
        return Some(HwmonChannel {
            prefix: "intrusion".into(),
            channel,
            descriptor: SensorDescriptor::intrusion(),
        });
    }
    let stem = name.strip_suffix("_input")?;
    for (prefix, descriptor, allow_zero) in [
        (
            "temp",
            SensorDescriptor::temperature(SensorScale::MILLI),
            false,
        ),
        (
            "fan",
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            false,
        ),
        ("power", SensorDescriptor::power(SensorScale::MICRO), false),
        ("in", SensorDescriptor::voltage(SensorScale::MILLI), true),
        ("curr", SensorDescriptor::current(SensorScale::MILLI), false),
        (
            "energy",
            SensorDescriptor::energy(SensorScale::MICRO),
            false,
        ),
        (
            "humidity",
            SensorDescriptor::relative_humidity(SensorScale::MILLI),
            false,
        ),
    ] {
        let Some(index) = stem.strip_prefix(prefix) else {
            continue;
        };
        if let Some(channel) = parse_index(index, allow_zero) {
            return Some(HwmonChannel {
                prefix: prefix.into(),
                channel,
                descriptor,
            });
        }
    }
    let (prefix, channel) = split_opaque_input_stem(stem)?;
    if ["temp", "fan", "power", "in", "curr", "energy", "humidity"].contains(&prefix) {
        return None;
    }
    let descriptor = SensorDescriptor::opaque(
        prefix.to_owned(),
        SensorUnit::Opaque(format!("raw_hwmon_{prefix}")),
        None,
    )
    .ok()?;
    Some(HwmonChannel {
        prefix: prefix.to_owned(),
        channel: channel.to_owned(),
        descriptor,
    })
}

fn indexed_suffix(name: &str, prefix: &str, suffix: &str, allow_zero: bool) -> Option<String> {
    let stem = name.strip_suffix(suffix)?;
    parse_index(stem.strip_prefix(prefix)?, allow_zero)
}

fn parse_index(index: &str, allow_zero: bool) -> Option<String> {
    if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let index_value = index.parse::<u32>().ok()?;
    (allow_zero || index_value > 0).then(|| index.to_owned())
}

fn split_opaque_input_stem(stem: &str) -> Option<(&str, &str)> {
    let digit_start = stem
        .char_indices()
        .rev()
        .find(|(_, character)| !character.is_ascii_digit())
        .map_or(0, |(index, character)| index + character.len_utf8());
    let (prefix, channel) = stem.split_at(digit_start);
    if prefix.is_empty()
        || channel.is_empty()
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || !channel.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    channel.parse::<u32>().ok()?;
    Some((prefix, channel))
}

pub(super) fn parse_magnitude(descriptor: &SensorDescriptor, raw: &str) -> Option<SensorMagnitude> {
    match descriptor.quantity() {
        SensorQuantity::Temperature => raw.parse::<i64>().ok().map(SensorMagnitude::Signed),
        SensorQuantity::FanSpeed
        | SensorQuantity::Power
        | SensorQuantity::Energy
        | SensorQuantity::RelativeHumidity => {
            raw.parse::<u64>().ok().map(SensorMagnitude::Unsigned)
        }
        SensorQuantity::Voltage | SensorQuantity::Current => {
            raw.parse::<i64>().ok().map(SensorMagnitude::Signed)
        }
        SensorQuantity::PwmDutyCycle => raw
            .parse::<u32>()
            .ok()
            .filter(|value| *value <= HWMON_PWM_MAXIMUM)
            .map(|value| SensorMagnitude::DutyCycle {
                value,
                maximum: HWMON_PWM_MAXIMUM,
            }),
        SensorQuantity::Intrusion => match raw.parse::<u8>().ok()? {
            0 => Some(SensorMagnitude::Boolean(false)),
            1 => Some(SensorMagnitude::Boolean(true)),
            _ => None,
        },
        SensorQuantity::Opaque(_) => raw
            .parse::<i64>()
            .ok()
            .map(SensorMagnitude::Signed)
            .or_else(|| raw.parse::<u64>().ok().map(SensorMagnitude::Unsigned)),
        SensorQuantity::Unknown => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_sensors_hwmon_tests.rs"]
mod tests;
