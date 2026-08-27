use crate::core::FailureKind;
use crate::core::metrics::{CpuMetrics, CpuScalarObservations, ScalarObservation};

use super::cpu_summary_readout;

#[test]
fn cpu_readout_formats_current_observation_and_missing_state() {
    let mut cpu = CpuMetrics::default();
    cpu.apply_scalar_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(42.5, 10),
        ..Default::default()
    });
    assert_eq!(cpu_summary_readout(&cpu), "42.5%");

    cpu.apply_scalar_observations(CpuScalarObservations::unavailable(
        FailureKind::PermissionDenied,
    ));
    assert_eq!(cpu_summary_readout(&cpu), "—");
}
