//! CPU detail-field projection over the cached shell snapshot.

use super::*;

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
                    taskmanager_shell::presentation::load_average_values_summary(load),
                    taskmanager_shell::presentation::load_average_basis_summary(load),
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
            .map_or_else(
                missing_value,
                taskmanager_shell::presentation::pressure_summary,
            ),
        CpuField::Topology => {
            taskmanager_shell::presentation::cpu_topology_summary(cpu).unwrap_or_else(missing_value)
        }
        CpuField::IdleStates => taskmanager_shell::presentation::cpu_idle_state_summary(cpu)
            .unwrap_or_else(missing_value),
        CpuField::PowerLimits => taskmanager_shell::presentation::cpu_power_limits_summary(cpu)
            .unwrap_or_else(missing_value),
        CpuField::Interrupts => taskmanager_shell::presentation::cpu_interrupt_summary(cpu)
            .unwrap_or_else(missing_value),
        CpuField::ThermalThrottle => thermal_throttle_summary(cpu).unwrap_or_else(missing_value),
        CpuField::Load => missing_value(),
        CpuField::Core(index) => observed_percentage(core_usage_pct(shell, index)),
    }
}

/// Compact per-package summary of the cumulative CPU thermal-throttle trigger
/// counters (`power.thermal-throttle-events`): one `S{package_id}` segment per
/// package that observed at least one counter, naming the package-level and
/// per-core event counts. The always-mounted diagnostic row keeps the shared
/// dash when no package observed a counter; an observed package with an
/// unobserved sibling keeps the dash for that sibling — never a fabricated
/// zero.
fn thermal_throttle_summary(cpu: &CpuMetrics) -> Option<String> {
    let mut packages = Vec::new();
    for package in &cpu.packages {
        if package.package_throttle_count.is_none() && package.core_throttle_count.is_none() {
            continue;
        }
        let package_count = package
            .package_throttle_count
            .map_or_else(missing_value, |value| value.to_string());
        let core_count = package
            .core_throttle_count
            .map_or_else(missing_value, |value| value.to_string());
        packages.push(format!(
            "S{} {} {package_count} · {} {core_count}",
            package.package_id,
            t("cpu.throttle_package"),
            t("cpu.throttle_core"),
        ));
    }
    (!packages.is_empty()).then(|| packages.join(" | "))
}
