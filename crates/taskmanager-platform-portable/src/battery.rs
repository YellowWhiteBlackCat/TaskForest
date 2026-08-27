//! Shared battery snapshot assembly through the safe cross-platform
//! `starship_battery` crate (the maintained continuation of the archived
//! `battery` 0.7 crate). Native adapters supply only their identity namespace
//! and provider ID.

use std::collections::HashSet;

use taskmanager_core::{
    BatteryInfo, BatteryScalarObservations, DeviceId, DeviceState, DeviceStatus, FailureKind,
    PowerSupplyKind, PowerSupplySnapshot, ProviderId, ScalarObservation,
};
use taskmanager_platform_contract::{DeviceDiscovery, DeviceSourceSnapshot, ProviderFailure};

pub fn collect_battery_snapshot(
    identity_namespace: &str,
    provider: ProviderId,
    observed_at_ms: u64,
) -> Result<DeviceSourceSnapshot<PowerSupplySnapshot>, ProviderFailure> {
    let manager =
        starship_battery::Manager::new().map_err(|_| ProviderFailure::TemporarilyUnavailable)?;
    let mut batteries = Vec::new();
    let mut discovered = Vec::new();
    let mut identities = HashSet::<String>::new();
    let mut entry_failures = 0_usize;
    let mut weak_identities = 0_usize;
    let mut ambiguous_identities = 0_usize;
    for battery in manager
        .batteries()
        .map_err(|_| ProviderFailure::TemporarilyUnavailable)?
    {
        let battery = match battery {
            Ok(battery) => battery,
            Err(_) => {
                entry_failures += 1;
                continue;
            }
        };
        let serial = battery
            .serial_number()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let identity = serial.map_or_else(
            || {
                weak_identities += 1;
                format!(
                    "{}:{}:{:?}",
                    battery.vendor().unwrap_or("unknown-vendor"),
                    battery.model().unwrap_or("unknown-model"),
                    battery.technology()
                )
            },
            ToOwned::to_owned,
        );
        if !identities.insert(identity.clone()) {
            ambiguous_identities += 1;
            continue;
        }
        let id = format!("{identity_namespace}:battery:{identity}");
        let capacity_pct = battery
            .state_of_charge()
            .get::<starship_battery::units::ratio::percent>();
        let voltage_v = battery
            .voltage()
            .get::<starship_battery::units::electric_potential::volt>();
        let power_w = battery
            .energy_rate()
            .get::<starship_battery::units::power::watt>();
        // Degradation facts in Wh; a non-finite quantity is the crate's
        // "driver did not report it" signal, not a measurement. The crate's
        // quantities are f32 — widened losslessly to the shared f64 fact
        // axis here.
        let energy_full_wh = f64::from(
            battery
                .energy_full()
                .get::<starship_battery::units::energy::watt_hour>(),
        );
        let energy_full_design_wh = f64::from(
            battery
                .energy_full_design()
                .get::<starship_battery::units::energy::watt_hour>(),
        );
        // Runtime estimates: the crate reports `None` whenever the native
        // source has no estimate (including its own Charging/Discharging
        // gating), so `None` maps to typed unavailability — never zero.
        let time_to_full_secs = battery
            .time_to_full()
            .map(|estimate| f64::from(estimate.get::<starship_battery::units::time::second>()));
        let time_to_empty_secs = battery
            .time_to_empty()
            .map(|estimate| f64::from(estimate.get::<starship_battery::units::time::second>()));

        let mut row = BatteryInfo::new(id.clone(), DeviceState::healthy(observed_at_ms));
        row.kind = PowerSupplyKind::Battery;
        row.display_name = battery.model().unwrap_or("Battery").to_string();
        row.device_generation = taskmanager_core::DeviceGeneration::INITIAL;
        row.status = status_label(battery.state()).to_string();
        row.technology = format!("{:?}", battery.technology());
        row.model_name = battery.model().unwrap_or_default().to_string();
        row.manufacturer = battery.vendor().unwrap_or_default().to_string();
        row.apply_scalar_observations(BatteryScalarObservations {
            capacity_pct: percent_observation(capacity_pct, observed_at_ms),
            voltage_uv: voltage_observation(voltage_v, observed_at_ms),
            power_w: finite_nonnegative_observation(power_w, observed_at_ms),
            cycle_count: battery.cycle_count().map_or_else(
                || ScalarObservation::unavailable(FailureKind::Unsupported),
                |count| ScalarObservation::available(count, observed_at_ms),
            ),
            energy_full_uwh: watt_hours_observation(energy_full_wh, observed_at_ms),
            energy_full_design_uwh: watt_hours_observation(energy_full_design_wh, observed_at_ms),
            time_to_full_secs: estimate_observation(time_to_full_secs, observed_at_ms),
            time_to_empty_secs: estimate_observation(time_to_empty_secs, observed_at_ms),
        });
        discovered.push(DeviceId::new(id));
        batteries.push(row);
    }

    let discovery_failure = if entry_failures > 0 {
        Some(FailureKind::ProviderFault)
    } else if weak_identities > 0 || ambiguous_identities > 0 {
        Some(FailureKind::Unsupported)
    } else {
        None
    };
    let discovery = match (batteries.is_empty(), discovery_failure) {
        (true, Some(failure)) => DeviceDiscovery::Unavailable(failure),
        (true, None) => DeviceDiscovery::Empty,
        (false, Some(failure)) => DeviceDiscovery::Partial {
            discovered_devices: discovered,
            failure,
        },
        (false, None) => DeviceDiscovery::Available(discovered),
    };
    let state = match &discovery {
        DeviceDiscovery::Available(_) | DeviceDiscovery::Empty => {
            DeviceState::healthy(observed_at_ms)
        }
        DeviceDiscovery::Partial { .. } => DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: Some(observed_at_ms),
        },
        DeviceDiscovery::Unavailable(_) => DeviceState::default(),
    };
    Ok(DeviceSourceSnapshot::from_discovery(
        PowerSupplySnapshot {
            state,
            timestamp_ms: observed_at_ms,
            batteries,
            device_lifecycles: Default::default(),
        },
        provider,
        discovery,
        Vec::new(),
    ))
}

fn status_label(state: starship_battery::State) -> &'static str {
    match state {
        starship_battery::State::Charging => "Charging",
        starship_battery::State::Discharging => "Discharging",
        starship_battery::State::Full => "Full",
        starship_battery::State::Empty => "Empty",
        _ => "Unknown",
    }
}

fn percent_observation(value: f32, observed_at_ms: u64) -> ScalarObservation<u8> {
    if value.is_finite() && (0.0..=100.0).contains(&value) {
        ScalarObservation::available(value.round() as u8, observed_at_ms)
    } else {
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    }
}

fn voltage_observation(value_volts: f32, observed_at_ms: u64) -> ScalarObservation<u64> {
    let microvolts = value_volts as f64 * 1_000_000.0;
    if microvolts.is_finite() && (0.0..=u64::MAX as f64).contains(&microvolts) {
        ScalarObservation::available(microvolts.round() as u64, observed_at_ms)
    } else {
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    }
}

fn finite_nonnegative_observation(value: f32, observed_at_ms: u64) -> ScalarObservation<f32> {
    if value.is_finite() && value >= 0.0 {
        ScalarObservation::available(value, observed_at_ms)
    } else {
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    }
}

/// Wh quantity from the `starship_battery` crate (non-`Option`, so a
/// non-finite or negative value is the driver's "not reported" signal)
/// widened to the shared µWh fact axis.
fn watt_hours_observation(value_wh: f64, observed_at_ms: u64) -> ScalarObservation<f64> {
    let microwatt_hours = value_wh * 1_000_000.0;
    if microwatt_hours.is_finite() && microwatt_hours >= 0.0 {
        ScalarObservation::available(microwatt_hours, observed_at_ms)
    } else {
        ScalarObservation::unavailable(FailureKind::Unsupported)
    }
}

/// Native runtime estimate in seconds. `None` means the source (or its own
/// status gating) reported no estimate — typed unavailability, never zero.
fn estimate_observation(value_secs: Option<f64>, observed_at_ms: u64) -> ScalarObservation<f64> {
    match value_secs {
        Some(secs) if secs.is_finite() && secs >= 0.0 => {
            ScalarObservation::available(secs, observed_at_ms)
        }
        Some(_) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        None => ScalarObservation::unavailable(FailureKind::Unsupported),
    }
}

#[cfg(test)]
#[path = "../tests/headless/battery.rs"]
mod tests;
