//! The System page: one typed key/value projection of the host identity,
//! firmware, CPU, memory, and session facts the shell already owns.
//!
//! **Composition model** (the page-proxy contract in [`crate::pages`]): the
//! static tree is the title + status line + the EMPTY body container; the
//! body's only author is `paint_system`, bound by the root's on-insert
//! hook (the same single-authority shape the History page uses — a static
//! body here would race the paint pass into a doubled surface).
//!
//! Semantics follow the shared System vocabulary: labels come from the
//! `system.*` locale keys (the same fold GPUI's System page uses), a fact
//! the platform did not supply renders the shared dash — never a
//! compile-target guess — and an inventory that has not arrived yet is the
//! honest waiting state, not zeros.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    BackgroundColor, BorderRadius, FlexDirection, FlexWrap, Node, Overflow, UiRect, Val, percent,
    px,
};
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::sensors::SensorCenterSnapshot;
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_shell::SystemProjectionStore;
use taskmanager_shell::presentation::{bytes, missing_value};

pub(crate) mod dashboard;
mod hardware;
use hardware::hardware_fact_rows;

pub(crate) mod thermal;
pub(crate) use thermal::thermal_zone_rows;

use crate::app::{FrontendTrack, Page, PageContext};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, space_2, space_4, space_8, space_12};
use crate::widgets::controls::detail_row_scene;
use crate::widgets::layout::SystemDashboardBudget;
use crate::window::{Role, TextRole, WindowPalette};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::ComputedNode;
use bevy::ui_widgets::ScrollArea;
use dashboard::{SystemDashboardState, SystemDashboardToolbar};
use taskmanager_application::SmbiosMemoryState;
use taskmanager_application::system_timeline::SystemPageSection;
use taskmanager_core::core::metrics::SmbiosMemorySnapshot;
use taskmanager_core::core::npu::NpuEngineKind;
use taskmanager_core::core::npu::NpuInventorySnapshot;
use taskmanager_shell::presentation::health_score_for_snapshot;
use taskmanager_shell::presentation::smbios_memory_inventory_rows;

pub(crate) mod diagnostic_modal;

/// The page's single body container. Painted exclusively by
/// [`paint_system`], which the root's on-insert hook binds.
#[derive(Component, Clone, Default)]
pub(crate) struct SystemBody;

/// The one status line the paint pass rewrites (waiting/fact counts).
#[derive(Component, Clone, Default)]
pub(crate) struct SystemStatusLine;

#[derive(Component, Clone, Default)]
pub(crate) struct SystemActions;

#[derive(Component, Clone, Default)]
pub(crate) struct MemoryInventoryAnchor;

/// The page's root: mounting it binds the paint observers.
#[derive(Component, Clone, Default)]
pub(crate) struct SystemPageRoot;

pub(crate) mod health;
pub(crate) mod paint;
pub(crate) use paint::register;

// ---- pure projection ------------------------------------------------------

/// Pre-folded summary values for the System page KPI cards, matching GPUI and Iced.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SystemSummaryModel {
    /// Latest global CPU utilization, already folded to a display string.
    pub(crate) cpu: String,
    /// Latest observed memory utilization, already folded to a display string.
    pub(crate) memory: String,
    /// Observed process count (`None` while no inventory has arrived).
    pub(crate) processes: Option<usize>,
    /// Live active-alert count from the shell's evaluation mirror.
    pub(crate) active_alerts: usize,
    /// Transparent score plus the first bounded evidence deductions.
    pub(crate) health: Option<String>,
}

/// Fold the shell projection into the System page KPI summary values (pure).
/// Matches Iced `DashboardSummaryModel` and GPUI `summary_card` readouts.
pub(crate) fn system_summary_model(projection: &SystemProjectionStore) -> SystemSummaryModel {
    let snapshot = projection.snapshot.as_ref();
    SystemSummaryModel {
        cpu: snapshot
            .and_then(|snapshot| snapshot.cpu.current_global_usage_pct())
            .map_or_else(missing_value, |value| format!("{value:.1}%")),
        memory: snapshot
            .and_then(|snapshot| snapshot.memory.used_percentage_observed())
            .map_or_else(missing_value, |value| format!("{value:.1}%")),
        processes: projection
            .processes
            .as_ref()
            .map(|processes| processes.len()),
        active_alerts: projection.alert_active.len(),
        // KPI tiles own their label separately; keep the value slot to the
        // compact score so the full explanatory health sentence cannot be
        // clipped inside a narrow fifth card. The detailed deduction bill is
        // still available through the shared score summary on dense surfaces.
        health: snapshot
            .and_then(health_score_for_snapshot)
            .map(|score| format!("{}/100", score.score)),
    }
}

/// Format total memory size cleanly in binary units (KiB/MiB/GiB), delegated
/// to the shared presentation authority ([`bytes`]).
pub(crate) fn clean_memory_size(total_memory_mb: u64) -> String {
    bytes(total_memory_mb.saturating_mul(1024 * 1024))
}

/// Semantic section of an already-projected System fact.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SystemFactGroup {
    OperatingSystem,
    Cpu,
    Memory,
    MemoryInventory,
    Hardware,
    Npu,
}

/// One label→value fact row. The value is already final display text.
pub(crate) struct SystemFactRow {
    pub(crate) group: SystemFactGroup,
    pub(crate) label: String,
    pub(crate) value: String,
}

fn optional(value: Option<&str>) -> String {
    value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(missing_value)
}

fn joined(first: Option<&str>, second: Option<&str>) -> String {
    match (
        first.filter(|s| !s.trim().is_empty()),
        second.filter(|s| !s.trim().is_empty()),
    ) {
        (Some(first), Some(second)) => format!("{first} {second}"),
        (Some(first), None) => first.to_owned(),
        (None, Some(second)) => second.to_owned(),
        (None, None) => missing_value(),
    }
}

/// The host facts the System page states, in the shared System-page order.
/// Pure; headless tests pin it against fixture hardware.
pub(crate) fn system_fact_rows(
    hardware: Option<&HardwareInfo>,
    smbios: Option<&SmbiosMemorySnapshot>,
    npu: Option<&NpuInventorySnapshot>,
) -> Vec<SystemFactRow> {
    let mut rows = hardware.map(hardware_fact_rows).unwrap_or_default();
    if let Some(smbios) = smbios {
        for (label, value) in smbios_memory_inventory_rows(smbios, UnitPreferences::default()) {
            rows.push(SystemFactRow {
                group: SystemFactGroup::MemoryInventory,
                label,
                value,
            });
        }
    }
    if let Some(npu) = npu.filter(|inv| inv.is_success()) {
        for dev in &npu.devices {
            rows.push(SystemFactRow {
                group: SystemFactGroup::Npu,
                label: format!("{} {}", t("npu.title"), dev.device_id.as_str()),
                value: dev
                    .brand
                    .clone()
                    .unwrap_or_else(|| t("npu.device_title").to_owned()),
            });
            rows.push(SystemFactRow {
                group: SystemFactGroup::Npu,
                label: format!("{} · {}", t("npu.title"), t("common.utilization")),
                value: dev
                    .utilization_pct
                    .current_value()
                    .copied()
                    .filter(|value| value.is_finite())
                    .map_or_else(missing_value, |value| format!("{value:.0}%")),
            });
            for engine in &dev.engines {
                let label = match engine.kind {
                    NpuEngineKind::Compute => t("npu.engine_compute"),
                    NpuEngineKind::Matrix => t("npu.engine_matrix"),
                    NpuEngineKind::Vector => t("npu.engine_vector"),
                    NpuEngineKind::Video => t("npu.engine_video"),
                    NpuEngineKind::Copy => t("npu.engine_copy"),
                    NpuEngineKind::Unknown => t("npu.engine_unknown"),
                };
                rows.push(SystemFactRow {
                    group: SystemFactGroup::Npu,
                    label: format!("{} · {label}", t("npu.title")),
                    value: engine
                        .utilization_pct
                        .current_value()
                        .copied()
                        .filter(|value| value.is_finite())
                        .map_or_else(missing_value, |value| format!("{value:.0}%")),
                });
            }
            for (label, observation) in [
                (t("npu.dedicated_memory"), &dev.memory.dedicated_total_bytes),
                (t("npu.shared_memory"), &dev.memory.shared_total_bytes),
                (t("npu.sram"), &dev.memory.sram_total_bytes),
            ] {
                rows.push(SystemFactRow {
                    group: SystemFactGroup::Npu,
                    label: format!("{} · {label}", t("npu.title")),
                    value: observation
                        .current_value()
                        .copied()
                        .map_or_else(missing_value, bytes),
                });
            }
        }
    }
    rows
}

/// The status line: an inventory that has not arrived states that; one that
/// has states its fact count — never a fabricated zero.
fn status_line_text(
    hardware: Option<&HardwareInfo>,
    smbios: Option<&SmbiosMemorySnapshot>,
    npu: Option<&NpuInventorySnapshot>,
    sensors: Option<&SensorCenterSnapshot>,
) -> String {
    if hardware.is_none() && smbios.is_none() && npu.is_none() && sensors.is_none() {
        t("common.waiting_inventory").to_owned()
    } else {
        let base_count = system_fact_rows(hardware, smbios, npu).len();
        let thermal_count = sensors.map(|s| thermal_zone_rows(s).len()).unwrap_or(0);
        let count = base_count + thermal_count;
        t("system.facts_ready").replacen("{count}", &count.to_string(), 1)
    }
}

// ---- render adapters ------------------------------------------------------

pub(crate) fn fact_row_scene(row: &SystemFactRow, palette: &UiPalette) -> impl Scene + use<> {
    // Inspection facts keep complete values in the owned wrapped row grammar.
    let value = row.value.clone();
    let text_val = row.value.clone();
    let label = row.label.clone();
    let value_scene = Box::new(bsn! {
        Text(text_val)
        TextRole(Role::Body)
        TextLayout { linebreak: LineBreak::WordBoundary }
        Node { width: percent(100), min_width: px(0.0) }
        crate::text_selection::SelectableText(label, value)
    }) as Box<dyn bevy::scene::Scene>;
    detail_row_scene(row.label.clone(), value_scene, palette)
}

fn kpi_tile_scene(
    title: String,
    value: String,
    note: String,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_grow: 1.0,
            flex_basis: px(200.0),
            min_width: px(140.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_12())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
             Text(title) TextRole(Role::Caption) --

                Node {
                    width: percent(100),
                    overflow: Overflow::clip_x(),
                }
                Children [  Text(value) TextRole(Role::Heading) TextLayout { linebreak: LineBreak::NoWrap }  ]
            --

                Node {
                    width: percent(100),
                    overflow: Overflow::clip_x(),
                }
                Children [  Text(note) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }  ]

        ]
    }
}

pub(crate) fn section_card_scene(
    title: String,
    rows: Vec<Box<dyn bevy::scene::Scene>>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_grow: 1.0,
            flex_basis: px(340.0),
            min_width: px(300.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_12())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
             Text(title) TextRole(Role::Body) --
            { rows }
        ]
    }
}

fn system_body_scene(
    hardware: Option<&HardwareInfo>,
    smbios: Option<&SmbiosMemorySnapshot>,
    npu: Option<&NpuInventorySnapshot>,
    sensors: Option<&SensorCenterSnapshot>,
    summary: &SystemSummaryModel,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let rows = system_fact_rows(hardware, smbios, npu);
    let thermal_rows = sensors.map(thermal_zone_rows).unwrap_or_default();
    if rows.is_empty() && thermal_rows.is_empty() {
        return Box::new(bsn! {
            Node {
                width: percent(100),
                padding: UiRect::all(Val::Px(space_12())),
                border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
            }
            BackgroundColor({ palette.panel_fill })
            Children [
                 Text({ t("common.waiting_inventory").to_owned() }) TextRole(Role::Body)
            ]
        }) as Box<dyn bevy::scene::Scene>;
    }

    let cpu_note = hardware
        .and_then(|h| match (h.cpu_brand.as_deref(), h.cpu_cores) {
            (Some(brand), Some(cores)) => Some(format!("{brand} · {cores} {}", t("common.cores"))),
            (Some(brand), None) => Some(brand.to_owned()),
            (None, Some(cores)) => Some(format!("{cores} {}", t("common.cores"))),
            (None, None) => None,
        })
        .unwrap_or_else(|| t("common.utilization").to_owned());

    let mem_total = hardware
        .and_then(|h| h.total_memory_mb)
        .map(clean_memory_size);
    let mem_note = match mem_total {
        Some(total) => format!("{total} {}", t("system.field.installed_memory")),
        None => t("system.field.installed_memory").to_owned(),
    };

    let proc_val = summary
        .processes
        .map_or_else(missing_value, |count| count.to_string());
    let proc_note = t("tab.apps").to_owned();

    let alerts_val = summary.active_alerts.to_string();
    let alerts_note = t("tab.alerts").to_owned();

    let health_tile = summary.health.as_ref().map(|health| {
        Box::new(kpi_tile_scene(
            t("system.health_score").to_owned(),
            health.clone(),
            t("system.health_score").to_owned(),
            palette,
        )) as Box<dyn bevy::scene::Scene>
    });

    let mut tiles = vec![
        Box::new(kpi_tile_scene(
            t("common.cpu").to_owned(),
            summary.cpu.clone(),
            cpu_note,
            palette,
        )) as Box<dyn bevy::scene::Scene>,
        Box::new(kpi_tile_scene(
            t("common.memory").to_owned(),
            summary.memory.clone(),
            mem_note,
            palette,
        )) as Box<dyn bevy::scene::Scene>,
        Box::new(kpi_tile_scene(
            t("dashboard.processes").to_owned(),
            proc_val,
            proc_note,
            palette,
        )) as Box<dyn bevy::scene::Scene>,
        Box::new(kpi_tile_scene(
            t("dashboard.active_alerts").to_owned(),
            alerts_val,
            alerts_note,
            palette,
        )) as Box<dyn bevy::scene::Scene>,
    ];
    if let Some(tile) = health_tile {
        tiles.push(tile);
    }

    let tiles_row = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(space_8()),
            row_gap: Val::Px(space_8()),
        }
        Children [
            { tiles }
        ]
    };

    let mut cards = [
        (SystemFactGroup::OperatingSystem, "common.operating_system"),
        (SystemFactGroup::Cpu, "system.field.cpu"),
        (SystemFactGroup::Memory, "common.memory"),
        (SystemFactGroup::Hardware, "common.hardware"),
        (SystemFactGroup::Npu, "npu.title"),
        (SystemFactGroup::MemoryInventory, "system.memory_inventory"),
    ]
    .into_iter()
    .filter_map(|(group, title)| {
        let group_rows = rows
            .iter()
            .filter(|row| row.group == group)
            .map(|row| Box::new(fact_row_scene(row, palette)) as Box<dyn Scene>)
            .collect::<Vec<_>>();
        (!group_rows.is_empty()).then(|| {
            let card = section_card_scene(t(title).to_owned(), group_rows, palette);
            if group == SystemFactGroup::MemoryInventory {
                Box::new(bsn! { MemoryInventoryAnchor @{card} }) as Box<dyn Scene>
            } else {
                Box::new(card) as Box<dyn Scene>
            }
        })
    })
    .collect::<Vec<_>>();

    if let Some(thermal_card) = thermal::thermal_zone_card_scene(&thermal_rows, palette) {
        cards.push(thermal_card);
    }

    let cards_grid = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(space_8()),
            row_gap: Val::Px(space_8()),
        }
        Children [
            { cards }
        ]
    };

    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_12()),
        }
        Children [
             @{ tiles_row } --
             @{ cards_grid }
        ]
    })
}

/// Content-region scene for the System page. The body container starts empty;
/// [`paint_system`] is its only author.
pub(crate) fn content(_context: &PageContext<'_>) -> impl Scene + use<> {
    let dashboard = dashboard::button(
        t("dashboard.title").to_owned(),
        dashboard::DashboardControl::Section(SystemPageSection::Dashboard),
        false,
        _context.palette,
    );
    let title = Page::System.title();
    let health = dashboard::button(
        t("health.system_health_alerts").into(),
        dashboard::DashboardControl::Section(SystemPageSection::Health),
        false,
        _context.palette,
    );
    let waiting = t("common.waiting_inventory").to_owned();
    let diagnostic = diagnostic_modal::diagnostic_button_scene(_context.palette);
    let about = crate::about_modal::action_scene(
        t("about.title"),
        crate::about_modal::AboutCommand::Open,
        _context.palette,
    );
    let system_information = crate::about_modal::action_scene(
        t("about.system_information"),
        crate::about_modal::AboutCommand::SystemInformation,
        _context.palette,
    );
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        SystemPageRoot
        Children [
             Text(title) TextRole(Role::Heading) --
             Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(space_8()), row_gap: px(space_8()) } SystemActions
             Children [ @{ dashboard } -- @{ health } -- @{ diagnostic } -- @{ about } -- @{ system_information } ] --

                Text(waiting)
                SystemStatusLine
                TextRole(Role::Caption)
            --
             Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column } SystemDashboardToolbar Children [] --

                Node {
                    width: percent(100),
                    min_height: px(0.0), flex_grow: 1.0, flex_basis: px(0.0),
                    overflow: Overflow::scroll_y(),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                    padding: UiRect::bottom(px(space_8())),
                }
                SystemBody ScrollArea

        ]
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/system.rs"]
mod tests;
