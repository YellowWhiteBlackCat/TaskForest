//! Controlled power samples enter the normal correlated history writer.
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::identity::DeviceGeneration;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::power::{BatteryInfo, BatteryScalarObservations, PowerSupplySnapshot};
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture::{
    ProjectionSeedFact, record_demo_history_frame, seed_projection_fact,
};

pub(super) fn seed(shell: &mut ShellApp) {
    let Some(mut system) = shell.projection().snapshot.clone() else {
        return;
    };
    let anchor = system.timestamp_ms;
    for index in 0..8_u8 {
        let now = anchor.saturating_add(u64::from(index) + 1);
        let mut battery =
            BatteryInfo::new("power-supply:capture-battery", DeviceState::healthy(now));
        battery.device_generation = DeviceGeneration::new(1);
        battery.display_name = "Internal battery".into();
        battery.model_name = "Capture Battery".into();
        battery.status = "Discharging".into();
        battery.technology = "Li-ion".into();
        battery.manufacturer = "TaskForest".into();
        battery.apply_scalar_observations(BatteryScalarObservations {
            capacity_pct: ScalarObservation::available(85 - index, now),
            voltage_uv: ScalarObservation::available(12_100_000, now),
            power_w: ScalarObservation::available(12.8 + f32::from(index) * 0.2, now),
            cycle_count: ScalarObservation::available(142, now),
            ..Default::default()
        });
        let power = PowerSupplySnapshot {
            state: DeviceState::healthy(now),
            timestamp_ms: now,
            batteries: vec![battery],
            ..Default::default()
        };
        system.timestamp_ms = now;
        record_demo_history_frame(shell, &system, Some(&power), None);
        seed_projection_fact(shell, ProjectionSeedFact::PowerSupplies(Some(power)));
    }
}
