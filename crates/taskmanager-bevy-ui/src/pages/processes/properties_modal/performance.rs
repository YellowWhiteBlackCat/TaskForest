//! Four frozen-process performance curves over accepted process histories.

use crate::widgets::chart::MAX_CHART_POINTS;
use std::sync::Arc;
use taskmanager_application::i18n::t;
use taskmanager_application::process_details_vm::{
    ProcessDetailsField, ProcessDetailsRowVm, detail_value,
};
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences, format_quantity_f64};
use taskmanager_shell::presentation::{MISSING_VALUE, peak_of};

#[derive(Clone, Debug)]
pub(crate) struct PerformanceCurve {
    pub metric: ProcessDetailsField,
    pub label: String,
    pub current: String,
    pub peak: String,
    pub samples: Arc<[f32]>,
    pub ceiling: f32,
}
impl PerformanceCurve {
    pub(super) fn same_rendered(&self, other: &Self) -> bool {
        self.metric == other.metric
            && self.current == other.current
            && self.peak == other.peak
            && self.samples.len() == other.samples.len()
            && self
                .samples
                .iter()
                .zip(other.samples.iter())
                .all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

pub(super) fn performance_curves(
    process: &ProcessItem,
    vm: &[ProcessDetailsRowVm],
) -> Vec<PerformanceCurve> {
    [
        (
            ProcessDetailsField::Cpu,
            "common.cpu",
            &process.cpu_history,
            process.current_cpu_percentage(),
            QuantityFamily::Memory,
            false,
        ),
        (
            ProcessDetailsField::Memory,
            "common.memory",
            &process.mem_history,
            process.current_memory_bytes().map(|v| v as f32),
            QuantityFamily::Memory,
            false,
        ),
        (
            ProcessDetailsField::DiskReadRate,
            "proc.disk_read",
            &process.disk_read_history,
            process.current_disk_read_bytes_per_sec().map(|v| v as f32),
            QuantityFamily::Drive,
            true,
        ),
        (
            ProcessDetailsField::DiskWriteRate,
            "proc.disk_write",
            &process.disk_write_history,
            process.current_disk_write_bytes_per_sec().map(|v| v as f32),
            QuantityFamily::Drive,
            true,
        ),
    ]
    .into_iter()
    .map(|(metric, label, samples, current, family, rate)| {
        let samples: Arc<[f32]> =
            Arc::from(&samples[samples.len().saturating_sub(MAX_CHART_POINTS)..]);
        let peak = peak_of(&samples, current);
        let peak_text = peak.map_or_else(
            || MISSING_VALUE.to_owned(),
            |value| {
                if metric == ProcessDetailsField::Cpu {
                    format!("{value:.1}%")
                } else {
                    format_quantity_f64(f64::from(value), family, rate, &UnitPreferences::default())
                }
            },
        );
        PerformanceCurve {
            metric,
            label: t(label).to_owned(),
            current: detail_value(vm, metric).text_or(MISSING_VALUE).to_owned(),
            peak: peak_text,
            samples,
            ceiling: if metric == ProcessDetailsField::Cpu {
                100.0
            } else {
                peak.unwrap_or(1.0).max(1.0)
            },
        }
    })
    .collect()
}
