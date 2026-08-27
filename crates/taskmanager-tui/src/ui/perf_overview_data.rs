//! Pure data-layer folds for the performance overview.

use taskmanager_application::{CpuMetrics, SystemSnapshot, i18n::t};
use taskmanager_shell::presentation::{missing_value, power_w};

use super::units::{observed_frequency, observed_percentage, observed_temperature_for_source};

pub(super) struct CpuMetricFact {
    pub(super) label: &'static str,
    pub(super) value: String,
    pub(super) available: bool,
}

/// Every CPU headline fact in the page's compact presentation order.
/// Unavailable observations remain present as an honest dash.
pub(super) fn cpu_metric_facts(cpu: &CpuMetrics) -> Vec<CpuMetricFact> {
    let utilization = cpu.current_global_usage_pct();
    let temperature = cpu.current_temperature_c();
    let temperature_source = cpu.temperature_source;
    let frequency = cpu.current_frequency_mhz();
    let power = cpu.current_power_w();
    vec![
        CpuMetricFact {
            label: t("common.utilization"),
            value: observed_percentage(utilization),
            available: utilization.is_some(),
        },
        CpuMetricFact {
            label: t("common.temperature"),
            value: observed_temperature_for_source(temperature, temperature_source),
            available: temperature.is_some(),
        },
        CpuMetricFact {
            label: t("cpu.frequency"),
            value: observed_frequency(frequency),
            available: frequency.is_some(),
        },
        CpuMetricFact {
            label: t("common.power"),
            value: power.map_or_else(missing_value, power_w),
            available: power.is_some(),
        },
    ]
}

pub(super) fn cpu_gauge_value(snapshot: &SystemSnapshot) -> Option<f32> {
    snapshot.cpu.current_global_usage_pct()
}
