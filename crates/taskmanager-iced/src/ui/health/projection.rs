//! Pure system-health observation projection.

use taskmanager_application::SystemSnapshot;

pub(super) struct HealthObservation {
    pub cpu_usage_pct: Option<f32>,
    pub cpu_frequency: String,
    pub cpu_temperature_c: Option<f32>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub swap_used_bytes: Option<u64>,
    pub swap_total_bytes: Option<u64>,
}

impl From<&SystemSnapshot> for HealthObservation {
    fn from(snapshot: &SystemSnapshot) -> Self {
        Self {
            cpu_usage_pct: snapshot.cpu.current_global_usage_pct(),
            cpu_frequency: crate::ui::perf_overview::cpu_frequency_readout_for_source(
                snapshot.cpu.current_frequency_mhz(),
                snapshot.cpu.frequency_source.is_bogomips(),
            ),
            cpu_temperature_c: snapshot.cpu.current_temperature_c(),
            memory_used_bytes: snapshot.memory.current_used_bytes(),
            memory_total_bytes: snapshot.memory.current_total_bytes(),
            swap_used_bytes: snapshot.memory.current_swap_used_bytes(),
            swap_total_bytes: snapshot.memory.current_swap_total_bytes(),
        }
    }
}

#[must_use]
pub(super) fn thermal_readings(snapshot: &SystemSnapshot) -> Vec<(String, f32)> {
    let mut readings = Vec::new();
    if let Some(temperature) = snapshot.cpu.current_temperature_c() {
        readings.push(("CPU Package".to_owned(), temperature));
    }
    readings.extend(snapshot.gpu.iter().enumerate().filter_map(|(index, gpu)| {
        let temperature = gpu.current_temperature_c()?;
        let label = if gpu.brand.is_empty() {
            format!("GPU {index}")
        } else {
            format!("GPU {index} ({})", gpu.brand)
        };
        Some((label, temperature))
    }));
    readings
}
