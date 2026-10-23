//! The shell owns the bounded NPU power history.
//!
//! The NPU inventory publishes the accelerator's power and energy facts; the
//! shell turns them into the bounded series a chart draws. The series lives
//! here, beside the NPU usage history, so no frontend owns a buffer. A host
//! whose accelerator does not report the capability leaves the buffer EMPTY:
//! the absence of the counter is the absence of the series, never a zero line
//! and never a value borrowed from the utilization curve.

use crate::ShellApp;
use crate::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_core::core::DeviceGeneration;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::npu::{
    NpuDevice, NpuInventorySnapshot, NpuPowerObservation, NpuPowerReport,
};

/// One accelerator with a measured utilization.
fn accelerator(device_id: &str, utilization_pct: f32) -> NpuDevice {
    NpuDevice {
        device_id: device_id.into(),
        device_generation: DeviceGeneration::new(1),
        brand: None,
        driver: None,
        utilization_pct: ScalarObservation::available(utilization_pct, 1_000),
        engines: Vec::new(),
        memory: Default::default(),
    }
}

/// One accelerator's power and energy observation.
fn power(device_id: &str, watts: f32, joules: u64) -> NpuPowerObservation {
    NpuPowerObservation {
        device_id: device_id.into(),
        report: NpuPowerReport {
            power_w: ScalarObservation::available(watts, 1_000),
            energy_j: ScalarObservation::available(joules, 1_000),
        },
    }
}

#[test]
fn an_accelerator_without_the_power_capability_leaves_the_history_empty() {
    let mut shell = ShellApp::new();
    let snapshot = NpuInventorySnapshot::discovered(vec![accelerator("npu0", 20.0)], 1_000);
    seed_projection_fact(&mut shell, ProjectionSeedFact::NpuInventory(Some(snapshot)));

    assert!(
        shell.projection().npu_power_history.is_empty(),
        "no power counter means no series: {:?}",
        shell.projection().npu_power_history
    );
}

#[test]
fn the_measured_power_draw_joins_the_bounded_history_with_its_instant() {
    let mut shell = ShellApp::new();
    let snapshot = NpuInventorySnapshot::discovered(vec![accelerator("npu0", 20.0)], 4_242)
        .with_power(vec![power("npu0", 12.5, 3_600)]);
    seed_projection_fact(&mut shell, ProjectionSeedFact::NpuInventory(Some(snapshot)));

    assert_eq!(
        shell.projection().npu_power_history.len(),
        1,
        "the measured draw joins the history"
    );
    assert_eq!(
        shell.projection().npu_power_history.front().copied(),
        Some((4_242, 12.5)),
        "the sample carries the observation instant and the measured watts"
    );
}

#[test]
fn the_history_sums_the_accelerators_that_report_the_capability() {
    let mut shell = ShellApp::new();
    let snapshot = NpuInventorySnapshot::discovered(
        vec![accelerator("npu0", 20.0), accelerator("npu1", 30.0)],
        1_000,
    )
    .with_power(vec![power("npu0", 4.0, 100), power("npu1", 6.5, 200)]);
    seed_projection_fact(&mut shell, ProjectionSeedFact::NpuInventory(Some(snapshot)));

    assert_eq!(
        shell.projection().npu_power_history.back().copied(),
        Some((1_000, 10.5)),
        "the aggregate draw is the sum of the accelerators that report it"
    );
}

#[test]
fn the_history_stays_bounded() {
    let mut shell = ShellApp::new();
    for revision in 1..=700_u64 {
        let snapshot = NpuInventorySnapshot::discovered(vec![accelerator("npu0", 20.0)], revision)
            .with_power(vec![power("npu0", 12.5, 3_600)]);
        seed_projection_fact(&mut shell, ProjectionSeedFact::NpuInventory(Some(snapshot)));
    }

    assert_eq!(
        shell.projection().npu_power_history.len(),
        600,
        "the history is bounded: a long session cannot grow it without limit"
    );
}
