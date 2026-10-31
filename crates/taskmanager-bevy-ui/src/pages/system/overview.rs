//! The System overview body: the KPI tiles and the hardware fact cards.
//!
//! The overview is a measured grid. The root hands the body the content
//! width it measured, the grid resolves how many cards fit on one row from
//! that width and the rem-axis minimum card width, and the cards then fill
//! their row exactly. A small window therefore folds the hardware cards into
//! a narrower grid instead of letting them overflow the viewport.

use bevy::app::{App, PostUpdate};
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, ResMut};
use bevy::scene::{Scene, bsn};
use bevy::ui::UiSystems;
use bevy::ui::prelude::{
    BackgroundColor, BorderRadius, ComputedNode, FlexDirection, FlexWrap, Node, UiRect, Val,
    percent,
};
use bevy::ui::widget::Text;
use taskforest_core::core::hardware::HardwareInfo;
use taskforest_core::core::metrics::SmbiosMemorySnapshot;
use taskforest_core::core::npu::NpuInventorySnapshot;
use taskforest_core::core::sensors::SensorCenterSnapshot;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::missing_value;

use super::paint::SystemPaint;
use super::privacy::{SerialDisclosure, identity_fact_rows, module_serial_rows};
use super::thermal::thermal_zone_card_scene;
use super::thermal::{gpu_temperature_card_scene, sensor_temperature_projection};
use super::topology::{memory_topology, topology_card_scene};
use super::{
    MemoryInventoryAnchor, SystemBody, SystemFactGroup, SystemFactRow, SystemSummaryModel,
    clean_memory_size, fact_row_scene, kpi_tile_scene, section_card_scene, system_fact_rows,
};
use crate::palette::{UiPalette, space_8, space_12};
use crate::window::{Role, TextRole};

/// The minimum width of one hardware card, in rem of the live reading size.
const MIN_CARD_REM: f32 = 15.0;

/// The measured column budget of the overview's hardware-card grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OverviewGrid {
    /// How many cards fit on one row.
    pub(crate) columns: usize,
    /// The measured width of one card; `None` before the first measurement.
    pub(crate) card_width: Option<f32>,
}

impl OverviewGrid {
    /// Resolve the grid from the measured content width.
    ///
    /// The column count follows the measured width and the rem-axis minimum
    /// card width, capped by the number of cards: a row is never split by a
    /// column no card can occupy. The card width then fills the row exactly,
    /// so the grid leaves no ragged column behind.
    pub(crate) fn resolve(content_width: f32, gap: f32, min_card_width: f32, cards: usize) -> Self {
        if cards == 0 || content_width <= 0.0 || !content_width.is_finite() {
            return Self {
                columns: 1,
                card_width: None,
            };
        }
        let min_card_width = min_card_width.max(1.0);
        let fitting = ((content_width + gap) / (min_card_width + gap))
            .floor()
            .max(1.0) as usize;
        let columns = fitting.min(cards);
        let gaps = gap * columns.saturating_sub(1) as f32;
        let card_width = ((content_width - gaps) / columns as f32).max(0.0);
        Self {
            columns,
            card_width: Some(card_width),
        }
    }
}

/// The content width the overview last laid out against.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct OverviewLayout {
    /// The measured content width, once the body has a layout.
    pub(crate) content_width: Option<f32>,
}

/// The typed inputs the overview body renders.
pub(crate) struct OverviewInputs<'a> {
    /// The host inventory, when the platform supplied one.
    pub(crate) hardware: Option<&'a HardwareInfo>,
    /// The accepted SMBIOS memory inventory, when the lane answered.
    pub(crate) smbios: Option<&'a SmbiosMemorySnapshot>,
    /// The accepted NPU inventory, when the lane answered.
    pub(crate) npu: Option<&'a NpuInventorySnapshot>,
    /// The sensor-center snapshot, when the lane answered.
    pub(crate) sensors: Option<&'a SensorCenterSnapshot>,
    /// The KPI summary values.
    pub(crate) summary: &'a SystemSummaryModel,
    /// Whether the device serials are stated or masked.
    pub(crate) disclosure: SerialDisclosure,
    /// The measured content width of the body.
    pub(crate) content_width: Option<f32>,
}

/// One drafted hardware card: its group, its title and its rows.
struct CardDraft {
    /// The fact group the card belongs to; the memory inventory keeps the
    /// capture anchor that scrolls the page to it.
    group: Option<SystemFactGroup>,
    /// The card's title.
    title: String,
    /// The card's rows.
    rows: Vec<Box<dyn Scene>>,
}

/// Repaint the overview when the measured content width moved.
fn resize(
    mut layout: ResMut<OverviewLayout>,
    bodies: Query<&ComputedNode, With<SystemBody>>,
    mut paint: ResMut<SystemPaint>,
) {
    // The grid lays its cards out in the body's CONTENT box, not in its
    // border box: the body reserves a right lane for the floating rail, so
    // the border box is wider than the room the cards actually get. Resolving
    // the grid against the border box overshoots the row by exactly that
    // lane and pushes the last card onto a second row.
    let measured = bodies
        .iter()
        .next()
        .map(|node| node.content_box().width() * node.inverse_scale_factor())
        .filter(|width| *width > 0.0);
    let Some(width) = measured else {
        return;
    };
    if layout.content_width == Some(width) {
        return;
    }
    layout.content_width = Some(width);
    paint.dirty = true;
}

/// Register the overview's measured layout.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<OverviewLayout>()
        .add_systems(PostUpdate, resize.after(UiSystems::Layout));
}

/// The KPI tile row: the summary values the overview leads with.
fn tiles_row(
    summary: &SystemSummaryModel,
    hardware: Option<&HardwareInfo>,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let cpu_note = hardware
        .and_then(
            |hardware| match (hardware.cpu_brand.as_deref(), hardware.cpu_cores) {
                (Some(brand), Some(cores)) => {
                    Some(format!("{brand} · {cores} {}", t("common.cores")))
                }
                (Some(brand), None) => Some(brand.to_owned()),
                (None, Some(cores)) => Some(format!("{cores} {}", t("common.cores"))),
                (None, None) => None,
            },
        )
        .unwrap_or_else(|| t("common.utilization").to_owned());
    let mem_note = hardware
        .and_then(|hardware| hardware.total_memory_mb)
        .map(|total| {
            format!(
                "{} {}",
                clean_memory_size(total),
                t("system.field.installed_memory")
            )
        })
        .unwrap_or_else(|| t("system.field.installed_memory").to_owned());
    let proc_value = summary
        .processes
        .map_or_else(missing_value, |count| count.to_string());
    let mut tiles = vec![
        Box::new(kpi_tile_scene(
            t("common.cpu").to_owned(),
            summary.cpu.clone(),
            cpu_note,
            palette,
        )) as Box<dyn Scene>,
        Box::new(kpi_tile_scene(
            t("common.memory").to_owned(),
            summary.memory.clone(),
            mem_note,
            palette,
        )) as Box<dyn Scene>,
        Box::new(kpi_tile_scene(
            t("dashboard.processes").to_owned(),
            proc_value,
            t("tab.apps").to_owned(),
            palette,
        )) as Box<dyn Scene>,
        Box::new(kpi_tile_scene(
            t("dashboard.active_alerts").to_owned(),
            summary.active_alerts.to_string(),
            t("tab.alerts").to_owned(),
            palette,
        )) as Box<dyn Scene>,
    ];
    if let Some(health) = summary.health.as_ref() {
        tiles.push(Box::new(kpi_tile_scene(
            t("system.health_score").to_owned(),
            health.clone(),
            t("system.health_score").to_owned(),
            palette,
        )) as Box<dyn Scene>);
    }
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(space_8()),
            row_gap: Val::Px(space_8()),
        }
        Children [ { tiles } ]
    })
}

/// The body of the System page: the KPI tiles and the hardware fact cards.
pub(crate) fn system_body_scene(
    inputs: &OverviewInputs<'_>,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let mut rows = system_fact_rows(inputs.hardware, inputs.smbios, inputs.npu);
    if let Some(detail) = inputs.summary.health_detail.as_ref() {
        rows.push(SystemFactRow {
            group: SystemFactGroup::Health,
            label: t("system.health_score").to_owned(),
            value: detail.clone(),
        });
    }
    rows.extend(identity_fact_rows(inputs.smbios, inputs.disclosure));
    rows.extend(module_serial_rows(inputs.smbios, inputs.disclosure));
    let projection = inputs.sensors.map(sensor_temperature_projection);
    let topology = inputs.smbios.map(memory_topology);
    let zones_present = projection
        .as_ref()
        .is_some_and(|projection| !projection.zones.rows.is_empty());
    let gpu_present = projection
        .as_ref()
        .is_some_and(|projection| !projection.gpu.rows.is_empty());
    let topology_present = topology
        .as_ref()
        .is_some_and(|topology| !topology.slots.is_empty());
    if rows.is_empty() && !zones_present && !gpu_present && !topology_present {
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
        });
    }

    let drafts: Vec<CardDraft> = [
        (SystemFactGroup::OperatingSystem, "common.operating_system"),
        (SystemFactGroup::Cpu, "system.field.cpu"),
        (SystemFactGroup::Memory, "common.memory"),
        (SystemFactGroup::Hardware, "common.hardware"),
        (SystemFactGroup::Npu, "npu.title"),
        (SystemFactGroup::MemoryInventory, "system.memory_inventory"),
        (SystemFactGroup::Health, "system.health_score"),
    ]
    .into_iter()
    .filter_map(|(group, title)| {
        let group_rows: Vec<Box<dyn Scene>> = rows
            .iter()
            .filter(|row| row.group == group)
            .map(|row| Box::new(fact_row_scene(row, palette)) as Box<dyn Scene>)
            .collect();
        (!group_rows.is_empty()).then(|| CardDraft {
            group: Some(group),
            title: t(title).to_owned(),
            rows: group_rows,
        })
    })
    .collect();

    let card_count = drafts.len()
        + usize::from(zones_present)
        + usize::from(gpu_present)
        + usize::from(topology_present);
    let grid = OverviewGrid::resolve(
        inputs.content_width.unwrap_or(0.0),
        space_8(),
        palette.rem_px * MIN_CARD_REM,
        card_count,
    );

    let mut cards: Vec<Box<dyn Scene>> = Vec::new();
    for draft in drafts {
        let card = Box::new(section_card_scene(
            draft.title,
            draft.rows,
            grid.card_width,
            palette,
        )) as Box<dyn Scene>;
        if draft.group == Some(SystemFactGroup::MemoryInventory) {
            // The capture anchor must sit on an entity that HAS a layout node:
            // the capture marker resolves the inventory through
            // `&ComputedNode + With<MemoryInventoryAnchor>`, and a bare
            // marker entity (no `Node`) never matches — the capture could not
            // scroll to the inventory at all.
            cards.push(Box::new(bsn! {
                Node { width: percent(100) }
                MemoryInventoryAnchor
                Children [
                     @{ card }
                ]
            }));
        } else {
            cards.push(card);
        }
    }
    if let Some(projection) = projection.as_ref()
        && zones_present
        && let Some(card) = thermal_zone_card_scene(&projection.zones, grid.card_width, palette)
    {
        cards.push(card);
    }
    if let Some(projection) = projection.as_ref()
        && gpu_present
        && let Some(card) = gpu_temperature_card_scene(&projection.gpu, grid.card_width, palette)
    {
        cards.push(card);
    }
    if let Some(topology) = topology.as_ref()
        && topology_present
        && let Some(card) = topology_card_scene(topology, grid.card_width, palette)
    {
        cards.push(card);
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
             @{ tiles_row(inputs.summary, inputs.hardware, palette) } --
             @{ cards_grid }
        ]
    })
}
