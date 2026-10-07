//! GPU device fact projections for the Performance page.

use super::*;

/// One GPU block's joined fact line; each fact keeps its own dash-on-missing
/// semantics (TUI `gpu_data` parity via the shared formatters).
pub(crate) fn gpu_fact_line(gpu: &GpuMetrics) -> String {
    let mut values = vec![
        observed_percentage(gpu.current_utilization_pct()),
        gpu.current_temperature_c()
            .filter(|value| value.is_finite())
            .map_or_else(missing_value, temperature_c),
        gpu.current_frequency_mhz()
            .map_or_else(missing_value, |mhz| megahertz(mhz as f32)),
        gpu.current_power_w()
            .filter(|value| value.is_finite())
            .map_or_else(missing_value, power_w),
        gpu_memory_line(gpu),
    ];
    if let Some(connected) = gpu.display_connected {
        values.push(format!(
            "{} {}",
            t("gpu.display_output"),
            t(if connected { "common.yes" } else { "common.no" })
        ));
    }
    if let Some(version) = gpu
        .vbios_version
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        values.push(format!("{} {version}", t("gpu.vbios_version")));
    }
    if let Some(api) = gpu.graphics_api.as_ref()
        && let Some(version) = api
            .mesa_version
            .as_deref()
            .filter(|value| !value.is_empty())
    {
        values.push(format!("{} {version}", t("gpu.mesa_version")));
    }
    if let Some(rpm) = gpu.current_fan_speed_rpm() {
        values.push(format!("{} {rpm} RPM", t("fan.rpm")));
    }
    if let Some(pct) = gpu
        .current_fan_speed_pct()
        .filter(|value| value.is_finite())
    {
        values.push(format!("{} {pct:.0}%", t("fan.pwm")));
    }
    if let Some(bandwidth) = gpu
        .memory_bandwidth_gbps
        .filter(|value| value.is_finite() && *value > 0.0)
    {
        values.push(format!("{} {bandwidth:.1} GB/s", t("gpu.memory_bandwidth")));
    }
    if let Some(depth) = gpu.queue_depth {
        values.push(format!("{} {depth}", t("gpu.queue_depth")));
    }
    values.join(" · ")
}
