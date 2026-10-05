//! Recent deterministic telemetry samples through the shared history ingestor.
use super::DEMO_HISTORY_FRAMES;
use crate::TuiApp;
use taskmanager_core::core::metrics::{ScalarObservation, ScalarObservationGroup};
use taskmanager_shell::fixture::record_demo_history_frame;

pub(super) fn seed_demo_history(app: &mut TuiApp) {
    let Some(base) = app.projection().snapshot.clone() else {
        return;
    };
    let last_frame = DEMO_HISTORY_FRAMES.saturating_sub(1);
    for frame_index in 1..DEMO_HISTORY_FRAMES {
        let index = frame_index as f64;
        let settle = 1.0 - index / f64::from(last_frame as u32);
        // Three non-harmonic phases so no two channels draw the same wave.
        let wave = |period: f64, phase: f64| {
            (0.5 - 0.5 * (std::f64::consts::TAU * index / period + phase).cos()) * settle
        };
        let mut frame = base.clone();
        frame.timestamp_ms = base.timestamp_ms.saturating_add(frame_index as u64 * 1_000);

        // CPU: per-core utilization swings proportionally to its own base
        // (busy cores breathe more), the global readout on its own phase.
        let mut cpu_observations = frame.cpu.scalar_observations().clone();
        if let Some(cores) = cpu_observations.core_usage_group.current_observations() {
            let varied: Vec<f32> = cores
                .iter()
                .filter_map(|core| core.current_value())
                .map(|base_value| {
                    let base = f64::from(*base_value);
                    let amplitude = 3.0 + base * 0.25;
                    let varied = base + amplitude * wave(12.0, 0.0);
                    varied.clamp(0.5, 99.0) as f32
                })
                .collect();
            cpu_observations.core_usage_group =
                ScalarObservationGroup::available(varied, frame.timestamp_ms);
        }
        if let Some(global) = cpu_observations.global_usage_pct.current_value() {
            let varied = f64::from(*global) + 14.0 * wave(17.0, 0.9);
            cpu_observations.global_usage_pct =
                ScalarObservation::available(varied.clamp(1.0, 99.0) as f32, frame.timestamp_ms);
        }
        frame.cpu.apply_scalar_observations(cpu_observations);

        // Memory: the used share breathes on a slow phase; the available lane
        // follows the same bounded total so the gauge stays honest.
        let memory = &mut frame.memory;
        let mut scalar = *memory.scalar_observations();
        let optional = memory.optional_observations().clone();
        if let (Some(total), Some(used)) = (
            scalar.total_bytes.current_value().copied(),
            scalar.used_bytes.current_value().copied(),
        ) {
            let headroom = total.saturating_sub(1);
            let used_varied = (used.min(headroom) as f64
                + (64.0 * 1024.0 * 1024.0) * wave(19.0, 1.7))
            .clamp(1024.0, headroom as f64);
            let used_bytes = (used_varied as u64).min(headroom);
            scalar.used_bytes = ScalarObservation::available(used_bytes, frame.timestamp_ms);
            scalar.available_bytes =
                ScalarObservation::available(total - used_bytes, frame.timestamp_ms);
        }
        if let Some(swap_used) = scalar.swap_used_bytes.current_value() {
            let varied = *swap_used as f64 + (96.0 * 1024.0 * 1024.0) * wave(9.0, 2.4);
            scalar.swap_used_bytes =
                ScalarObservation::available(varied.max(0.0) as u64, frame.timestamp_ms);
        }
        memory.apply_observations(scalar, optional);

        // Disk and NIC main lanes breathe on their own phases so the device
        // trend rows and throughput summaries draw a shape, not a flat line.
        for disk in &mut frame.disks {
            let mut scalar = *disk.scalar_observations();
            if let Some(read) = scalar.read_bytes_per_sec.current_value() {
                let varied = *read as f64 * (0.65 + 0.7 * wave(11.0, 0.4));
                scalar.read_bytes_per_sec =
                    ScalarObservation::available(varied.max(0.0) as u64, frame.timestamp_ms);
            }
            if let Some(write) = scalar.write_bytes_per_sec.current_value() {
                let varied = *write as f64 * (0.65 + 0.7 * wave(8.0, 1.1));
                scalar.write_bytes_per_sec =
                    ScalarObservation::available(varied.max(0.0) as u64, frame.timestamp_ms);
            }
            disk.apply_scalar_observations(scalar);
        }
        for network in &mut frame.networks {
            let mut scalar = *network.scalar_observations();
            let wireless = network.wireless_observations().clone();
            if let Some(rx) = scalar.rx_bytes_per_sec.current_value() {
                let varied = *rx as f64 * (0.55 + 0.9 * wave(13.0, 2.0));
                scalar.rx_bytes_per_sec =
                    ScalarObservation::available(varied.max(0.0) as u64, frame.timestamp_ms);
            }
            if let Some(tx) = scalar.tx_bytes_per_sec.current_value() {
                let varied = *tx as f64 * (0.55 + 0.9 * wave(10.0, 0.2));
                scalar.tx_bytes_per_sec =
                    ScalarObservation::available(varied.max(0.0) as u64, frame.timestamp_ms);
            }
            network.apply_observations(network.adapter_type(), scalar, wireless);
        }

        record_demo_history_frame(&mut app.shell, &frame, None, None);
    }
}
