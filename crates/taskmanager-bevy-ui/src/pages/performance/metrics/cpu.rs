//! CPU detail-field projection over the cached shell snapshot.

use super::*;
use taskmanager_shell::presentation::cpu_idle_state_summary;
use taskmanager_shell::presentation::cpu_interrupt_summary;
use taskmanager_shell::presentation::cpu_power_limits_summary;
use taskmanager_shell::presentation::cpu_thermal_throttle_summary;
use taskmanager_shell::presentation::cpu_topology_summary;
use taskmanager_shell::presentation::load_average_basis_summary;
use taskmanager_shell::presentation::load_average_values_summary;
use taskmanager_shell::presentation::pressure_summary;

pub(in super::super) fn cpu_field_text(shell: &ShellApp, field: CpuField) -> String {
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
        CpuField::Load => missing_value(),
        CpuField::Core(index) => observed_percentage(core_usage_pct(shell, index)),
    }
}
