//! test-intent: behavior
//!
//! Behavior tests for CPU cache topology, heterogeneous core classes,
//! per-core frequencies, and RAPL package power on the Bevy Performance page.
//!
//! Split out of `performance.rs` so the line-budget gate keeps both files
//! under the test hard limit.

use taskmanager_core::core::hardware::{CoreBreakdown, CpuType, HardwareInfo};
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, ScalarObservation, ScalarObservationGroup,
};
use taskmanager_shell::fixture;
use taskmanager_shell::presentation::MISSING_VALUE;

use super::tests::{Folded, GIB, cpu_outcome, memory_metrics, projection};
use super::{CpuField, cpu_field_text};

#[test]
fn cpu_cache_topology_heterogeneous_cores_and_core_frequency_render_correctly() {
    let mut folded = Folded::new();
    let at = 1_000;
    let observations = CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(20.0, at),
        core_usage_group: ScalarObservationGroup::available(vec![12.0], at),
        per_core_frequency_group: ScalarObservationGroup::available(vec![3600], at),
        power_w: ScalarObservation::available(65.0, at),
        ..CpuScalarObservations::default()
    };
    let mut cpu = CpuMetrics::from_observations(observations);
    cpu.l1d_cache_kb = Some(1280);
    cpu.l1i_cache_kb = Some(768);
    cpu.l2_cache_kb = Some(16384);
    cpu.l3_cache_kb = Some(32768);

    let memory = memory_metrics(at, 4 * GIB, 16 * GIB, 12 * GIB, (GIB, 4 * GIB));
    folded.apply(
        vec![cpu_outcome(1, at, cpu.clone())],
        projection(1, cpu, memory, Vec::new()),
    );

    let hardware = HardwareInfo {
        core_breakdown: CoreBreakdown {
            p_cores: 8,
            e_cores: 4,
            lp_cores: 0,
        },
        cpu_types: vec![CpuType::Performance],
        ..HardwareInfo::default()
    };
    fixture::edit_hardware(&mut folded.shell, |slot| *slot = Some(hardware));

    assert_eq!(
        cpu_field_text(&folded.shell, CpuField::L1dCache),
        "1.25 MiB"
    );
    assert_eq!(cpu_field_text(&folded.shell, CpuField::L1iCache), "768 KiB");
    assert_eq!(cpu_field_text(&folded.shell, CpuField::L2Cache), "16 MiB");
    assert_eq!(cpu_field_text(&folded.shell, CpuField::L3Cache), "32 MiB");
    assert_eq!(
        cpu_field_text(&folded.shell, CpuField::PerformanceCores),
        "8"
    );
    assert_eq!(
        cpu_field_text(&folded.shell, CpuField::EfficiencyCores),
        "4"
    );
    assert_eq!(
        cpu_field_text(&folded.shell, CpuField::LowPowerCores),
        MISSING_VALUE
    );
    assert_eq!(
        cpu_field_text(&folded.shell, CpuField::Core(0)),
        "12.0% · 3.6 GHz"
    );
    assert_eq!(cpu_field_text(&folded.shell, CpuField::Power), "65.0 W");
}
