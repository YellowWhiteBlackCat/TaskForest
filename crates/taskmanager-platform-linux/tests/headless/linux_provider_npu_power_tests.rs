//! Capability-gated NPU power and energy reads (CORE-49).
//!
//! The contract publishes an accelerator's power/energy observation only where
//! the driver exposes a real counter. These tests pin both states: an
//! accelerator with a `hwmon` power/energy counter reports measured values,
//! and an accelerator without one reports no observation at all — the energy
//! series is never derived from the utilization curve.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use taskmanager_core::ScalarAvailability;

static SCRATCH: AtomicU64 = AtomicU64::new(0);

struct ScratchDir(std::path::PathBuf);

impl ScratchDir {
    fn new(label: &str) -> Self {
        let unique = SCRATCH.fetch_add(1, Ordering::Relaxed);
        let root = crate::test_support::repo_temp_dir().join(format!(
            "taskforest-npu-power-{label}-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&root).expect("scratch root");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn accel_node(root: &Path) -> PathBuf {
    let node = root.join("accel0");
    fs::create_dir_all(node.join("device")).expect("device dir");
    node
}

fn write_hwmon(node: &Path, power: Option<&str>, energy: Option<&str>) {
    let hwmon = node.join("device/hwmon/hwmon0");
    fs::create_dir_all(&hwmon).expect("hwmon dir");
    fs::write(hwmon.join("name"), "intel_vpu\n").expect("hwmon name");
    if let Some(power) = power {
        fs::write(hwmon.join("power1_input"), power).expect("power counter");
    }
    if let Some(energy) = energy {
        fs::write(hwmon.join("energy1_input"), energy).expect("energy counter");
    }
}

#[test]
fn an_accelerator_with_hwmon_counters_reports_measured_power_and_energy() {
    let scratch = ScratchDir::new("measured");
    let node = accel_node(scratch.path());
    write_hwmon(&node, Some("2500000\n"), Some("3000000000\n"));

    let snapshot = discover_accelerators(scratch.path(), 42).expect("discovery succeeds");
    assert!(snapshot.is_success());
    assert_eq!(snapshot.devices.len(), 1);
    assert_eq!(snapshot.power.len(), 1);
    let report = snapshot
        .power_for(&snapshot.devices[0].device_id)
        .expect("the measured accelerator carries its power observation");
    assert_eq!(report.power_w.current_value().copied(), Some(2.5));
    assert_eq!(report.energy_j.current_value().copied(), Some(3_000));
}

#[test]
fn an_accelerator_without_power_counters_reports_no_energy_series() {
    let scratch = ScratchDir::new("absent");
    let node = accel_node(scratch.path());
    fs::create_dir_all(node.join("device/hwmon/hwmon0")).expect("hwmon dir");

    let snapshot = discover_accelerators(scratch.path(), 42).expect("discovery succeeds");
    assert_eq!(snapshot.devices.len(), 1);
    assert!(
        snapshot.power.is_empty(),
        "an accelerator without a power counter contributes no observation"
    );
    assert!(
        snapshot.power_for(&snapshot.devices[0].device_id).is_none(),
        "the energy series stays absent instead of being derived from utilization"
    );
}

#[test]
fn a_malformed_counter_is_not_reported_as_a_measurement() {
    let scratch = ScratchDir::new("malformed");
    let node = accel_node(scratch.path());
    write_hwmon(&node, Some("not-a-number\n"), None);

    let snapshot = discover_accelerators(scratch.path(), 42).expect("discovery succeeds");
    assert_eq!(snapshot.devices.len(), 1);
    assert!(
        snapshot.power.is_empty(),
        "an unreadable counter must not become a measured value"
    );
}

#[test]
fn an_unsupported_platform_reports_the_typed_absence() {
    // The report itself stays typed: a platform without the capability keeps
    // `Unavailable(Unsupported)` for both fields.
    let report = NpuPowerReport::unsupported();
    assert_eq!(
        report.power_w.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(
        report.energy_j.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert!(report.power_w.current_value().is_none());
    assert!(report.energy_j.current_value().is_none());
}
