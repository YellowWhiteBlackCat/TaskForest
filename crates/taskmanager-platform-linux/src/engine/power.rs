//! Linux power-supply discovery with stable identity and typed source status.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::{DeviceId, ProviderId};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{
    BatteryInfo, BatteryScalarObservations, DeviceState, DeviceStatus, PowerSupplyKind,
    PowerSupplySnapshot, ScalarAvailability, ScalarObservation,
};
use taskmanager_platform_contract::{DeviceDiscovery, DeviceSourceSnapshot};

const POWER_SUPPLY_PROVIDER: ProviderId = ProviderId::borrowed("linux.power-supply.sysfs");
const POWER_SUPPLY_SCALAR_PROVIDER: ProviderId = ProviderId::borrowed("linux.power-supply.scalars");
const MAX_PLAUSIBLE_POWER_UW: u64 = 1_000_000_000_000;
const MAX_PLAUSIBLE_CURRENT_UA: u64 = 1_000_000_000;
const MAX_PLAUSIBLE_VOLTAGE_UV: u64 = 1_000_000_000;
/// Energy ceiling (µWh, 1 MWh) covering large UPS packs while rejecting
/// garbage node contents.
const MAX_PLAUSIBLE_ENERGY_UWH: u64 = 1_000_000_000_000;
/// Charge ceiling (µAh, 100 kAh) for the charge-based energy fallback.
const MAX_PLAUSIBLE_CHARGE_UAH: u64 = 100_000_000_000;
/// Runtime-estimate ceiling (whole minutes, ~one year): the kernel reports
/// instant estimates, so anything beyond this is a driver fault, not data.
const MAX_PLAUSIBLE_ESTIMATE_MINS: u64 = 366 * 24 * 60;

pub fn collect_power_supplies(observed_at_ms: u64) -> DeviceSourceSnapshot<PowerSupplySnapshot> {
    collect_power_supplies_from(Path::new("/sys/class/power_supply"), observed_at_ms)
}

pub fn collect_power_supplies_from(
    root: &Path,
    observed_at_ms: u64,
) -> DeviceSourceSnapshot<PowerSupplySnapshot> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            let (status, failure) = match error.kind() {
                ErrorKind::PermissionDenied => (
                    DeviceStatus::PermissionDenied,
                    FailureKind::PermissionDenied,
                ),
                ErrorKind::NotFound => (DeviceStatus::Unsupported, FailureKind::Unsupported),
                _ => (DeviceStatus::Stale, FailureKind::ProviderFault),
            };
            return DeviceSourceSnapshot::from_discovery(
                PowerSupplySnapshot {
                    state: DeviceState {
                        status,
                        last_success_ms: None,
                    },
                    timestamp_ms: observed_at_ms,
                    batteries: Vec::new(),
                    device_lifecycles: Default::default(),
                },
                POWER_SUPPLY_PROVIDER,
                DeviceDiscovery::Unavailable(failure),
                Vec::new(),
            );
        }
    };

    let mut entry_failures = 0usize;
    let mut weak_identities = 0usize;
    let mut batteries = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                entry_failures = entry_failures.saturating_add(1);
                continue;
            }
        };
        if let Some((battery, identity_is_persistent)) = read_battery(
            &entry.path(),
            &entry.file_name().to_string_lossy(),
            observed_at_ms,
        ) {
            weak_identities += usize::from(!identity_is_persistent);
            batteries.push(battery);
        }
    }
    batteries.sort_by(|left, right| left.id.cmp(&right.id));
    let before_dedup = batteries.len();
    batteries.dedup_by(|left, right| left.id == right.id);
    let ambiguous_identities = before_dedup.saturating_sub(batteries.len());
    let scalar_source = power_scalar_source_status(&batteries);
    let discovered_devices = batteries
        .iter()
        .map(|battery| DeviceId::new(battery.id.clone()))
        .collect::<Vec<_>>();
    let outcome = if entry_failures > 0 {
        if batteries.is_empty() {
            SourceOutcome::Unavailable(FailureKind::ProviderFault)
        } else {
            SourceOutcome::Partial(FailureKind::ProviderFault)
        }
    } else if batteries.is_empty() {
        SourceOutcome::Empty
    } else if weak_identities > 0 || ambiguous_identities > 0 {
        SourceOutcome::Partial(FailureKind::Unsupported)
    } else {
        SourceOutcome::Available
    };
    let state = if entry_failures == 0 {
        power_snapshot_state(scalar_source.outcome, observed_at_ms)
    } else {
        DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: None,
        }
    };
    let discovery = match outcome {
        SourceOutcome::Available => DeviceDiscovery::Available(discovered_devices),
        SourceOutcome::Empty => DeviceDiscovery::Empty,
        SourceOutcome::Partial(failure) => DeviceDiscovery::Partial {
            discovered_devices,
            failure,
        },
        SourceOutcome::Unavailable(failure) => DeviceDiscovery::Unavailable(failure),
    };
    DeviceSourceSnapshot::from_discovery(
        PowerSupplySnapshot {
            state,
            timestamp_ms: observed_at_ms,
            batteries,
            device_lifecycles: Default::default(),
        },
        POWER_SUPPLY_PROVIDER,
        discovery,
        vec![scalar_source],
    )
}

fn power_scalar_source_status(batteries: &[BatteryInfo]) -> SourceStatus {
    let mut successful = 0usize;
    let mut total = 0usize;
    let mut failure = None;
    for battery in batteries {
        let observations = battery.scalar_observations();
        // Runtime estimates are mutually exclusive by status (discharging ↔
        // empty, charging ↔ full), so a slot participates only when the
        // status makes it applicable; the inapplicable twin is confirmed
        // not-applicable, not a data gap.
        for availability in [
            Some(observations.capacity_pct.availability()),
            Some(observations.voltage_uv.availability()),
            Some(observations.power_w.availability()),
            Some(observations.cycle_count.availability()),
            Some(observations.energy_full_uwh.availability()),
            Some(observations.energy_full_design_uwh.availability()),
            status_allows_time_to_empty(&battery.status)
                .then(|| observations.time_to_empty_secs.availability()),
            status_allows_time_to_full(&battery.status)
                .then(|| observations.time_to_full_secs.availability()),
        ]
        .into_iter()
        .flatten()
        {
            total = total.saturating_add(1);
            successful = successful.saturating_add(usize::from(availability.is_current()));
            if let Some(candidate) = availability.failure()
                && failure
                    .is_none_or(|current| failure_priority(candidate) > failure_priority(current))
            {
                failure = Some(candidate);
            }
        }
    }
    let outcome = match (successful, total, failure) {
        (0, 0, _) => SourceOutcome::Empty,
        (current, count, None) if current == count => SourceOutcome::Available,
        (current, _, Some(failure)) if current > 0 => SourceOutcome::Partial(failure),
        (_, _, Some(failure)) => SourceOutcome::Unavailable(failure),
        _ => SourceOutcome::Unavailable(FailureKind::ProviderFault),
    };
    SourceStatus {
        provider: POWER_SUPPLY_SCALAR_PROVIDER,
        outcome,
        item_count: successful,
    }
}

fn power_snapshot_state(outcome: SourceOutcome, observed_at_ms: u64) -> DeviceState {
    match outcome {
        SourceOutcome::Available | SourceOutcome::Empty | SourceOutcome::Partial(_) => {
            DeviceState::healthy(observed_at_ms)
        }
        SourceOutcome::Unavailable(FailureKind::PermissionDenied) => DeviceState {
            status: DeviceStatus::PermissionDenied,
            last_success_ms: None,
        },
        SourceOutcome::Unavailable(FailureKind::Unsupported) => DeviceState {
            status: DeviceStatus::Unsupported,
            last_success_ms: None,
        },
        SourceOutcome::Unavailable(_) => DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: None,
        },
    }
}

fn read_battery(dir: &Path, name: &str, observed_at_ms: u64) -> Option<(BatteryInfo, bool)> {
    let supply_type = read_string(&dir.join("type"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let kind = match supply_type.as_str() {
        "battery" => PowerSupplyKind::Battery,
        "ups" => PowerSupplyKind::UninterruptiblePowerSupply,
        _ if name.to_ascii_uppercase().starts_with("BAT") => PowerSupplyKind::Battery,
        _ => return None,
    };

    let serial = read_string(&dir.join("serial_number")).filter(|value| !value.is_empty());
    let manufacturer = read_string(&dir.join("manufacturer")).unwrap_or_default();
    let model_name = read_string(&dir.join("model_name")).unwrap_or_default();
    let id = power_supply_identity(dir, name, serial.as_deref(), &manufacturer, &model_name);
    let capacity_pct = observe_capacity(&dir.join("capacity"), observed_at_ms);
    let voltage_uv = observe_u64(&dir.join("voltage_now"), observed_at_ms);
    let power_w = observe_power(dir, voltage_uv, observed_at_ms);
    let cycle_count = observe_cycle_count(&dir.join("cycle_count"), observed_at_ms);
    let energy_full_uwh = observe_energy(
        dir,
        "energy_full",
        "charge_full",
        voltage_uv,
        observed_at_ms,
    );
    let energy_full_design_uwh = observe_energy(
        dir,
        "energy_full_design",
        "charge_full_design",
        voltage_uv,
        observed_at_ms,
    );
    // Read once, use for both the display field and the estimate gating so
    // the two can never disagree.
    let status = read_string(&dir.join("status")).unwrap_or_else(|| "Unknown".into());
    let time_to_empty_secs = observe_time_estimate(
        &dir.join("time_to_empty_now"),
        status_allows_time_to_empty(&status),
        observed_at_ms,
    );
    let time_to_full_secs = observe_time_estimate(
        &dir.join("time_to_full_now"),
        status_allows_time_to_full(&status),
        observed_at_ms,
    );

    let scalar_observations = BatteryScalarObservations {
        capacity_pct,
        voltage_uv,
        power_w,
        cycle_count,
        energy_full_uwh,
        energy_full_design_uwh,
        time_to_empty_secs,
        time_to_full_secs,
    };
    let identity_is_persistent = serial.is_some();
    let mut battery = BatteryInfo::new(id, DeviceState::healthy(observed_at_ms));
    battery.kind = kind;
    battery.display_name = name.to_string();
    battery.status = status;
    battery.technology = read_string(&dir.join("technology")).unwrap_or_default();
    battery.model_name = model_name;
    battery.manufacturer = manufacturer;
    battery.apply_scalar_observations(scalar_observations);
    Some((battery, identity_is_persistent))
}

fn power_supply_identity(
    dir: &Path,
    name: &str,
    serial: Option<&str>,
    manufacturer: &str,
    model_name: &str,
) -> String {
    if let Some(serial) = serial {
        return format!(
            "power-supply:serial:{}:{}:{serial}",
            manufacturer.trim(),
            model_name.trim()
        );
    }
    fs::canonicalize(dir).map_or_else(
        |_| format!("power-supply:sysfs:{name}"),
        |physical| format!("power-supply:path:{}", physical.to_string_lossy()),
    )
}

fn read_string(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

fn observe_u64(path: &Path, observed_at_ms: u64) -> ScalarObservation<u64> {
    match fs::read_to_string(path) {
        Ok(value) => match value.trim().parse() {
            Ok(value) => ScalarObservation::available(value, observed_at_ms),
            Err(_) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        },
        Err(error) => ScalarObservation::unavailable(classify_scalar_io_error(error.kind())),
    }
}

fn observe_capacity(path: &Path, observed_at_ms: u64) -> ScalarObservation<u8> {
    convert_u64_observation(observe_u64(path, observed_at_ms), observed_at_ms, |value| {
        (value <= 100).then(|| u8::try_from(value).ok()).flatten()
    })
}

fn observe_cycle_count(path: &Path, observed_at_ms: u64) -> ScalarObservation<u32> {
    convert_u64_observation(observe_u64(path, observed_at_ms), observed_at_ms, |value| {
        u32::try_from(value).ok()
    })
}

fn convert_u64_observation<T>(
    observation: ScalarObservation<u64>,
    observed_at_ms: u64,
    convert: impl FnOnce(u64) -> Option<T>,
) -> ScalarObservation<T> {
    let availability = observation.availability();
    match (observation.into_last_known_value(), availability) {
        (Some(value), ScalarAvailability::Available) => convert(value).map_or_else(
            || ScalarObservation::unavailable(FailureKind::ProviderFault),
            |value| ScalarObservation::available(value, observed_at_ms),
        ),
        (_, ScalarAvailability::Unavailable(failure)) => ScalarObservation::unavailable(failure),
        _ => ScalarObservation::unavailable(FailureKind::ProviderFault),
    }
}

/// Status gate for the runtime estimates: `time_to_empty_now` is meaningful
/// only while the supply reports `Discharging`. Case-insensitive to tolerate
/// vendor spellings of the documented kernel states.
fn status_allows_time_to_empty(status: &str) -> bool {
    status.eq_ignore_ascii_case("Discharging")
}

/// Mirrored gate for `time_to_full_now` — meaningful only while `Charging`.
fn status_allows_time_to_full(status: &str) -> bool {
    status.eq_ignore_ascii_case("Charging")
}

/// Full-charge energy (µWh): `energy_full`/`energy_full_design` directly, or
/// the `charge_full`/`charge_full_design` pair (µAh) converted with the live
/// `voltage_now` (falling back to `constant_charge_voltage_now`) so
/// charge-reporting drivers still yield the degradation facts. Missing nodes
/// are typed unavailability, never an error state.
fn observe_energy(
    dir: &Path,
    energy_node: &str,
    charge_node: &str,
    voltage_uv: ScalarObservation<u64>,
    observed_at_ms: u64,
) -> ScalarObservation<f64> {
    let direct = observe_u64(&dir.join(energy_node), observed_at_ms);
    if let Some(microwatt_hours) = direct.current_value().copied() {
        if microwatt_hours <= MAX_PLAUSIBLE_ENERGY_UWH {
            return ScalarObservation::available(microwatt_hours as f64, observed_at_ms);
        }
        return ScalarObservation::unavailable(FailureKind::ProviderFault);
    }

    let direct_failure = scalar_failure(direct);
    let charge_uah = observe_u64(&dir.join(charge_node), observed_at_ms);
    // Prefer the already-read live voltage; constant_charge_voltage_now is
    // the documented fallback for drivers without an instant reading.
    let conversion_voltage = if voltage_uv.current_value().is_some() {
        voltage_uv
    } else {
        observe_u64(&dir.join("constant_charge_voltage_now"), observed_at_ms)
    };
    match (
        charge_uah.current_value().copied(),
        conversion_voltage.current_value().copied(),
    ) {
        (Some(charge_uah), Some(conversion_voltage_uv)) => {
            let fallback = microamp_hours_to_micro_watt_hours(charge_uah, conversion_voltage_uv);
            match fallback {
                Some(value) if direct_failure == FailureKind::Unsupported => {
                    ScalarObservation::available(value, observed_at_ms)
                }
                Some(value) => ScalarObservation::partial(value, observed_at_ms, direct_failure),
                None => ScalarObservation::unavailable(strongest_failure([
                    direct_failure,
                    FailureKind::ProviderFault,
                ])),
            }
        }
        _ => ScalarObservation::unavailable(strongest_failure([
            direct_failure,
            scalar_failure(charge_uah),
            scalar_failure(conversion_voltage),
        ])),
    }
}

/// Convert a charge/voltage pair (µAh × µV) to µWh. u128 math keeps the
/// intermediate exact; the result is bounded by both input plausibility
/// ceilings before the lossless u64 → f64 widening.
fn microamp_hours_to_micro_watt_hours(charge_uah: u64, voltage_uv: u64) -> Option<f64> {
    if charge_uah > MAX_PLAUSIBLE_CHARGE_UAH || voltage_uv > MAX_PLAUSIBLE_VOLTAGE_UV {
        return None;
    }
    let microwatt_hours = u128::from(charge_uah) * u128::from(voltage_uv) / 1_000_000;
    u64::try_from(microwatt_hours)
        .ok()
        .filter(|value| *value <= MAX_PLAUSIBLE_ENERGY_UWH)
        .map(|value| value as f64)
}

/// Native runtime estimate (`time_to_empty_now`/`time_to_full_now`, whole
/// minutes) as seconds. Reported ONLY when the node exists AND the status
/// logically allows it; every other case is typed unavailability — the
/// kernel reporting no estimate is never fabricated as zero.
fn observe_time_estimate(
    path: &Path,
    status_allows: bool,
    observed_at_ms: u64,
) -> ScalarObservation<f64> {
    if !status_allows {
        return ScalarObservation::unavailable(FailureKind::Unsupported);
    }
    let minutes = observe_u64(path, observed_at_ms);
    match minutes.current_value().copied() {
        Some(minutes) if minutes <= MAX_PLAUSIBLE_ESTIMATE_MINS => {
            ScalarObservation::available(minutes as f64 * 60.0, observed_at_ms)
        }
        Some(_) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        None => ScalarObservation::unavailable(scalar_failure(minutes)),
    }
}

fn observe_power(
    dir: &Path,
    voltage_uv: ScalarObservation<u64>,
    observed_at_ms: u64,
) -> ScalarObservation<f32> {
    let direct = observe_u64(&dir.join("power_now"), observed_at_ms);
    if let Some(microwatts) = direct.current_value().copied() {
        return micro_units_to_base(microwatts, MAX_PLAUSIBLE_POWER_UW).map_or_else(
            || ScalarObservation::unavailable(FailureKind::ProviderFault),
            |watts| ScalarObservation::available(watts, observed_at_ms),
        );
    }

    let direct_failure = scalar_failure(direct);
    let current_ua = observe_u64(&dir.join("current_now"), observed_at_ms);
    match (
        current_ua.current_value().copied(),
        voltage_uv.current_value().copied(),
    ) {
        (Some(current_ua), Some(voltage_uv)) => {
            let fallback = micro_units_to_base(current_ua, MAX_PLAUSIBLE_CURRENT_UA)
                .zip(micro_units_to_base(voltage_uv, MAX_PLAUSIBLE_VOLTAGE_UV))
                .map(|(amps, volts)| amps * volts)
                .filter(|watts| watts.is_finite());
            match fallback {
                Some(value) if direct_failure == FailureKind::Unsupported => {
                    ScalarObservation::available(value, observed_at_ms)
                }
                Some(value) => ScalarObservation::partial(value, observed_at_ms, direct_failure),
                None => ScalarObservation::unavailable(strongest_failure([
                    direct_failure,
                    FailureKind::ProviderFault,
                ])),
            }
        }
        _ => ScalarObservation::unavailable(strongest_failure([
            direct_failure,
            scalar_failure(current_ua),
            scalar_failure(voltage_uv),
        ])),
    }
}

fn micro_units_to_base(value: u64, maximum: u64) -> Option<f32> {
    if value > maximum {
        return None;
    }
    let value = value.to_string().parse::<f32>().ok()? / 1_000_000.0;
    value.is_finite().then_some(value)
}

fn scalar_failure<T>(observation: ScalarObservation<T>) -> FailureKind {
    observation
        .availability()
        .failure()
        .unwrap_or(FailureKind::ProviderFault)
}

fn strongest_failure(failures: impl IntoIterator<Item = FailureKind>) -> FailureKind {
    failures
        .into_iter()
        .max_by_key(|failure| failure_priority(*failure))
        .unwrap_or(FailureKind::ProviderFault)
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
        FailureKind::IdentityChanged => 2,
        FailureKind::Rejected => 1,
    }
}

const fn classify_scalar_io_error(kind: ErrorKind) -> FailureKind {
    match kind {
        ErrorKind::NotFound | ErrorKind::Unsupported => FailureKind::Unsupported,
        ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        ErrorKind::Interrupted | ErrorKind::WouldBlock | ErrorKind::TimedOut => {
            FailureKind::TemporarilyUnavailable
        }
        _ => FailureKind::ProviderFault,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_engine_power_tests.rs"]
mod tests;
