//! Platform-neutral power-supply snapshots and hot-plug lifecycle.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::core::device_state::{
    DeviceLifecycle, DeviceLifecycleDelta, DeviceLifecycleRegistry, DeviceRefreshOutcome,
    DeviceState,
};
use crate::core::metrics::{ScalarAvailability, ScalarObservation};
use crate::core::{DeviceGeneration, DeviceStatus, FailureKind};

/// Portable classification for stored-energy power supplies.
///
/// The existing `BatteryInfo`/`batteries` wire names are retained for schema
/// compatibility, but providers may also publish an uninterruptible power
/// supply through the same lifecycle and scalar contract. Native transport
/// names (for example Linux power-supply `type` strings) stay at the adapter
/// boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PowerSupplyKind {
    #[default]
    Battery,
    UninterruptiblePowerSupply,
    /// Battery-backed peripheral discovered through a native device service.
    PeripheralBattery,
    /// A stored-energy supply outside the shared baseline taxonomy.
    Other,
}

/// Independently fallible numeric observations for one power supply.
///
/// This is the only canonical scalar truth. Historical `Option` fields exist
/// only in the private `BatteryInfoWire` compatibility boundary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct BatteryScalarObservations {
    pub capacity_pct: ScalarObservation<u8>,
    pub voltage_uv: ScalarObservation<u64>,
    pub power_w: ScalarObservation<f32>,
    pub cycle_count: ScalarObservation<u32>,
    /// Energy the pack holds when full (µWh) — the degradation-health
    /// numerator. `serde(default)` keeps payloads written before this field
    /// decodable as `Unknown` (hidden by every current-value reader).
    #[serde(default)]
    pub energy_full_uwh: ScalarObservation<f64>,
    /// Energy the pack was designed to hold when full (µWh) — the
    /// degradation-health denominator. See [`Self::health_pct`].
    #[serde(default)]
    pub energy_full_design_uwh: ScalarObservation<f64>,
    /// Remaining-until-empty estimate in seconds. Filled ONLY when the native
    /// source reports an estimate while the supply logically discharges;
    /// when the source provides none (status `Full`/`Not charging`/`Unknown`,
    /// or no estimate node exists) this stays Unavailable — never a
    /// fabricated `0`.
    #[serde(default)]
    pub time_to_empty_secs: ScalarObservation<f64>,
    /// Remaining-until-full estimate in seconds, with the mirrored status
    /// invariant: reported only while charging, Unavailable otherwise.
    #[serde(default)]
    pub time_to_full_secs: ScalarObservation<f64>,
}

impl BatteryScalarObservations {
    /// Degradation health as `energy_full / energy_full_design × 100`.
    ///
    /// This is the single pure rule — providers report the two µWh facts and
    /// never compute health themselves. Unavailable when either input is not
    /// current or the design capacity is zero/non-finite: an honest absence,
    /// never a fabricated 0% or 100%. A partial input degrades the result to
    /// `Partial` with that input's failure; the success time is the older of
    /// the two inputs (the ratio is only as fresh as both facts).
    #[must_use]
    pub fn health_pct(&self) -> ScalarObservation<f64> {
        let full = self.energy_full_uwh;
        let design = self.energy_full_design_uwh;
        let (Some(full_uwh), Some(design_uwh)) = (
            full.current_value().copied(),
            design.current_value().copied(),
        ) else {
            return ScalarObservation::unavailable(missing_ratio_input_failure(
                design.availability(),
                full.availability(),
            ));
        };
        let health = full_uwh / design_uwh * 100.0;
        if !design_uwh.is_finite() || design_uwh <= 0.0 || !health.is_finite() {
            return ScalarObservation::unavailable(FailureKind::ProviderFault);
        }
        let observed_at_ms = full
            .last_success_ms()
            .unwrap_or_default()
            .min(design.last_success_ms().unwrap_or_default());
        match (full.availability(), design.availability()) {
            (ScalarAvailability::Available, ScalarAvailability::Available) => {
                ScalarObservation::available(health, observed_at_ms)
            }
            _ => ScalarObservation::partial(
                health,
                observed_at_ms,
                full.availability()
                    .failure()
                    .or(design.availability().failure())
                    .unwrap_or(FailureKind::ProviderFault),
            ),
        }
    }

    #[must_use]
    fn retain_previous(self, previous: Self) -> Self {
        Self {
            capacity_pct: self.capacity_pct.retain_previous(previous.capacity_pct),
            voltage_uv: self.voltage_uv.retain_previous(previous.voltage_uv),
            power_w: self.power_w.retain_previous(previous.power_w),
            cycle_count: self.cycle_count.retain_previous(previous.cycle_count),
            energy_full_uwh: self
                .energy_full_uwh
                .retain_previous(previous.energy_full_uwh),
            energy_full_design_uwh: self
                .energy_full_design_uwh
                .retain_previous(previous.energy_full_design_uwh),
            time_to_empty_secs: self
                .time_to_empty_secs
                .retain_previous(previous.time_to_empty_secs),
            time_to_full_secs: self
                .time_to_full_secs
                .retain_previous(previous.time_to_full_secs),
        }
    }
}

/// Failure carried by a derived ratio whose inputs are not current. The
/// denominator (design) reason wins — a missing denominator is the primary
/// cause; inputs that were merely never observed decode as `Unsupported`.
fn missing_ratio_input_failure(
    design: ScalarAvailability,
    full: ScalarAvailability,
) -> FailureKind {
    design
        .failure()
        .or(full.failure())
        .unwrap_or(FailureKind::Unsupported)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BatteryInfo {
    /// Stable platform identity, independent from display strings.
    pub id: String,
    /// Portable stored-energy supply category. Defaults to `Battery` when
    /// decoding schema-v1 payloads written before this distinction existed.
    pub kind: PowerSupplyKind,
    /// Provider-supplied display label; never used for lifecycle identity.
    pub display_name: String,
    /// Advances only after confirmed absence followed by reappearance.
    pub device_generation: DeviceGeneration,
    pub device_state: DeviceState,
    /// "Charging" / "Discharging" / "Full" / "Not charging" / "Unknown".
    pub status: String,
    pub technology: String,
    pub model_name: String,
    pub manufacturer: String,
    scalar_observations: BatteryScalarObservations,
}

impl BatteryInfo {
    /// Construct one identified power-supply row before applying its typed
    /// scalar assembly. Descriptive fields may be filled by the provider;
    /// scalar truth enters only through [`Self::apply_scalar_observations`].
    #[must_use]
    pub fn new(id: impl Into<String>, device_state: DeviceState) -> Self {
        Self {
            id: id.into(),
            device_state,
            ..Self::default()
        }
    }

    /// Read-only access to the canonical scalar group.
    #[must_use]
    pub const fn scalar_observations(&self) -> &BatteryScalarObservations {
        &self.scalar_observations
    }

    #[must_use]
    pub const fn current_capacity_pct(&self) -> Option<u8> {
        self.scalar_observations
            .capacity_pct
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_voltage_uv(&self) -> Option<u64> {
        self.scalar_observations.voltage_uv.current_value().copied()
    }

    #[must_use]
    pub const fn current_power_w(&self) -> Option<f32> {
        self.scalar_observations.power_w.current_value().copied()
    }

    #[must_use]
    pub const fn current_cycle_count(&self) -> Option<u32> {
        self.scalar_observations
            .cycle_count
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_energy_full_uwh(&self) -> Option<f64> {
        self.scalar_observations
            .energy_full_uwh
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_energy_full_design_uwh(&self) -> Option<f64> {
        self.scalar_observations
            .energy_full_design_uwh
            .current_value()
            .copied()
    }

    /// Current degradation health through the pure
    /// [`BatteryScalarObservations::health_pct`] rule (full/design × 100),
    /// or `None` when either fact is missing.
    #[must_use]
    pub fn current_health_pct(&self) -> Option<f64> {
        self.scalar_observations
            .health_pct()
            .current_value()
            .copied()
    }

    /// Native remaining-until-empty estimate in seconds, or `None` when the
    /// source reported none (see the status-gating invariant on
    /// [`BatteryScalarObservations::time_to_empty_secs`]).
    #[must_use]
    pub fn current_time_to_empty_secs(&self) -> Option<f64> {
        self.scalar_observations
            .time_to_empty_secs
            .current_value()
            .copied()
    }

    /// Native remaining-until-full estimate in seconds, or `None` when the
    /// source reported none.
    #[must_use]
    pub fn current_time_to_full_secs(&self) -> Option<f64> {
        self.scalar_observations
            .time_to_full_secs
            .current_value()
            .copied()
    }

    /// Replace the canonical scalar truth in one operation.
    pub fn apply_scalar_observations(&mut self, observations: BatteryScalarObservations) {
        self.scalar_observations = observations;
    }

    fn retain_previous_scalars(&mut self, previous: &Self) {
        self.apply_scalar_observations(
            self.scalar_observations
                .retain_previous(previous.scalar_observations),
        );
    }
}

/// Compatibility-only JSON shape. The four historical scalar options never
/// become writable fields on `BatteryInfo`.
#[derive(Serialize, Deserialize)]
struct BatteryInfoWire {
    id: String,
    #[serde(default)]
    kind: PowerSupplyKind,
    #[serde(default)]
    display_name: String,
    device_generation: DeviceGeneration,
    device_state: DeviceState,
    status: String,
    #[serde(default)]
    capacity_pct: Option<u8>,
    #[serde(default)]
    voltage_uv: Option<u64>,
    #[serde(default)]
    power_w: Option<f32>,
    #[serde(default)]
    technology: String,
    #[serde(default)]
    cycle_count: Option<u32>,
    #[serde(default)]
    model_name: String,
    #[serde(default)]
    manufacturer: String,
    #[serde(default)]
    scalar_observations: BatteryScalarObservations,
}

impl Serialize for BatteryInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        BatteryInfoWire {
            id: self.id.clone(),
            kind: self.kind,
            display_name: self.display_name.clone(),
            device_generation: self.device_generation,
            device_state: self.device_state,
            status: self.status.clone(),
            capacity_pct: legacy_scalar_projection(&self.scalar_observations.capacity_pct),
            voltage_uv: legacy_scalar_projection(&self.scalar_observations.voltage_uv),
            power_w: legacy_scalar_projection(&self.scalar_observations.power_w),
            technology: self.technology.clone(),
            cycle_count: legacy_scalar_projection(&self.scalar_observations.cycle_count),
            model_name: self.model_name.clone(),
            manufacturer: self.manufacturer.clone(),
            scalar_observations: self.scalar_observations,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BatteryInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = BatteryInfoWire::deserialize(deserializer)?;
        let legacy_success_ms = trustworthy_legacy_success_ms(&wire.id, wire.device_state);
        let scalar_observations = BatteryScalarObservations {
            capacity_pct: hydrate_legacy_scalar(
                wire.scalar_observations.capacity_pct,
                wire.capacity_pct,
                legacy_success_ms,
            ),
            voltage_uv: hydrate_legacy_scalar(
                wire.scalar_observations.voltage_uv,
                wire.voltage_uv,
                legacy_success_ms,
            ),
            power_w: hydrate_legacy_scalar(
                wire.scalar_observations.power_w,
                wire.power_w,
                legacy_success_ms,
            ),
            cycle_count: hydrate_legacy_scalar(
                wire.scalar_observations.cycle_count,
                wire.cycle_count,
                legacy_success_ms,
            ),
            energy_full_uwh: wire.scalar_observations.energy_full_uwh,
            energy_full_design_uwh: wire.scalar_observations.energy_full_design_uwh,
            time_to_empty_secs: wire.scalar_observations.time_to_empty_secs,
            time_to_full_secs: wire.scalar_observations.time_to_full_secs,
        };
        Ok(Self {
            id: wire.id,
            kind: wire.kind,
            display_name: wire.display_name,
            device_generation: wire.device_generation,
            device_state: wire.device_state,
            status: wire.status,
            technology: wire.technology,
            model_name: wire.model_name,
            manufacturer: wire.manufacturer,
            scalar_observations,
        })
    }
}

const fn trustworthy_legacy_success_ms(id: &str, device_state: DeviceState) -> Option<u64> {
    if !id.is_empty() && matches!(device_state.status, DeviceStatus::Healthy) {
        device_state.last_success_ms
    } else {
        None
    }
}

const fn legacy_scalar_projection<T: Copy>(observation: &ScalarObservation<T>) -> Option<T> {
    if matches!(observation.availability(), ScalarAvailability::Available) {
        observation.current_value().copied()
    } else {
        None
    }
}

fn hydrate_legacy_scalar<T: Copy>(
    observation: ScalarObservation<T>,
    legacy: Option<T>,
    last_success_ms: Option<u64>,
) -> ScalarObservation<T> {
    match (observation.availability(), legacy, last_success_ms) {
        (ScalarAvailability::Unknown, Some(value), Some(observed_at_ms)) => {
            ScalarObservation::available(value, observed_at_ms)
        }
        _ => observation,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PowerSupplySnapshot {
    pub state: DeviceState,
    pub timestamp_ms: u64,
    pub batteries: Vec<BatteryInfo>,
    /// Retained physical-device lifecycle records keyed by `BatteryInfo::id`.
    #[serde(default)]
    pub device_lifecycles: HashMap<String, DeviceLifecycle>,
}

#[derive(Debug, Clone)]
pub struct PowerSupplyLifecycleTracker {
    registry: DeviceLifecycleRegistry,
    center_state: DeviceState,
    previous_batteries: HashMap<String, BatteryInfo>,
}

impl PowerSupplyLifecycleTracker {
    #[must_use]
    pub fn new(retention_ms: u64) -> Self {
        Self {
            registry: DeviceLifecycleRegistry::new(retention_ms),
            center_state: DeviceState::default(),
            previous_batteries: HashMap::new(),
        }
    }

    pub fn reconcile(
        &mut self,
        snapshot: &mut PowerSupplySnapshot,
        outcome: DeviceRefreshOutcome,
    ) -> DeviceLifecycleDelta {
        let discovered_devices = snapshot
            .batteries
            .iter()
            .map(|battery| crate::core::DeviceId::new(battery.id.clone()))
            .collect::<Vec<_>>();
        self.reconcile_discovered(snapshot, &discovered_devices, outcome)
    }

    /// Reconcile values against the discovery provider's explicit identity
    /// list. Retained stale rows cannot keep a removed battery present.
    pub fn reconcile_discovered(
        &mut self,
        snapshot: &mut PowerSupplySnapshot,
        discovered_devices: &[crate::core::DeviceId],
        outcome: DeviceRefreshOutcome,
    ) -> DeviceLifecycleDelta {
        let now_ms = snapshot.timestamp_ms;
        self.center_state = self.center_state.merge_observation(snapshot.state, now_ms);
        snapshot.state = self.center_state;

        self.registry.begin_refresh();
        let discovered_devices = discovered_devices
            .iter()
            .map(|id| id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut observed_batteries = std::collections::HashSet::new();
        for battery in &mut snapshot.batteries {
            if discovered_devices.contains(battery.id.as_str()) {
                observed_batteries.insert(battery.id.clone());
                if let Some(previous) = self.previous_batteries.get(&battery.id) {
                    battery.retain_previous_scalars(previous);
                }
                let lifecycle =
                    self.registry
                        .observe(battery.id.clone(), battery.device_state, now_ms);
                battery.device_generation = lifecycle.generation;
                battery.device_state = lifecycle.state;
            }
        }
        for device_id in &discovered_devices {
            if !observed_batteries.contains(*device_id) {
                self.registry
                    .observe((*device_id).to_owned(), snapshot.state, now_ms);
            }
        }
        let delta = self.registry.finish_refresh(outcome, now_ms);
        for device_id in delta.newly_absent.iter().chain(&delta.expired) {
            self.previous_batteries.remove(device_id.as_str());
        }
        for battery in &mut snapshot.batteries {
            if let Some(lifecycle) = self.registry.get(&battery.id) {
                battery.device_generation = lifecycle.generation;
                battery.device_state = lifecycle.state;
                self.previous_batteries
                    .insert(battery.id.clone(), battery.clone());
            }
        }
        snapshot.device_lifecycles = self
            .registry
            .iter()
            .map(|(id, lifecycle)| (id.to_owned(), *lifecycle))
            .collect();
        delta
    }

    #[must_use]
    pub fn lifecycle(&self, device_id: &str) -> Option<&DeviceLifecycle> {
        self.registry.get(device_id)
    }
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_power_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/core_core_power_power_gap_tests.rs"]
mod power_gap_tests;
