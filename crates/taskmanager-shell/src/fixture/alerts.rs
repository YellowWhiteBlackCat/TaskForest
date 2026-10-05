//! Deterministic active alerts produced by the normal shared evaluator.

use taskmanager_core::core::metrics::ScalarObservation;

use crate::ShellApp;
use crate::fixture::{ProjectionSeedFact, seed_projection_fact};

/// Cross the default CPU rule's duration floor with two observed frames.
/// No notification is submitted and no renderer-owned alert is manufactured.
pub fn seed_shell_active_alert(app: &mut ShellApp) -> bool {
    let Some(mut snapshot) = app.projection().snapshot.clone() else {
        return false;
    };
    for elapsed in [0, 10_000] {
        snapshot.timestamp_ms = snapshot.timestamp_ms.saturating_add(elapsed);
        let mut observations = snapshot.cpu.scalar_observations().clone();
        observations.global_usage_pct = ScalarObservation::available(94.0, snapshot.timestamp_ms);
        snapshot.cpu.apply_scalar_observations(observations);
        let evaluation = app.evaluate_alerts(&snapshot, snapshot.timestamp_ms);
        seed_projection_fact(app, ProjectionSeedFact::ActiveAlerts(evaluation.active));
    }
    seed_projection_fact(app, ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))));
    !app.projection().alert_active.is_empty()
}
