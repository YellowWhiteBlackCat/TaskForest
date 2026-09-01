//! Capture-only shell seed for the deterministic demo window.
//!
//! The visual-contract capture needs a populated curve window, so the demo
//! composition records a short adjacent telemetry sequence on top of the
//! shared `demo_app()` fixture. This module is a pure data fixture: it owns
//! no UI tree and paints nothing — it only feeds the shared shell the same
//! typed observations a real collection frame would, which keeps it out of
//! the renderer/fold boundary the display gate pins (ARCH.md §8.1).
//!
//! Production collection and the shared fixture's semantics are untouched:
//! `demo_app` starts with one honest sample and the sequence below is
//! appended only inside the capture composition.

use taskmanager_core::core::metrics::{CpuMetrics, ScalarObservation, ScalarObservationGroup};

/// Build the capture-only shell with a warm, deterministic graph window.
pub(crate) fn demo_shell() -> taskmanager_shell::ShellApp {
    let mut shell = taskmanager_shell::demo_app();
    if let Some(seed) = shell.projection().snapshot.clone() {
        for offset in 1..=24_u64 {
            let mut next = seed.clone();
            next.timestamp_ms = next
                .timestamp_ms
                .saturating_add(offset.saturating_mul(1_000));
            next.cpu = demo_cpu_frame(&next.cpu, next.timestamp_ms, offset);
            taskmanager_shell::fixture::record_demo_history_frame(&mut shell, &next, None, None);
        }
    }
    shell
}

/// One adjacent CPU frame: the seed's identity and shape with a deterministic
/// utilization sequence applied to the global and per-core observations.
fn demo_cpu_frame(seed: &CpuMetrics, timestamp_ms: u64, offset: u64) -> CpuMetrics {
    let usage = if offset == 24 {
        37.4
    } else {
        24.0 + f32::from((offset.saturating_mul(7) % 26) as u8)
    };
    let mut observations = seed.scalar_observations().clone();
    observations.global_usage_pct = ScalarObservation::available(usage, timestamp_ms);
    let core_values = (0..seed.current_core_usage_len())
        .map(|index| (usage + (index as f32 * 4.0) - (offset % 3) as f32 * 2.0).clamp(0.0, 100.0))
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
    frame
}
