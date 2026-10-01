//! CPU detail-field projection over the cached shell snapshot.

use super::*;
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
    Core(usize),
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
