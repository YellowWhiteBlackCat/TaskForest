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
    px, rem,
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

pub(crate) mod overview;
pub(crate) mod privacy;
pub(crate) mod topology;

use crate::app::{FrontendTrack, Page, PageContext};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, space_2, space_4, space_8, space_12};
use crate::text_selection::ReadoutInput;
use crate::widgets::controls::detail_row_scene;
use crate::widgets::layout::SystemDashboardBudget;
use crate::window::{Role, TextRole, WindowPalette};
use bevy::text::{LineBreak, TextColor, TextFont, TextLayout, TextReadWriteMode};
use bevy::ui::ComputedNode;
use bevy::ui_widgets::{ScrollArea, TextInput};
use dashboard::{SystemDashboardState, SystemDashboardToolbar};
use privacy::{SerialDisclosure, identity_fact_rows, module_serial_rows};
use taskmanager_application::SmbiosMemoryState;
use taskmanager_application::system_timeline::SystemPageSection;
use taskmanager_core::core::metrics::SmbiosMemorySnapshot;
use taskmanager_core::core::npu::NpuEngineKind;
use taskmanager_core::core::npu::NpuInventorySnapshot;
use taskmanager_shell::presentation::health_score_for_snapshot;
use taskmanager_shell::presentation::health_score_summary;
use taskmanager_shell::presentation::smbios_memory_inventory_rows;

/// The KPI tile's flex basis, in rem: the width one tile asks for before the
/// row wraps the next one beside it.
const KPI_TILE_FLEX_BASIS_REM: f32 = 12.5;

/// The KPI tile's minimum width, in rem: the narrowest a tile may take
/// before its own value would clip.
const KPI_TILE_MIN_WIDTH_REM: f32 = 8.75;

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

/// The marker on one hardware fact card of the overview grid.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SystemFactCard;

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
    /// The full score + deduction bill for the dense System facts card. The
    /// KPI tile keeps the compact score; this row is the auditable surface
    /// required by `pressure.memory-thrashing-health`.
    pub(crate) health_detail: Option<String>,
}

/// Fold the shell projection into the System page KPI summary values (pure).
/// Matches Iced `DashboardSummaryModel` and GPUI `summary_card` readouts.
pub(crate) fn system_summary_model(projection: &SystemProjectionStore) -> SystemSummaryModel {
    let snapshot = projection.snapshot.as_ref();
    let health = snapshot.and_then(health_score_for_snapshot);
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
        active_alerts: projection.alert_center.active_alerts().len(),
        // KPI tiles own their label separately; keep the value slot to the
        // compact score so the full explanatory health sentence cannot be
        // clipped inside a narrow fifth card. The full deduction bill is
        // rendered by the dense System facts card below.
        health: health.as_ref().map(|score| format!("{}/100", score.score)),
        health_detail: health.as_ref().map(health_score_summary),
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
    Health,
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
    disclosure: SerialDisclosure,
) -> String {
    if hardware.is_none() && smbios.is_none() && npu.is_none() && sensors.is_none() {
        return t("common.waiting_inventory").to_owned();
    }
    let base_count = system_fact_rows(hardware, smbios, npu).len()
        + identity_fact_rows(smbios, disclosure).len()
        + module_serial_rows(smbios, disclosure).len();
    let thermal_count = sensors.map(|s| thermal_zone_rows(s).len()).unwrap_or(0);
    let count = base_count + thermal_count;
    t("system.facts_ready").replacen("{count}", &count.to_string(), 1)
}

// ---- render adapters ------------------------------------------------------

pub(crate) fn fact_row_scene(row: &SystemFactRow, palette: &UiPalette) -> impl Scene + use<> {
    // Inspection facts keep complete values in the owned wrapped row grammar.
    // The value is a READ-ONLY native text input: pointer presses/drags,
    // double-click words, triple-click select-all, Ctrl/Cmd+A, and the
    // clipboard chords are the official widget's behavior, over the same
    // wrapped layout the plain readout used.
    let value = row.value.clone();
    let value_scene = Box::new(bsn! {
        TextInput
        TextReadWriteMode::ReadOnly
        TextFont::default()
        TextColor::default()
        TextRole(Role::Body)
        TextLayout { linebreak: LineBreak::WordBoundary }
        Node { width: percent(100), min_width: px(0.0) }
        ReadoutInput({ value })
    }) as Box<dyn bevy::scene::Scene>;
    detail_row_scene(row.label.clone(), value_scene, palette)
}

pub(crate) fn kpi_tile_scene(
    title: String,
    value: String,
    note: String,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_grow: 1.0,
            flex_basis: rem(KPI_TILE_FLEX_BASIS_REM),
            min_width: rem(KPI_TILE_MIN_WIDTH_REM),
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

/// One fact card.
///
/// The card's width is the measured grid width: the overview resolves how
/// many cards fit on one row from the content width it measured, so a small
/// window folds the cards instead of letting them overflow.
pub(crate) fn section_card_scene(
    title: String,
    rows: Vec<Box<dyn bevy::scene::Scene>>,
    width: Option<f32>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let basis = width.map_or(Val::Auto, px);
    bsn! {
        Node {
            flex_grow: 1.0,
            flex_basis: { basis },
            min_width: px(0.0),
            max_width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_12())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        SystemFactCard
        Children [
             Text(title) TextRole(Role::Body) --
            { rows }
        ]
    }
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
    let mask = privacy::mask_button_scene(SerialDisclosure::default(), _context.palette);
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
    let rail = crate::widgets::scrollbar::scrollbar_rail_scene(_context.palette);
    let lane = crate::widgets::scrollbar::rail_lane(UiRect::bottom(px(space_8())));
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
             Children [ @{ dashboard } -- @{ health } -- @{ diagnostic } -- @{ mask } -- @{ about } -- @{ system_information } ] --

                Text(waiting)
                SystemStatusLine
                TextRole(Role::Caption)
            --
             Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column } SystemDashboardToolbar Children [] --

                // The viewport wrapper owns the scroll area AND the floating
                // rail: the rail is a sibling of the scroll area, so it never
                // scrolls with the content and never takes layout space away
                // from it.
                Node {
                    width: percent(100),
                    min_height: px(0.0), flex_grow: 1.0, flex_basis: px(0.0),
                    flex_direction: FlexDirection::Column,
                }
                Children [
                    Node {
                        width: percent(100),
                        min_height: px(0.0), flex_grow: 1.0, flex_basis: px(0.0),
                        overflow: Overflow::scroll_y(),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(space_2()),
                        // The rail floats over the trailing edge: the content
                        // keeps that lane clear, so no row's right edge ever
                        // sits under the scrollbar.
                        padding: { lane },
                    }
                    SystemBody ScrollArea
                    --
                    @{ rail }
                ]

        ]
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/system.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/system_parity.rs"]
mod parity_tests;
