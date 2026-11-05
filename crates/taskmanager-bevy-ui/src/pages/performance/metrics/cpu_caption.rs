//! Sidebar CPU caption projection.

use super::*;
use taskmanager_shell::presentation::cpu_clock_readout;

/// The sidebar CPU row's two GPUI-aligned captions: brand, then usage,
/// frequency, package temperature, and optional per-core range.
pub(crate) fn cpu_sidebar_caption(shell: &ShellApp) -> (String, String) {
    let Some(cpu) = cpu_metrics(shell) else {
        return (missing_value(), missing_value());
    };
    let brand = cpu
        .brand
        .as_deref()
        .map(str::trim)
        .filter(|brand| !brand.is_empty())
        .map_or_else(missing_value, str::to_owned);
    let usage = cpu
        .current_global_usage_pct()
        .filter(|value| value.is_finite())
        .map_or_else(missing_value, |value| format!("{value:.0}%"));
    let frequency = cpu.current_frequency_mhz().map(cpu_clock_readout);
    let mut line2 = match (cpu.current_temperature_c(), frequency) {
        (Some(temperature), Some(frequency)) => {
            format!("{usage}  {frequency} ({temperature:.0} °C)")
        }
        (Some(temperature), None) => format!("{usage} ({temperature:.0} °C)"),
        (None, Some(frequency)) => format!("{usage}  {frequency}"),
        (None, None) => usage,
    };
    let mut minimum = f32::INFINITY;
    let mut maximum = f32::NEG_INFINITY;
    for index in 0..cpu.current_core_temperature_len() {
        if let Some(temperature) = cpu.current_core_temperature_c(index) {
            minimum = minimum.min(temperature);
            maximum = maximum.max(temperature);
        }
    }
    if minimum.is_finite() {
        if (maximum - minimum).abs() < 0.5 {
            line2.push_str(&format!("  ·  cores {minimum:.0} °C"));
        } else {
            line2.push_str(&format!("  ·  cores {minimum:.0}..{maximum:.0} °C"));
        }
    }
    (brand, line2)
}
