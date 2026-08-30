//! GPU performance-page stat readout construction.
//!
//! Row contract (求同存异): headline facts whose absence is a sampling gap
//! (utilization) keep their row and render the shared dash; facts that simply
//! do not exist on this GPU family (power draw, temperature, VRAM pairs,
//! throttle reason) omit their rows entirely.

use taskmanager_application::i18n;
use taskmanager_core::core::metrics::GpuMetrics;

use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_shell::viewmodel::StatRow;

use super::device_status_i18n_key;

pub(super) fn gpu_stats(g: &GpuMetrics, units: UnitPreferences) -> Vec<StatRow> {
    let mut stats: Vec<StatRow> = vec![
        StatRow::text(
            i18n::t("device.status"),
            Some(i18n::t(device_status_i18n_key(g.device_state.status)).into()),
        ),
        StatRow::text(
            i18n::t("common.utilization"),
            g.current_utilization_pct()
                .map(|percentage| format!("{:.0}%", percentage.round())),
        ),
    ];
    if let Some(name) = g
        .marketing_name
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        stats.push(StatRow::text(
            i18n::t("gpu.marketing_name"),
            Some(name.to_owned()),
        ));
    }
    if let Some(api) = &g.graphics_api {
        if let Some(version) = api
            .opengl_version
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            stats.push(StatRow::text(
                i18n::t("gpu.opengl_version"),
                Some(version.to_owned()),
            ));
        }
        if let Some(version) = api
            .vulkan_version
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            stats.push(StatRow::text(
                i18n::t("gpu.vulkan_version"),
                Some(version.to_owned()),
            ));
        }
    }

    // ── Split VRAM: dedicated on-card (amdgpu) + shared GTT aperture ──

    // ── Live core clock + advertised max (Intel i915/xe tile freq). ──
    // The clock reads 0 only when fully unavailable; the reader already falls
    // back to `cur_freq` so an RC6-idle GT won't show "0 MHz" here.
    if let Some(mhz) = g.current_frequency_mhz() {
        stats.push(StatRow::text(
            i18n::t("common.clock"),
            Some(format!("{mhz} MHz")),
        ));
    }
    if let Some(mhz) = g.current_max_frequency_mhz() {
        stats.push(StatRow::text(
            i18n::t("gpu.max_clock"),
            Some(format!("{mhz} MHz")),
        ));
    }
    // ── Idle residency: only the GPU families that expose the counter ──
    // (Intel RC6). Absent on amdgpu/NVML — omit rather than park a dash.
    if let Some(percentage) = g.current_idle_residency_pct() {
        stats.push(StatRow::text(
            i18n::t("gpu.idle_residency"),
            Some(format!("{:.0}%", percentage.round())),
        ));
    }
    // ── Power draw (amdgpu hwmon / NVML; None on Intel sysfs — no xe hwmon). ──
    if let Some(w) = g.current_power_w() {
        stats.push(StatRow::text(
            i18n::t("common.power"),
            Some(format!("{w:.1} W")),
        ));
    }
    // ── Driver (basename of device/driver symlink — xe / i915 / amdgpu). ──
    if let Some(d) = &g.driver {
        stats.push(StatRow::text(i18n::t("common.driver"), Some(d.clone())));
    }
    // ── Driver version (registry DriverVersion / NVML sys version). ──
    // Absent on drivers that expose no versioned release — omit the row.
    if let Some(version) = g
        .driver_version
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        stats.push(StatRow::text(
            i18n::t("gpu.driver_version"),
            Some(version.to_owned()),
        ));
    }
    // ── Temperature: absent sensor omits the row (no fabricated "— °C"). ──
    if let Some(value) = g.current_temperature_c() {
        stats.push(StatRow::text(
            i18n::t("common.temperature"),
            Some(format!("{:.0} \u{b0}C", value.round())),
        ));
    }

    // ── Split VRAM: dedicated on-card (amdgpu) + shared GTT aperture ──
    // Only shown when the vendor actually exposes a non-zero total, so Intel
    // iGPUs (unified memory, all-zero) don't print a misleading "0.0 / 0.0 GiB".
    if let (Some(used), Some(total)) = (
        g.current_dedicated_vram_used_bytes(),
        g.current_dedicated_vram_total_bytes(),
    ) && total > 0
    {
        stats.push(StatRow::pair(
            i18n::t("gpu.dedicated_vram"),
            Some(units.format_quantity_pair(used, total, QuantityFamily::Memory, false)),
        ));
    }
    if let (Some(used), Some(total)) = (
        g.current_shared_vram_used_bytes(),
        g.current_shared_vram_total_bytes(),
    ) && total > 0
    {
        stats.push(StatRow::pair(
            i18n::t("gpu.shared_vram"),
            Some(units.format_quantity_pair(used, total, QuantityFamily::Memory, false)),
        ));
    }
    // Total video memory: dedicated + shared. Windows fills the shared half
    // from PDH `\GPU Adapter Memory(*)\Shared Usage` (WDDM 2.0+), so the pair
    // is a real observed total, never an all-system-RAM label.
    if let (Some(used), Some(total)) = (
        g.current_memory_used_bytes(),
        g.current_memory_total_bytes(),
    ) {
        stats.push(StatRow::pair(
            i18n::t("gpu.vram"),
            Some(units.format_quantity_pair(used, total, QuantityFamily::Memory, false)),
        ));
    }
    // ── Per-engine utilization (amdgpu graphics/compute/copy/decode/encode) ──
    for e in &g.engines {
        stats.push(StatRow::text(
            e.name.clone(),
            Some(format!("{:.0}%", e.usage_pct.round())),
        ));
    }
    // ── GPU throttling reason (amdgpu hwmon) ──
    if let Some(reason) = g
        .current_throttle_reason_text()
        .filter(|reason| !reason.is_empty())
    {
        stats.push(StatRow::text(i18n::t("gpu.throttling"), Some(reason)));
    }
    if let Some(slot) = g.pci_slot.as_deref().filter(|slot| !slot.trim().is_empty()) {
        stats.push(StatRow::text(
            i18n::t("gpu.pci_slot"),
            Some(slot.to_owned()),
        ));
    }
    stats
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VramCompositionData {
    pub(super) dedicated_total: u64,
    pub(super) dedicated_used: u64,
    pub(super) shared_total: u64,
    pub(super) shared_used: u64,
    pub(super) total_capacity: u64,
    pub(super) total_used: u64,
}

pub(super) fn vram_composition_data(g: &GpuMetrics) -> Option<VramCompositionData> {
    // A composition bar needs a complete pair for each family it paints. If
    // one counter is unavailable, treating that family as 0 would inflate the
    // free segment and make a partial provider result look like a full GPU
    // memory budget. The scalar rows above still show every independently
    // proven pair.
    let (Some(dedicated_used), Some(dedicated_total)) = (
        g.current_dedicated_vram_used_bytes(),
        g.current_dedicated_vram_total_bytes(),
    ) else {
        return None;
    };
    let (Some(shared_used), Some(shared_total)) = (
        g.current_shared_vram_used_bytes(),
        g.current_shared_vram_total_bytes(),
    ) else {
        return None;
    };
    if dedicated_total == 0 && shared_total == 0 {
        return None;
    }
    let dedicated_used = dedicated_used.min(dedicated_total);
    let shared_used = shared_used.min(shared_total);
    let total_capacity = dedicated_total.saturating_add(shared_total);
    let total_used = dedicated_used
        .saturating_add(shared_used)
        .min(total_capacity);
    Some(VramCompositionData {
        dedicated_total,
        dedicated_used,
        shared_total,
        shared_used,
        total_capacity,
        total_used,
    })
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_gpu_stats_tests.rs"]
mod tests;
