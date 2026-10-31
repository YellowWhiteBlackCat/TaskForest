//! The NPU accelerator's power fact and its history series.
//!
//! The shared contract publishes the accelerator's power draw and its
//! cumulative energy as ONE typed fact
//! ([`taskforest_core::core::npu::NpuPowerReport`]). A platform that
//! cannot read the capability publishes no fact at all, and this projection
//! then has nothing to draw: the energy curve is absent, never a flat zero
//! line and never a value borrowed from the utilization curve.
//!
//! The series itself lives in the shell (the bounded power history beside
//! the NPU usage history); a frontend reads it and never owns a buffer.

use taskforest_core::core::npu::NpuInventorySnapshot;
use taskmanager_shell::ShellApp;

/// The accelerator power fact of the current NPU inventory.
pub struct NpuPowerFact {
    /// The aggregate power draw, in watts.
    pub watts: f32,
    /// The aggregate consumed energy, in joules.
    pub energy_j: u64,
}

/// The NPU power fact of the current inventory, or `None` when no accelerator
/// reports the capability.
///
/// The aggregate is the sum over the accelerators that REPORT the fact: an
/// accelerator without a power counter contributes nothing, and an inventory
/// where none reports it has no fact at all.
pub fn npu_power_fact(shell: &ShellApp) -> Option<NpuPowerFact> {
    let inventory = shell.projection().npu_inventory.as_ref()?;
    let watts = aggregate_watts(inventory)?;
    let energy_j = inventory
        .power
        .iter()
        .filter_map(|observation| observation.report.energy_j.current_value().copied())
        .sum();
    Some(NpuPowerFact { watts, energy_j })
}

/// The NPU power series in watts, read from the shell's bounded history.
///
/// The buffer is EMPTY while no accelerator reported the capability: an
/// empty series means no curve, never a zero line.
pub fn npu_power_samples(shell: &ShellApp) -> Vec<f32> {
    shell
        .projection()
        .npu_power_history
        .iter()
        .map(|(_, watts)| *watts)
        .collect()
}

/// The measured draw of the accelerators that report it, in watts.
fn aggregate_watts(inventory: &NpuInventorySnapshot) -> Option<f32> {
    inventory
        .power
        .iter()
        .filter_map(|observation| {
            observation
                .report
                .power_w
                .current_value()
                .copied()
                .filter(|value| value.is_finite())
        })
        .reduce(|left, right| left + right)
}
