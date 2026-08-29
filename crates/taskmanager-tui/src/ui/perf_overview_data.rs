//! Pure data-layer folds for the performance overview.

use taskmanager_application::i18n::t;
use taskmanager_core::core::hardware::{CoreBreakdown, HardwareInfo};
use taskmanager_core::core::metrics::{CpuMetrics, SystemSnapshot};
use taskmanager_shell::presentation::{duration, missing_value, power_w};

use super::units::{
    cache_mib, observed_frequency_for_source, observed_percentage, observed_temperature_for_source,
    spec_ghz,
};

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
            value: observed_frequency_for_source(frequency, cpu.frequency_source),
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

/// The page-title subtitle: the provider-reported brand, trimmed; an absent
/// or blank brand stays an honest dash (the gpui `cpu_brand` fold's exact
/// semantics — the frontend never invents an "unknown CPU" model).
pub(super) fn cpu_brand_subtitle(cpu: &CpuMetrics) -> String {
    cpu.brand
        .as_deref()
        .map(str::trim)
        .filter(|brand| !brand.is_empty())
        .map_or_else(missing_value, str::to_owned)
}
/// One right-rail `label value` row. Labels resolve through `t()` at fold
/// time, so a frame is always painted with one consistent locale.
pub(super) struct CpuRailRow {
    pub(super) label: String,
    pub(super) value: String,
}

/// The live rail rows: system-wide counters that belong beside the CPU
/// graph exactly as the gpui pinned details panel places them. Labels reuse
/// the System page's keys and the shared `duration` readout so one fact has
/// one presentation in the whole frontend.
pub(super) fn cpu_live_rail_rows(snapshot: &SystemSnapshot) -> Vec<CpuRailRow> {
    vec![
        CpuRailRow {
            label: t("common.processes").to_owned(),
            value: snapshot.processes.to_string(),
        },
        CpuRailRow {
            label: t("common.threads").to_owned(),
            value: snapshot
                .threads
                .map_or_else(missing_value, |threads| threads.to_string()),
        },
        CpuRailRow {
            label: t("common.uptime").to_owned(),
            value: duration(snapshot.uptime_secs),
        },
    ]
}

/// The static spec rows, row-for-row the gpui `cpu_spec_rows` projection
/// (`cpu_view/details_panel.rs`): same order, same labels, same value
/// semantics, same honesty rules — an unobserved scalar renders the shared
/// dash, a whole-missing hardware inventory renders dashes instead of
/// fabricating bare-metal answers, and a policy row the platform does not
/// report is an ABSENT row, never a dash slot.
pub(super) fn cpu_spec_rail_rows(
    cpu: &CpuMetrics,
    hardware: Option<&HardwareInfo>,
) -> Vec<CpuRailRow> {
    let mut rows = vec![
        // Static advertised base clock — NOT the live frequency above.
        CpuRailRow {
            label: t("cpu.base_speed").to_owned(),
            value: spec_ghz(hardware.and_then(|item| item.base_freq_mhz)),
        },
        CpuRailRow {
            label: t("common.sockets").to_owned(),
            value: hardware
                .and_then(|item| item.sockets)
                .map_or_else(missing_value, |count| count.to_string()),
        },
        CpuRailRow {
            label: t("common.cores").to_owned(),
            value: cpu
                .physical_cores
                .map_or_else(missing_value, |cores| cores.to_string()),
        },
    ];
    let unclassified = CoreBreakdown::default();
    let breakdown = hardware.map_or(&unclassified, |item| &item.core_breakdown);
    rows.extend(heterogeneous_core_rows(breakdown));
    rows.extend([
        CpuRailRow {
            label: t("cpu.logical_processors").to_owned(),
            value: cpu
                .logical_cores
                .map_or_else(missing_value, |cores| cores.to_string()),
        },
        // Real hypervisor label when a hardware inventory exists ("None" on
        // bare metal, the gpui semantics); with no inventory at all the dash
        // is the only honest answer.
        CpuRailRow {
            label: t("common.virtualization").to_owned(),
            value: hardware.map_or_else(missing_value, |item| {
                item.virt
                    .clone()
                    .unwrap_or_else(|| t("common.none").to_owned())
            }),
        },
        CpuRailRow {
            label: t("common.l1_cache").to_owned(),
            value: cache_mib(cpu.l1_cache_kb),
        },
        CpuRailRow {
            label: t("common.l2_cache").to_owned(),
            value: cache_mib(cpu.l2_cache_kb),
        },
        CpuRailRow {
            label: t("common.l3_cache").to_owned(),
            value: cache_mib(cpu.l3_cache_kb),
        },
    ]);
    let policy = &cpu.performance_policy;
    if let Some(driver) = &policy.frequency_implementation {
        rows.push(CpuRailRow {
            label: t("cpu.cpufreq_driver").to_owned(),
            value: driver.clone(),
        });
    }
    if let Some(governor) = &policy.active_policy {
        rows.push(CpuRailRow {
            label: t("cpu.cpufreq_governor").to_owned(),
            value: governor.clone(),
        });
    }
    if let Some(preference) = &policy.energy_preference {
        rows.push(CpuRailRow {
            label: t("cpu.power_preference").to_owned(),
            value: preference.clone(),
        });
    }
    rows
}

/// One aligned row per heterogeneous core class, only when the topology
/// actually reports efficiency classes — the gpui `heterogeneous_core_rows`
/// fold verbatim (a uniform topology stays unbroken by all-zero rows).
fn heterogeneous_core_rows(core: &CoreBreakdown) -> Vec<CpuRailRow> {
    if core.e_cores == 0 && core.lp_cores == 0 {
        return Vec::new();
    }
    [
        ("cpu.performance_cores", core.p_cores),
        ("cpu.efficiency_cores", core.e_cores),
        ("cpu.low_power_cores", core.lp_cores),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(key, count)| CpuRailRow {
        label: t(key).to_owned(),
        value: count.to_string(),
    })
    .collect()
}

/// The per-core average + maximum temperature footnote beneath the package
/// reading — the gpui `core_temperature_note` fold verbatim: shown only when
/// at least two real core channels report, collapsing to a single "Cores"
/// readout when every reported core sits within one degree. A missing or
/// single-channel observation keeps the package line alone; no value is
/// ever invented for unmapped cores.
pub(super) fn cpu_core_temperature_note(cpu: &CpuMetrics) -> Option<String> {
    let reporting: Vec<f32> = (0..cpu.current_core_temperature_len())
        .filter_map(|index| cpu.current_core_temperature_c(index))
        .collect();
    if reporting.len() < 2 {
        return None;
    }
    let average = reporting.iter().copied().sum::<f32>() / reporting.len() as f32;
    let maximum = reporting.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let minimum = reporting.iter().copied().fold(f32::INFINITY, f32::min);
    Some(if (maximum - minimum) < 1.0 {
        format!("{} {:.0} °C", t("common.cores"), maximum.round())
    } else {
        format!(
            "{} {:.0} · {} {:.0} °C",
            t("common.avg"),
            average.round(),
            t("common.max"),
            maximum.round()
        )
    })
}
