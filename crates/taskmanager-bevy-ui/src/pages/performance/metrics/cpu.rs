//! CPU detail-field projection over the cached shell snapshot.

use super::*;
use taskforest_core::core::hardware::CpuType;
use taskmanager_shell::presentation::cpu_idle_state_summary;
use taskmanager_shell::presentation::cpu_interrupt_summary;
use taskmanager_shell::presentation::cpu_power_limits_summary;
use taskmanager_shell::presentation::cpu_thermal_throttle_summary;
use taskmanager_shell::presentation::cpu_topology_summary;
use taskmanager_shell::presentation::load_average_basis_summary;
use taskmanager_shell::presentation::load_average_values_summary;
use taskmanager_shell::presentation::msr_thermal_status_summary;
use taskmanager_shell::presentation::pressure_summary;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CpuField {
    Brand,
    Usage,
    Frequency,
    Temperature,
    Power,
    Pressure,
    Load,
    Topology,
    IdleStates,
    PowerLimits,
    ThermalThrottle,
    ThermalStatus,
    Interrupts,
    /// The per-core bar's fill magnitude: the core's live utilization. The
    /// bar and the numeric slot are two views of this one observation.
    Core(usize),
    /// The per-core value slot: the core's utilization percentage alone.
    CoreUsage(usize),
    /// The per-core micro subtitle: the core's live frequency and
    /// temperature, the two secondary facts a core cell carries.
    CoreTelemetry(usize),
    L1dCache,
    L1iCache,
    L2Cache,
    L3Cache,
    PerformanceCores,
    EfficiencyCores,
    LowPowerCores,
}

impl Default for CpuField {
    /// Template seed only — the bsn! paren form requires a `Default` value
    /// that every spawned scene immediately patches with a real field.
    fn default() -> Self {
        Self::Usage
    }
}

pub(crate) fn cpu_field_text(shell: &ShellApp, field: CpuField) -> String {
    if let CpuField::Load = field {
        return shell
            .projection()
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.load_average.as_ref())
            .map_or_else(missing_value, |load| {
                format!(
                    "{} · {}",
                    load_average_values_summary(load),
                    load_average_basis_summary(load),
                )
            });
    }
    // The real-time thermal-status / PROCHOT assertion rides the privileged
    // `telemetry.cpu.msr` lane, not the periodic CPU snapshot, so it is read
    // before the snapshot gate: the mounted row keeps the shared dash while
    // the lane produced no accepted readout.
    if let CpuField::ThermalStatus = field {
        return msr_thermal_status_summary(shell.msr_readout_state()).unwrap_or_else(missing_value);
    }
    let Some(cpu) = cpu_metrics(shell) else {
        return missing_value();
    };
    match field {
        CpuField::Brand => cpu
            .brand
            .as_deref()
            .map(str::trim)
            .filter(|brand| !brand.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(missing_value),
        CpuField::Usage => observed_percentage(cpu.current_global_usage_pct()),
        CpuField::Frequency => cpu
            .current_frequency_mhz()
            .map(|value| megahertz(value as f32))
            .unwrap_or_else(missing_value),
        CpuField::Temperature => cpu
            .current_temperature_c()
            .filter(|value| value.is_finite())
            .map(temperature_c)
            .unwrap_or_else(missing_value),
        CpuField::Power => cpu
            .current_power_w()
            .filter(|value| value.is_finite())
            .map(power_w)
            .unwrap_or_else(missing_value),
        CpuField::Pressure => shell
            .projection()
            .snapshot
            .as_ref()
            .and_then(|s| s.pressure.as_ref())
            .and_then(|p| p.cpu.current_value())
            .map_or_else(missing_value, pressure_summary),
        CpuField::Topology => cpu_topology_summary(cpu).unwrap_or_else(missing_value),
        CpuField::IdleStates => cpu_idle_state_summary(cpu).unwrap_or_else(missing_value),
        CpuField::PowerLimits => cpu_power_limits_summary(cpu).unwrap_or_else(missing_value),
        CpuField::Interrupts => cpu_interrupt_summary(cpu).unwrap_or_else(missing_value),
        // The always-mounted diagnostic row keeps the shared dash while no
        // package observed a counter (the renderer's fixed-row discipline);
        // the value itself is the shared shell fold.
        CpuField::ThermalThrottle => {
            cpu_thermal_throttle_summary(cpu).unwrap_or_else(missing_value)
        }
        // The real-time thermal-status / PROCHOT assertion from the privileged
        // `telemetry.cpu.msr` lane is handled before the snapshot gate above.
        CpuField::ThermalStatus => missing_value(),
        CpuField::Load => missing_value(),
        CpuField::Core(index) => {
            let usage = observed_percentage(core_usage_pct(shell, index));
            let freq = cpu.current_core_frequency_mhz(index).map(|mhz| {
                if mhz >= 1000 {
                    format!("{:.1} GHz", mhz as f64 / 1000.0)
                } else {
                    format!("{mhz} MHz")
                }
            });
            match freq {
                Some(freq_str) => format!("{usage} · {freq_str}"),
                None => usage,
            }
        }
        CpuField::CoreUsage(index) => observed_percentage(core_usage_pct(shell, index)),
        CpuField::CoreTelemetry(index) => core_telemetry_text(shell, index),
        CpuField::L1dCache => format_cache_kb(cpu.l1d_cache_kb),
        CpuField::L1iCache => format_cache_kb(cpu.l1i_cache_kb),
        CpuField::L2Cache => format_cache_kb(cpu.l2_cache_kb),
        CpuField::L3Cache => format_cache_kb(cpu.l3_cache_kb),
        CpuField::PerformanceCores => shell
            .projection()
            .hardware
            .as_ref()
            .and_then(|h| (h.core_breakdown.p_cores > 0).then_some(h.core_breakdown.p_cores))
            .map_or_else(missing_value, |c| c.to_string()),
        CpuField::EfficiencyCores => shell
            .projection()
            .hardware
            .as_ref()
            .and_then(|h| (h.core_breakdown.e_cores > 0).then_some(h.core_breakdown.e_cores))
            .map_or_else(missing_value, |c| c.to_string()),
        CpuField::LowPowerCores => shell
            .projection()
            .hardware
            .as_ref()
            .and_then(|h| (h.core_breakdown.lp_cores > 0).then_some(h.core_breakdown.lp_cores))
            .map_or_else(missing_value, |c| c.to_string()),
    }
}

fn format_cache_kb(kb: Option<u64>) -> String {
    kb.map_or_else(missing_value, |kb| {
        if kb >= 1024 {
            let mib = kb as f64 / 1024.0;
            if (mib.fract()).abs() < 0.05 {
                format!("{mib:.0} MiB")
            } else {
                format!("{mib:.2} MiB")
            }
        } else {
            format!("{kb} KiB")
        }
    })
}

/// The per-core micro subtitle: the core's live frequency and temperature,
/// joined only when the platform observed them. A segment the host did not
/// observe keeps the shared dash; a core with neither fact reads as one
/// dash, never a fabricated zero.
pub(crate) fn core_telemetry_text(shell: &ShellApp, index: usize) -> String {
    let Some(cpu) = cpu_metrics(shell) else {
        return missing_value();
    };
    let mut parts: Vec<String> = Vec::with_capacity(2);
    if let Some(mhz) = cpu.current_core_frequency_mhz(index) {
        parts.push(if mhz >= 1000 {
            format!("{:.1} GHz", mhz as f64 / 1000.0)
        } else {
            format!("{mhz} MHz")
        });
    }
    if let Some(celsius) = cpu
        .current_core_temperature_c(index)
        .filter(|value| value.is_finite())
    {
        parts.push(temperature_c(celsius));
    }
    if parts.is_empty() {
        missing_value()
    } else {
        parts.join(" · ")
    }
}

/// The heterogeneous core classes, in the fixed presentation order the
/// reference per-core matrix uses.
const CORE_CLASS_ORDER: [CpuType; 4] = [
    CpuType::Performance,
    CpuType::Efficient,
    CpuType::LowPower,
    CpuType::Unknown,
];

/// One heterogeneous core class and the logical cores the observed
/// inventory places in it.
pub(crate) struct CoreClassGroup {
    /// The observed class of every core in this group.
    pub(crate) class: CpuType,
    /// The logical core indices of the group, in core order.
    pub(crate) indices: Vec<usize>,
}

impl CoreClassGroup {
    /// The locale key of the group's caption. The shared catalog owns the
    /// wording; the frontend never spells a class label itself.
    pub(crate) fn label_key(&self) -> &'static str {
        match self.class {
            CpuType::Performance => "cpu.performance_cores",
            CpuType::Efficient => "cpu.efficiency_cores",
            CpuType::LowPower => "cpu.low_power_cores",
            CpuType::Unknown => "common.cores",
        }
    }
}

/// One socket envelope: a physical package with the class groups its cores
/// form. `socket_id` is absent when the host published no package
/// inventory that covers every core — the envelope is then never drawn
/// rather than invented.
pub(crate) struct CoreSocketSection {
    /// The observed package index, when the envelope is backed by one.
    pub(crate) socket_id: Option<u32>,
    /// The NUMA node the package reported, when the platform exposed one.
    pub(crate) numa_node: Option<u32>,
    /// The class groups inside this envelope.
    pub(crate) classes: Vec<CoreClassGroup>,
}

/// The class groups of one core set. The per-logical-CPU type inventory is
/// the only source: a host without one keeps every core in a single flat
/// group, because a class is never guessed from a missing inventory.
fn core_class_groups_for(types: Option<&[CpuType]>, indices: &[usize]) -> Vec<CoreClassGroup> {
    let Some(types) = types.filter(|types| !types.is_empty()) else {
        return vec![CoreClassGroup {
            class: CpuType::Unknown,
            indices: indices.to_vec(),
        }];
    };
    CORE_CLASS_ORDER
        .iter()
        .filter_map(|&class| {
            let members: Vec<usize> = indices
                .iter()
                .copied()
                .filter(|&index| types.get(index).copied().unwrap_or_default() == class)
                .collect();
            (!members.is_empty()).then_some(CoreClassGroup {
                class,
                indices: members,
            })
        })
        .collect()
}

/// The per-core view's socket envelopes. The package inventory is the only
/// source of the socket partition; when it does not cover every core
/// exactly once, the view keeps one flat envelope instead of drawing a
/// partition it cannot substantiate.
pub(crate) fn core_socket_sections(shell: &ShellApp, core_count: usize) -> Vec<CoreSocketSection> {
    let types = shell
        .projection()
        .hardware
        .as_ref()
        .map(|hardware| hardware.cpu_types.as_slice());
    let all: Vec<usize> = (0..core_count).collect();
    let packages: Vec<(u32, Option<u32>, Vec<usize>)> = cpu_metrics(shell)
        .map(|cpu| {
            cpu.packages
                .iter()
                .filter(|package| !package.logical_core_ids.is_empty())
                .map(|package| {
                    (
                        package.package_id,
                        package.numa_node_id,
                        package
                            .logical_core_ids
                            .iter()
                            .copied()
                            .filter(|&index| index < core_count)
                            .collect::<Vec<usize>>(),
                    )
                })
                .filter(|(_, _, members)| !members.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let mut claimed: Vec<usize> = packages
        .iter()
        .flat_map(|(_, _, members)| members.iter().copied())
        .collect();
    claimed.sort_unstable();
    claimed.dedup();
    if claimed.len() != core_count {
        return vec![CoreSocketSection {
            socket_id: None,
            numa_node: None,
            classes: core_class_groups_for(types, &all),
        }];
    }
    let mut sections: Vec<CoreSocketSection> = packages
        .into_iter()
        .map(|(socket_id, numa_node, members)| CoreSocketSection {
            socket_id: Some(socket_id),
            numa_node,
            classes: core_class_groups_for(types, &members),
        })
        .collect();
    sections.sort_by_key(|section| section.socket_id.unwrap_or_default());
    sections
}
