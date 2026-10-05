//! Power capture follows the same correlated history writer as live ingestion.
use super::dynamic_power_fixture;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::power::BatteryScalarObservations;
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
        let mut power = dynamic_power_fixture();
        power.timestamp_ms = now;
        if let Some(battery) = power.batteries.first_mut() {
            battery.apply_scalar_observations(BatteryScalarObservations {
                capacity_pct: ScalarObservation::available(85 - index, now),
                voltage_uv: ScalarObservation::available(12_100_000, now),
                power_w: ScalarObservation::available(12.8 + f32::from(index) * 0.2, now),
                cycle_count: ScalarObservation::available(142, now),
                ..Default::default()
            });
        }
        system.timestamp_ms = now;
        record_demo_history_frame(shell, &system, Some(&power), None);
        seed_projection_fact(shell, ProjectionSeedFact::PowerSupplies(Some(power)));
    }
}
