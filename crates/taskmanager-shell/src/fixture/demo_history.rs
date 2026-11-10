//! The one deterministic demo/capture history sequence.
//!
//! GPUI owns a standalone `TelemetryStore` while the composed track owns a
//! `ShellApp`'s `LiveGraphHistory`; both are filled from this single sequence
//! through the same correlated ingestor. Keeping the frames here — instead of
//! letting each frontend grow its own warm-up loop — is what makes their
//! sparklines, headline curves and history readouts comparable evidence.

use taskforest_core::core::metrics::{CpuMetrics, ScalarObservation, ScalarObservationGroup};
use taskforest_core::core::percent::ClampedPercent;

/// Number of adjacent frames after the one honest `demo_app()` snapshot.
pub(super) const DEMO_HISTORY_FRAMES: u64 = 24;

/// One adjacent CPU frame: the seed's identity and shape with a deterministic
/// utilization sequence applied to the global and per-core observations. The
/// last frame returns to the snapshot's current fact, so the newest readout
/// and the waveform tail agree.
pub(super) fn demo_cpu_frame(seed: &CpuMetrics, timestamp_ms: u64, offset: u64) -> CpuMetrics {
    let usage = if offset == DEMO_HISTORY_FRAMES {
        37.4
    } else {
        24.0 + f32::from((offset.saturating_mul(7) % 26) as u8)
    };
    let mut observations = seed.scalar_observations().clone();
    observations.global_usage_pct = ScalarObservation::available(usage, timestamp_ms);
    let core_values = (0..seed.current_core_usage_len())
        .map(|index| (usage + (index as f32 * 4.0) - (offset % 3) as f32 * 2.0).clamped_percent())
        .collect();
    observations.core_usage_group = ScalarObservationGroup::available(core_values, timestamp_ms);
    let mut frame = CpuMetrics::from_observations(observations);
    frame.brand = seed.brand.clone();
    frame.frequency_source = seed.frequency_source;
    frame.temperature_source = seed.temperature_source;
    frame.physical_cores = seed.physical_cores;
    frame.logical_cores = seed.logical_cores;
    frame.l1d_cache_kb = seed.l1d_cache_kb;
    frame.l1i_cache_kb = seed.l1i_cache_kb;
    frame.l2_cache_kb = seed.l2_cache_kb;
    frame.l3_cache_kb = seed.l3_cache_kb;
    frame.performance_policy = seed.performance_policy.clone();
    frame.packages = seed.packages.clone();
    frame.idle_states = seed.idle_states.clone();
    frame.interrupts = seed.interrupts.clone();
    frame
}
