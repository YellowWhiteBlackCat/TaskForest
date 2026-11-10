//! Hardware-specific statistics rail cards for the Performance page.

pub(super) use super::device_stats_more::{
    battery_stats_card, fan_stats_card, gpu_stats_card, network_stats_card,
};
use crate::pages::performance::metrics::cpu::cpu_field_text;
use crate::pages::performance::metrics::{cpu_metrics, disk_stat_rows, memory_metrics};
use crate::pages::performance::{CpuField, DynField};
use crate::palette::{UiPalette, space_2};
use crate::widgets::controls::stat_row_scene;
use crate::window::{Role, TextRole};
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{AlignItems, FlexDirection, Node, Overflow, UiRect, Val, percent, rem};
use bevy::ui::widget::Text;
use taskmanager_application::i18n;
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::missing_value;
use taskmanager_ui_contract::IconId;

use crate::icons::{IconInk, IconPlate};

const MAX_VISIBLE_STATS_ROWS: usize = 15;

pub(super) fn capped_stats_rows(
    mut rows: Vec<Box<dyn Scene>>,
    palette: &UiPalette,
) -> Vec<Box<dyn Scene>> {
    if rows.len() > MAX_VISIBLE_STATS_ROWS {
        let omitted = rows.len() - (MAX_VISIBLE_STATS_ROWS - 1);
        rows.truncate(MAX_VISIBLE_STATS_ROWS - 1);
        let label = i18n::more_rows_label(omitted);
        rows.push(Box::new(bsn! {
            Node {
                width: percent(100),
                min_height: rem(palette.control_height_rem),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::vertical(Val::Px(space_2())),
            }
            Children [
                Text(label)
                TextRole(Role::Caption)
            ]
        }));
    }
    rows
}

pub(super) fn stat_text_row(label: String, value: String, palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(stat_row_scene(
        label,
        Box::new(bsn! {
            Text(value)
            TextRole(Role::Body)
            TextLayout { linebreak: LineBreak::NoWrap }
        }),
        palette,
    ))
}

/// A stat row whose fact crossed its typed level: the row wears the alert
/// plate beside its value, so the crossing is visible where the number is
/// read. The plate is a semantic bitmap sibling, never a decoration
/// codepoint inside the value text (the tofu law).
pub(super) fn stat_alert_row(label: String, value: String, palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(stat_row_scene(
        label,
        Box::new(bsn! {
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_2()),
                flex_shrink: 1.0,
                overflow: Overflow::clip_x(),
            }
            Children [
                Node {
                    width: rem(palette.icon_rem),
                    height: rem(palette.icon_rem),
                    flex_shrink: 0.0,
                }
                IconPlate({ IconId::Alert })
                IconInk({ palette.warning_color })
            --
                Text(value)
                TextRole(Role::Body)
                TextLayout { linebreak: LineBreak::NoWrap }
            ]
        }),
        palette,
    ))
}

pub(super) fn stat_dynamic_text_row(
    shell: &ShellApp,
    label: String,
    key: crate::pages::performance::metrics::stat_value::StatKey,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let value = crate::pages::performance::metrics::stat_value::stat_value(shell, key);
    stat_text_row(label, value, palette)
}

pub(super) fn cpu_stats_card(shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    let mut rows: Vec<Box<dyn Scene>> = Vec::new();
    for (label, field) in [
        (t("common.utilization"), CpuField::Usage),
        (t("cpu.frequency"), CpuField::Frequency),
        (t("common.temperature"), CpuField::Temperature),
        (t("common.power"), CpuField::Power),
        (t("cpu.processes"), CpuField::Processes),
        (t("common.threads"), CpuField::Threads),
        (t("perf.stall"), CpuField::Pressure),
        (t("system.load_normalized"), CpuField::Load),
        (t("common.up_time"), CpuField::UpTime),
    ] {
        rows.push(Box::new(stat_row_scene(
            label.to_owned(),
            super::marked_text_scene(
                cpu_field_text(shell, field),
                Role::Mono,
                DynField::Cpu(field),
            ),
            palette,
        )));
    }
    if let Some(cpu) = cpu_metrics(shell) {
        for (label, value) in [
            (
                t("system_about.cores"),
                cpu.physical_cores.map(|cores| cores.to_string()),
            ),
            (
                t("common.logical_cores"),
                cpu.logical_cores.map(|cores| cores.to_string()),
            ),
        ] {
            let display_value = value.unwrap_or_else(missing_value);
            rows.push(Box::new(stat_row_scene(
                label.to_owned(),
                Box::new(bsn! {
                    Text(display_value)
                    TextRole(Role::Mono)
                }),
                palette,
            )));
        }
    }
    for (label, field) in [
        (t("cpu.power_limits"), CpuField::PowerLimits),
        (t("cpu.thermal_throttle"), CpuField::ThermalThrottle),
        (t("cpu.thermal_status"), CpuField::ThermalStatus),
        (t("cpu.idle_states"), CpuField::IdleStates),
        (t("cpu.interrupts"), CpuField::Interrupts),
        (t("cpu.topology"), CpuField::Topology),
        (t("common.l1_data_cache"), CpuField::L1dCache),
        (t("common.l1_instruction_cache"), CpuField::L1iCache),
        (t("common.l2_cache"), CpuField::L2Cache),
        (t("common.l3_cache"), CpuField::L3Cache),
        (t("cpu.performance_cores"), CpuField::PerformanceCores),
        (t("cpu.efficiency_cores"), CpuField::EfficiencyCores),
        (t("cpu.low_power_cores"), CpuField::LowPowerCores),
    ] {
        rows.push(Box::new(stat_row_scene(
            label.to_owned(),
            super::marked_text_scene(
                cpu_field_text(shell, field),
                Role::Mono,
                DynField::Cpu(field),
            ),
            palette,
        )));
    }
    Box::new(super::summary_card_scene(
        capped_stats_rows(rows, palette),
        palette,
    ))
}

pub(super) fn memory_stats_card(shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    let mut rows: Vec<Box<dyn Scene>> = Vec::new();
    use crate::pages::performance::metrics::stat_value::StatKey;
    if memory_metrics(shell).is_some() {
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.in_use").to_owned(),
            StatKey::Used,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.available").to_owned(),
            StatKey::Available,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.hardware_reserved").to_owned(),
            StatKey::HardwareReserved,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.cached").to_owned(),
            StatKey::Cached,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.buffers").to_owned(),
            StatKey::Buffers,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.swap").to_owned(),
            StatKey::Swap,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("common.speed").to_owned(),
            StatKey::Speed,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.slots").to_owned(),
            StatKey::Slots,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.committed").to_owned(),
            StatKey::Committed,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.zram_swap").to_owned(),
            StatKey::ZramSwap,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.compression_ratio").to_owned(),
            StatKey::CompressionRatio,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.zram_ram_used").to_owned(),
            StatKey::ZramRamUsed,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.zswap").to_owned(),
            StatKey::Zswap,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.usage_rate").to_owned(),
            StatKey::UsageRate,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.swap_in_rate").to_owned(),
            StatKey::SwapInRate,
            palette,
        ));
        rows.push(stat_dynamic_text_row(
            shell,
            t("mem.swap_out_rate").to_owned(),
            StatKey::SwapOutRate,
            palette,
        ));
    }
    Box::new(super::summary_card_scene(
        capped_stats_rows(rows, palette),
        palette,
    ))
}

pub(super) fn disk_stats_card(shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    let rows = disk_stat_rows(shell)
        .into_iter()
        .map(|row| {
            if row.alert {
                stat_alert_row(row.label, row.value, palette)
            } else {
                stat_text_row(row.label, row.value, palette)
            }
        })
        .collect();
    Box::new(super::summary_card_scene(
        capped_stats_rows(rows, palette),
        palette,
    ))
}
