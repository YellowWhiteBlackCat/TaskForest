//! Pure CPU observation projection consumed by the CPU render tree.

use crate::core::metrics::SystemSnapshot;
use crate::gpui_app::formatting;
use crate::i18n;

use super::{
    cpu_frequency_readout_for_source, cpu_temperature_readout_for_source, per_core_cell_label,
};

pub(super) struct CpuCoreReadout {
    label: String,
}

impl CpuCoreReadout {
    pub(super) fn label(&self) -> &str {
        &self.label
    }
}

pub(super) struct CpuDetailsStats {
    pub utilization: String,
    pub speed: String,
    pub speed_note: Option<&'static str>,
    pub temperature: String,
    pub temperature_note: Option<String>,
}

pub(super) struct CpuLiveStats {
    pub cores: Vec<CpuCoreReadout>,
    pub utilization_readout: String,
    pub frequency_readout: Option<String>,
    pub temperature_readout: Option<String>,
    pub power_readout: Option<String>,
    pub details: CpuDetailsStats,
}

impl CpuLiveStats {
    pub(super) fn from_snapshot(snapshot: &SystemSnapshot) -> Self {
        let cpu = &snapshot.cpu;
        // Preserve the CPU page's one-cell fallback when the provider has not
        // published an indexed core vector yet. Every value in that cell stays
        // honestly missing; only the stable layout slot is synthesized.
        let core_count = cpu.current_core_usage_len().max(1);
        let cores = (0..core_count)
            .map(|index| {
                let usage = cpu.current_core_usage_pct(index);
                let temperature = cpu.current_core_temperature_c(index);
                let frequency = cpu.current_core_frequency_mhz(index);
                CpuCoreReadout {
                    label: per_core_cell_label(usage, temperature, frequency),
                }
            })
            .collect();
        let usage = cpu.current_global_usage_pct();
        let frequency = cpu.current_frequency_mhz();
        let temperature = cpu.current_temperature_c();
        let power = cpu.current_power_w().filter(|value| *value > 0.0);
        let reporting_temperatures: Vec<f32> = (0..cpu.current_core_temperature_len())
            .filter_map(|index| cpu.current_core_temperature_c(index))
            .collect();

        Self {
            cores,
            utilization_readout: usage
                .map_or_else(formatting::missing_value, |value| format!("{value:.0} %")),
            frequency_readout: frequency
                .map(|value| cpu_frequency_readout_for_source(Some(value), cpu.frequency_source)),
            temperature_readout: temperature.map(|value| {
                cpu_temperature_readout_for_source(Some(value), cpu.temperature_source)
            }),
            power_readout: power.map(|value| format!("{value:.1} W")),
            details: CpuDetailsStats {
                utilization: usage.map_or_else(formatting::missing_value, |value| {
                    format!("{:.0}%", value.round())
                }),
                speed: cpu_frequency_readout_for_source(frequency, cpu.frequency_source),
                speed_note: cpu
                    .frequency_source
                    .is_bogomips()
                    .then(|| i18n::t("cpu.frequency_bogomips")),
                temperature: cpu_temperature_readout_for_source(
                    temperature,
                    cpu.temperature_source,
                ),
                temperature_note: core_temperature_note(&reporting_temperatures),
            },
        }
    }
}

fn core_temperature_note(reporting: &[f32]) -> Option<String> {
    if reporting.len() < 2 {
        return None;
    }
    let average = reporting.iter().copied().sum::<f32>() / reporting.len() as f32;
    let maximum = reporting.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let minimum = reporting.iter().copied().fold(f32::INFINITY, f32::min);
    Some(if (maximum - minimum) < 1.0 {
        format!("{} {:.0} \u{b0}C", i18n::t("common.cores"), maximum.round())
    } else {
        format!(
            "{} {:.0} · {} {:.0} \u{b0}C",
            i18n::t("common.avg"),
            average.round(),
            i18n::t("common.max"),
            maximum.round()
        )
    })
}
