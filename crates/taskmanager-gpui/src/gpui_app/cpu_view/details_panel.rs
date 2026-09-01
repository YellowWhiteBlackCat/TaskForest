//! CPU detail/specification surface kept separate from the graph renderer.

use gpui::{Div, InteractiveElement, ParentElement, Styled, div, px};

use crate::gpui_app::formatting;
use taskmanager_application::i18n;
use taskmanager_core::core::hardware::{CoreBreakdown, HardwareInfo};
use taskmanager_core::core::metrics::{CpuMetrics, SystemSnapshot};
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::data::key_value_row::KeyValueRow;

use super::msr_readouts::MsrReadoutsModel;
use super::package_power::PackagePowerModel;
use super::{EscalationReadouts, format_uptime, stats::CpuDetailsStats};

const CPU_SPEC_ROW_SLOT: f32 = 26.0;
const CPU_DETAILS_RESERVE: f32 = 194.0;
const CPU_PACKAGE_POWER_RESERVE: f32 = 120.0;
const CPU_MSR_RESERVE: f32 = 180.0;
const MAX_CPU_SPEC_ROWS: usize = 32;

pub(super) fn render_pinned(
    theme: &Theme,
    snap: &SystemSnapshot,
    hardware: &HardwareInfo,
    live: &CpuDetailsStats,
    units: UnitPreferences,
    escalation: &EscalationReadouts,
    content_height: f32,
) -> Div {
    let EscalationReadouts {
        package_power,
        msr_readouts,
    } = escalation;
    let cpu = &snap.cpu;
    let spec_rows = spec_row_budget(content_height, package_power, msr_readouts);
    // Per-core average + maximum temperature, surfaced as a note beneath the
    // package reading. The typed CPU sensor source exposes Intel `coretemp`
    // channels as genuine per-core values and keeps AMD `k10temp` die readings
    // at package scope instead of fabricating one value per logical core. Show
    // the note only when at least two real core channels report; a single-core
    // or empty observation falls back to the package line alone. The Linux
    // provider emits typed `None` for unmapped logical cores (topology-mapped
    // SMT siblings still carry their parent physical core's reading), so the
    // legacy `>0.0` sentinel is no longer needed here.
    div()
        .debug_selector(|| "tm-cpu-details-panel".to_string())
        .w_full()
        .h_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        // Top stats as a clean label-left / value-right list (Win11 Task Manager /
        // Mission Center style): every row is one aligned line instead of the prior
        // alternating 2-up big-number blocks + full-width blocks, which read as a
        // cluttered "a:b" rather than a scannable column. Mirrors the row geometry
        // of `spec_grid` below for one unified details-panel aesthetic.
        .child(live_stats(theme, snap, live))
        // Package-power subsection (the escalation-backed RAPL lane). Absent
        // entirely while `Hidden`: no session and no registered lane on this
        // host renders nothing, never a placeholder.
        .children(
            matches!(package_power, PackagePowerModel::Packages(_))
                .then(|| super::package_power::render_package_power_section(theme, package_power)),
        )
        // MSR-readout subsection (the escalation-backed CpuMsr lane). Same
        // absence discipline: `Hidden` renders nothing at all.
        .children(
            matches!(msr_readouts, MsrReadoutsModel::Rows(_))
                .then(|| super::msr_readouts::render_msr_readouts_section(theme, msr_readouts)),
        )
        // Hairline section divider between the live stats and the static spec list.
        .child(
            div()
                .h(px(1.0))
                .w_full()
                .bg(taskmanager_ui::theme_binding::fill(theme.border)),
        )
        .child(spec_grid(theme, cpu, hardware, units, spec_rows))
}

fn live_stats(theme: &Theme, snap: &SystemSnapshot, live: &CpuDetailsStats) -> Div {
    let mut col = div()
        .debug_selector(|| "tm-cpu-details-live-stats".to_string())
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_5,
        ))
        .w_full();
    if let Some(utilization) = live.utilization.as_deref() {
        col = col.child(kv_row(theme, i18n::t("common.utilization"), utilization));
    }
    if let Some(speed) = live.speed.as_deref() {
        col = col.child(kv_row_with_note(
            theme,
            i18n::t("common.speed"),
            speed,
            live.speed_note,
        ));
    }
    if let Some(temperature) = live.temperature.as_deref() {
        col = col.child(kv_row_with_note(
            theme,
            i18n::t("common.temperature"),
            temperature,
            live.temperature_note.as_deref(),
        ));
    }
    col = col.child(kv_row(
        theme,
        i18n::t("cpu.processes"),
        &snap.processes.to_string(),
    ));
    if let Some(threads) = snap.threads {
        col = col.child(kv_row(
            theme,
            i18n::t("common.threads"),
            &threads.to_string(),
        ));
    }
    col.child(kv_row(
        theme,
        i18n::t("common.up_time"),
        &format_uptime(snap.uptime_secs),
    ))
}

/// A clean label-left / value-right details row (Win11 TM / Mission Center style):
/// dim label on the left, value flush-right, baseline-aligned so a column of
/// these rows reads as one scannable, consistently right-aligned list. Used by
/// BOTH the CPU details panel's top stats AND the spec list below — one shared
/// geometry for a unified panel.
pub(super) fn kv_row(theme: &Theme, label: &str, value: &str) -> Div {
    // The shared row owns the shrinkable label/value geometry so live stats
    // and specification rows cannot drift apart.
    KeyValueRow::new(label, value, theme.palette())
        .selectable_value(gpui::ElementId::Name(
            format!("cpu-detail-value:{label}").into(),
        ))
        .render()
}

/// Like [`kv_row`] but with an optional right-aligned dim sub-line beneath the
/// value — used for the per-core average + maximum temperature note beneath the
/// package reading. `None` renders as a plain [`kv_row`] (in a column wrapper).
fn kv_row_with_note(theme: &Theme, label: &str, value: &str, note: Option<&str>) -> Div {
    let mut col = div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_1,
        ))
        .w_full()
        .child(kv_row(theme, label, value));
    if let Some(n) = note {
        col = col.child(
            div()
                .flex()
                .justify_end()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(n.to_string()),
        );
    }
    col
}

fn spec_grid(
    theme: &Theme,
    cpu: &CpuMetrics,
    hardware: &HardwareInfo,
    units: UnitPreferences,
    max_rows: Option<usize>,
) -> Div {
    // Performance details are intentionally static: the page's only scrollable
    // surface is the device selector on the left. Missing optional facts were
    // removed by `cpu_spec_rows`; the remaining rows are painted in place so
    // the right rail cannot hide a second implicit list viewport.
    let rows = cpu_spec_rows(cpu, hardware, units);
    let row_limit = max_rows.map_or(rows.len(), |limit| {
        if rows.len() > limit {
            limit.saturating_sub(1)
        } else {
            limit
        }
    });
    let omitted = rows.len().saturating_sub(row_limit);
    let mut column = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full();
    for (index, (label, value)) in rows.into_iter().take(row_limit).enumerate() {
        let row = kv_row(theme, &label, &value);
        #[cfg(any(test, feature = "test-support"))]
        let row = row.debug_selector(move || format!("tm-cpu-spec:{index}"));
        #[cfg(not(any(test, feature = "test-support")))]
        let _ = index;
        column = column.child(row);
    }
    if omitted > 0 {
        column = column.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(i18n::t("common.more_rows").replace("{count}", &omitted.to_string())),
        );
    }
    column
}

fn spec_row_budget(
    content_height: f32,
    package_power: &PackagePowerModel,
    msr_readouts: &MsrReadoutsModel,
) -> Option<usize> {
    if content_height <= 0.0 {
        return None;
    }
    let mut reserved = CPU_DETAILS_RESERVE;
    if matches!(package_power, PackagePowerModel::Packages(_)) {
        reserved += CPU_PACKAGE_POWER_RESERVE;
    }
    if matches!(msr_readouts, MsrReadoutsModel::Rows(_)) {
        reserved += CPU_MSR_RESERVE;
    }
    let available = (content_height - reserved).max(0.0);
    let mut rows = 0_usize;
    let mut used = 0.0_f32;
    for _ in 0..MAX_CPU_SPEC_ROWS {
        if used + CPU_SPEC_ROW_SLOT > available {
            break;
        }
        rows += 1;
        used += CPU_SPEC_ROW_SLOT;
    }
    Some(rows)
}

/// Pure data-layer builder for the CPU specification rows (ADR-020
/// single-source / data-render split, ARCH.md §4): the exact ordered
/// (label, value) list `spec_grid` paints, free of any element/theme
/// concern so surface tests assert the projection directly. The socket
/// fold is shared with the System page via [`sockets_row`].
pub(crate) fn cpu_spec_rows(
    cpu: &CpuMetrics,
    hardware: &HardwareInfo,
    units: UnitPreferences,
) -> Vec<(String, String)> {
    let cache = |kb: Option<u64>| -> String {
        // `*_cache_kb` from detect_cpu_cache is in KiB; format_mib_2 expects
        // BYTES (it divides by 1024^2). Convert KiB -> bytes first, else every
        // cache reads ~1/1024 of its real size and L1 rounds to "0.00 MiB"
        // (matching system_view::fmt_cache_kb's `value * 1024`).
        kb.map_or_else(formatting::missing_value, |v| {
            units.format_quantity(v * 1024, QuantityFamily::Memory, false)
        })
    };
    // Base speed = STATIC advertised base clock (NOT the live frequency, which
    // belongs in the "Speed" row above and legitimately fluctuates); the
    // optional-MHz spelling is the shared `formatting::optional_ghz`.
    let base_speed = formatting::optional_ghz(hardware.base_freq_mhz);

    // Heterogeneous topology gets one aligned row per core class. A combined
    // "4 P + 8 E + 4 LP-E" value is hard to scan and truncates in this 280px
    // panel; separate label-left/count-right rows preserve the column rhythm.
    let multiplier = cpu
        .clock_multiplier(hardware.base_freq_mhz)
        .map_or_else(formatting::missing_value, |multiplier| {
            format!("\u{00d7}{multiplier:.1}")
        });
    let mut rows: Vec<(String, String)> = cpu_identity_rows(hardware);
    rows.extend([
        (i18n::t("cpu.base_speed").to_string(), base_speed),
        (i18n::t("cpu.multiplier").to_string(), multiplier),
        // Real socket count from HardwareInfo (distinct physical_package_id
        // values), shared with system_view.rs. Was hardcoded "1".
        sockets_row(hardware.sockets),
        (
            i18n::t("common.cores").to_string(),
            cpu.physical_cores
                .map_or_else(formatting::missing_value, |cores| cores.to_string()),
        ),
    ]);
    rows.extend(heterogeneous_core_rows(&hardware.core_breakdown));
    rows.extend([
        (
            i18n::t("cpu.logical_processors").to_string(),
            cpu.logical_cores
                .map_or_else(formatting::missing_value, |cores| cores.to_string()),
        ),
        // Real hypervisor label from HardwareInfo ("None" on bare metal);
        // mirrors system_view.rs. Was hardcoded "KVM / VT-x".
        (
            i18n::t("common.virtualization").to_string(),
            hardware
                .virt
                .clone()
                .unwrap_or_else(|| i18n::t("common.none").to_string()),
        ),
        (
            i18n::t("common.l1_data_cache").to_string(),
            cache(cpu.l1d_cache_kb),
        ),
        (
            i18n::t("common.l1_instruction_cache").to_string(),
            cache(cpu.l1i_cache_kb),
        ),
        (
            i18n::t("common.l2_cache").to_string(),
            cache(cpu.l2_cache_kb),
        ),
        (
            i18n::t("common.l3_cache").to_string(),
            cache(cpu.l3_cache_kb),
        ),
    ]);
    // Policy rows only exist when the platform actually reports them: Windows
    // has no energy-preference scalar, Linux has no Windows power-manager
    // driver name. A missing fact is an absent row, never a dash slot.
    if let Some(driver) = &cpu.performance_policy.frequency_implementation {
        rows.push((i18n::t("cpu.cpufreq_driver").to_string(), driver.clone()));
    }
    if let Some(governor) = &cpu.performance_policy.active_policy {
        rows.push((
            i18n::t("cpu.cpufreq_governor").to_string(),
            governor.clone(),
        ));
    }
    if let Some(preference) = &cpu.performance_policy.energy_preference {
        rows.push((
            i18n::t("cpu.power_preference").to_string(),
            preference.clone(),
        ));
    }
    // The Performance detail rail has no scroll surface. Keep only accepted
    // facts here; an unavailable optional value is represented by absence and
    // its authorization/recovery affordance belongs to the Settings
    // permission center, never a dashed row in this panel.
    let missing = formatting::missing_value();
    rows.retain(|(_, value)| !value.trim().is_empty() && value != &missing);
    rows
}

/// Single source for the CPUID identity rows (ADR-020): the CPU details
/// panel's spec list and the System page's CPU section share one ordered
/// projection of `HardwareInfo::cpu_identity`. The rows are conditional — a
/// platform that cannot probe the identity (non-x86 host, fixture inventory)
/// renders no row at all rather than a dash slot, matching the policy-row
/// discipline above.
pub(crate) fn cpu_identity_rows(hardware: &HardwareInfo) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if let Some(codename) = hardware.cpu_identity.codename() {
        rows.push((
            i18n::t("system.cpu_codename").to_string(),
            codename.to_string(),
        ));
    }
    if let Some(process) = hardware.cpu_identity.process_node() {
        rows.push((
            i18n::t("system.cpu_process").to_string(),
            process.to_string(),
        ));
    }
    if let Some(vendor) = hardware.cpu_identity.vendor_id.as_deref() {
        rows.push((i18n::t("system.cpu_vendor").to_string(), vendor.to_string()));
    }
    if let Some(code) = hardware.cpu_identity.code() {
        rows.push((i18n::t("system.cpu_identity").to_string(), code));
    }
    rows
}

/// Single source for the socket-count fold (ADR-020): the CPU details panel's
/// spec list and the System page's CPU section share one label + honest-dash
/// projection of `HardwareInfo::sockets`.
pub(crate) fn sockets_row(sockets: Option<u16>) -> (String, String) {
    (
        i18n::t("common.sockets").to_string(),
        sockets.map_or_else(formatting::missing_value, |count| count.to_string()),
    )
}

pub(crate) fn heterogeneous_core_rows(core: &CoreBreakdown) -> Vec<(String, String)> {
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
    .map(|(label, count)| (i18n::t(label).to_string(), count.to_string()))
    .collect()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_cpu_view_details_panel_tests.rs"]
mod tests;
