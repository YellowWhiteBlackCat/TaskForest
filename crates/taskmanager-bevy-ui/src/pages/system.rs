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
use bevy::ecs::resource::Resource;
use bevy::ecs::system::Commands;
use bevy::scene::template_value;
use bevy::scene::{Scene, WorldSceneExt, bsn};
use bevy::ui::prelude::{
    BackgroundColor, BorderRadius, FlexDirection, FlexWrap, Node, Overflow, UiRect, Val, percent,
    px,
};
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_shell::SystemProjectionStore;
use taskmanager_shell::presentation::{bytes, missing_value};

use crate::app::{FrontendTrack, Page, PageContext};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, no_wrap_text, space_2, space_4, space_8, space_12};
use crate::window::{Role, TextRole, WindowPalette};
use taskmanager_application::SmbiosMemoryState;
use taskmanager_core::core::metrics::SmbiosMemorySnapshot;
use taskmanager_core::core::npu::NpuEngineKind;
use taskmanager_core::core::npu::NpuInventorySnapshot;
use taskmanager_shell::presentation::health_score_for_snapshot;
use taskmanager_shell::presentation::kernel_error_summary;
use taskmanager_shell::presentation::smbios_memory_inventory_rows;

/// The page's single body container. Painted exclusively by
/// [`paint_system`], which the root's on-insert hook binds.
#[derive(Component, Clone, Default)]
pub(crate) struct SystemBody;

/// The one status line the paint pass rewrites (waiting/fact counts).
#[derive(Component, Clone, Default)]
pub(crate) struct SystemStatusLine;

/// The page's root: mounting it binds the paint observers.
#[derive(Component, Clone, Default)]
#[component(on_insert = bind_system_page)]
pub(crate) struct SystemPageRoot;

#[derive(Resource, Default)]
struct SystemPageBound;

/// Bind the paint observers to the app, once per world. The body's own Add
/// is the first-paint trigger (the root's insert fires before the body
/// exists); the fold observer is the only later refresh.
fn bind_system_page(
    mut world: bevy::ecs::world::DeferredWorld<'_>,
    _context: bevy::ecs::lifecycle::HookContext,
) {
    if world.get_resource::<SystemPageBound>().is_some() {
        return;
    }
    let mut commands = world.commands();
    commands.insert_resource(SystemPageBound);
    commands.add_observer(on_body_added);
    commands.add_observer(on_projection_folded);
}

/// First paint: the body container just landed.
fn on_body_added(_added: On<Add, SystemBody>, mut commands: Commands) {
    commands.queue(paint_system);
}

/// The drain fold is the page's only later data-refresh trigger.
fn on_projection_folded(_fold: On<ShellProjectionFolded>, mut commands: Commands) {
    commands.queue(paint_system);
}

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

/// One label→value fact row. The value is already final display text.
pub(crate) struct SystemFactRow {
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
    let mut rows: Vec<SystemFactRow> = Vec::new();
    let dash = missing_value();
    let Some(hardware) = hardware else {
        return rows;
    };
    rows.push(SystemFactRow {
        label: t("system.hostname").to_owned(),
        value: optional(hardware.hostname.as_deref()),
    });
    rows.push(SystemFactRow {
        label: t("system.os").to_owned(),
        value: joined(hardware.os_name.as_deref(), hardware.os_version.as_deref()),
    });
    rows.push(SystemFactRow {
        label: t("system.kernel").to_owned(),
        value: joined(
            hardware.kernel_version.as_deref(),
            hardware.kernel_build.as_deref(),
        ),
    });
    if let Some(errors) = kernel_error_summary(hardware) {
        rows.push(SystemFactRow {
            label: t("system.kernel_errors").to_owned(),
            value: errors,
        });
    }
    if let Some(count) = hardware.kernel_modules_count {
        rows.push(SystemFactRow {
            label: t("system.kernel_modules").to_owned(),
            value: count.to_string(),
        });
    }
    rows.push(SystemFactRow {
        label: t("system.model").to_owned(),
        value: joined(
            hardware.product_name.as_deref(),
            hardware.product_version.as_deref(),
        ),
    });
    rows.push(SystemFactRow {
        label: t("system.firmware").to_owned(),
        value: optional(hardware.firmware_vendor.as_deref()),
    });
    rows.push(SystemFactRow {
        label: t("system.field.cpu").to_owned(),
        value: optional(hardware.cpu_brand.as_deref()),
    });
    let cores = hardware
        .cpu_cores
        .map_or_else(|| dash.clone(), |cores| cores.to_string());
    rows.push(SystemFactRow {
        label: t("system.field.cores").to_owned(),
        value: cores,
    });
    if let Some(memory) = hardware.total_memory_mb {
        rows.push(SystemFactRow {
            label: t("system.section.memory").to_owned(),
            value: clean_memory_size(memory),
        });
    }
    if let Some(virt) = hardware.virt.as_deref() {
        rows.push(SystemFactRow {
            label: t("system.field.virt").to_owned(),
            value: virt.to_owned(),
        });
    }
    rows.push(SystemFactRow {
        label: t("system.desktop_environment").to_owned(),
        value: joined(
            hardware.desktop_environment.as_deref(),
            hardware.desktop_environment_version.as_deref(),
        ),
    });
    rows.push(SystemFactRow {
        label: t("system.windowing_system").to_owned(),
        value: joined(
            hardware.windowing_system.as_deref(),
            hardware.window_manager.as_deref(),
        ),
    });
    rows.push(SystemFactRow {
        label: t("system.field.init_system").to_owned(),
        value: optional(hardware.init_system.as_deref()),
    });
    if let Some(manager) = hardware.package_manager.as_deref() {
        let value = hardware.package_manager_version.as_deref().map_or_else(
            || manager.to_owned(),
            |version| format!("{manager} {version}"),
        );
        rows.push(SystemFactRow {
            label: t("system.package_manager").to_owned(),
            value,
        });
    }
    rows.push(SystemFactRow {
        label: t("system.field.shell").to_owned(),
        value: optional(hardware.shell.as_deref()),
    });
    rows.push(SystemFactRow {
        label: t("system.field.locale").to_owned(),
        value: optional(hardware.locale.as_deref()),
    });
    if let Some(smbios) = smbios {
        for (label, value) in smbios_memory_inventory_rows(smbios) {
            rows.push(SystemFactRow { label, value });
        }
    }
    if let Some(npu) = npu.filter(|inv| inv.is_success()) {
        for dev in &npu.devices {
            rows.push(SystemFactRow {
                label: format!("{} {}", t("npu.title"), dev.device_id.as_str()),
                value: dev
                    .brand
                    .clone()
                    .unwrap_or_else(|| t("npu.device_title").to_owned()),
            });
            rows.push(SystemFactRow {
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
) -> String {
    if hardware.is_none() && smbios.is_none() && npu.is_none() {
        t("common.waiting_inventory").to_owned()
    } else {
        let count = system_fact_rows(hardware, smbios, npu).len();
        t("system.facts_ready").replacen("{count}", &count.to_string(), 1)
    }
}

// ---- render adapters ------------------------------------------------------

fn fact_row_scene(row: &SystemFactRow, palette: &UiPalette) -> impl Scene + use<> {
    // Delegates to the shared bounded key/value row: same single-line
    // contract (NoWrap + clip) as the performance rail — one row grammar
    // across pages, never a page-local spelling.
    let value = row.value.clone();
    let value_scene = Box::new(bsn! {
        Text(value)
        TextRole(Role::Body)
        template_value(no_wrap_text())
    }) as Box<dyn bevy::scene::Scene>;
    crate::widgets::controls::stat_row_scene(row.label.clone(), value_scene, palette)
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
            ( Text(title) TextRole(Role::Caption) ),
            (
                Node {
                    width: percent(100),
                    overflow: Overflow::clip_x(),
                }
                Children [ ( Text(value) TextRole(Role::Heading) template_value(no_wrap_text()) ) ]
            ),
            (
                Node {
                    width: percent(100),
                    overflow: Overflow::clip_x(),
                }
                Children [ ( Text(note) TextRole(Role::Caption) template_value(no_wrap_text()) ) ]
            ),
        ]
    }
}

fn section_card_scene(
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
            ( Text(title) TextRole(Role::Body) ),
            { rows },
        ]
    }
}

fn system_body_scene(
    hardware: Option<&HardwareInfo>,
    smbios: Option<&SmbiosMemorySnapshot>,
    npu: Option<&NpuInventorySnapshot>,
    summary: &SystemSummaryModel,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let rows = system_fact_rows(hardware, smbios, npu);
    if rows.is_empty() {
        return Box::new(bsn! {
            Node {
                width: percent(100),
                padding: UiRect::all(Val::Px(space_12())),
                border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
            }
            BackgroundColor({ palette.panel_fill })
            Children [
                ( Text({ t("common.waiting_inventory").to_owned() }) TextRole(Role::Body) ),
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
            { tiles },
        ]
    };

    let mut os_rows: Vec<Box<dyn bevy::scene::Scene>> = Vec::new();
    let mut cpu_rows: Vec<Box<dyn bevy::scene::Scene>> = Vec::new();
    let mut mem_rows: Vec<Box<dyn bevy::scene::Scene>> = Vec::new();
    let mut hw_rows: Vec<Box<dyn bevy::scene::Scene>> = Vec::new();

    for (index, row) in rows.iter().enumerate() {
        let r = Box::new(fact_row_scene(row, palette)) as Box<dyn bevy::scene::Scene>;
        match index % 4 {
            0 => os_rows.push(r),
            1 => cpu_rows.push(r),
            2 => mem_rows.push(r),
            _ => hw_rows.push(r),
        }
    }

    let cards = vec![
        Box::new(section_card_scene(
            t("common.operating_system").to_owned(),
            os_rows,
            palette,
        )) as Box<dyn bevy::scene::Scene>,
        Box::new(section_card_scene(
            t("system.field.cpu").to_owned(),
            cpu_rows,
            palette,
        )) as Box<dyn bevy::scene::Scene>,
        Box::new(section_card_scene(
            t("common.memory").to_owned(),
            mem_rows,
            palette,
        )) as Box<dyn bevy::scene::Scene>,
        Box::new(section_card_scene(
            t("common.hardware").to_owned(),
            hw_rows,
            palette,
        )) as Box<dyn bevy::scene::Scene>,
    ];

    let cards_grid = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(space_8()),
            row_gap: Val::Px(space_8()),
        }
        Children [
            { cards },
        ]
    };

    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_12()),
        }
        Children [
            ( { tiles_row } ),
            ( { cards_grid } ),
        ]
    })
}

/// Content-region scene for the System page. The body container starts empty;
/// [`paint_system`] is its only author.
pub(crate) fn content(_context: &PageContext<'_>) -> impl Scene + use<> {
    let title = Page::System.title();
    let waiting = t("common.waiting_inventory").to_owned();
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
            ( Text(title) TextRole(Role::Heading) ),
            (
                Text(waiting)
                SystemStatusLine
                TextRole(Role::Caption)
            ),
            (
                Node {
                    width: percent(100),
                    height: Val::Auto,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                SystemBody
            ),
        ]
    }
}

// ---- the single body author ------------------------------------------------

pub(crate) fn paint_system(world: &mut bevy::ecs::world::World) {
    let palette = world.resource::<WindowPalette>().inner.clone();
    let (hardware, smbios, npu, summary, status) = {
        let shell = &world.non_send::<FrontendTrack>().shell;
        let smbios = match shell.smbios_memory_state() {
            SmbiosMemoryState::Ready(ready) => Some(ready.snapshot.clone()),
            _ => None,
        };
        let projection = shell.projection();
        let npu = projection.npu_inventory.clone();
        let hardware = projection.hardware.clone();
        let summary = system_summary_model(projection);
        let status = status_line_text(hardware.as_ref(), smbios.as_ref(), npu.as_ref());
        (hardware, smbios, npu, summary, status)
    };
    let scene = system_body_scene(
        hardware.as_ref(),
        smbios.as_ref(),
        npu.as_ref(),
        &summary,
        &palette,
    );
    let mut body_query = world.query_filtered::<bevy::ecs::entity::Entity, With<SystemBody>>();
    let Some(body) = body_query.iter(world).next() else {
        return;
    };
    let stale: Vec<bevy::ecs::entity::Entity> = world
        .get::<bevy::ecs::hierarchy::Children>(body)
        .map(|children| children.iter().copied().collect())
        .unwrap_or_default();
    // Synchronous World mutation, not queued commands: a same-frame second
    // paint must observe the previous paint's result or it would double the
    // body (the same lesson the History page learned).
    for entity in stale {
        let _ = world.despawn(entity);
    }
    let fresh = match world.spawn_scene(scene) {
        Ok(entity) => entity.id(),
        Err(_) => return,
    };
    world
        .entity_mut(body)
        .add_one_related::<bevy::ecs::hierarchy::ChildOf>(fresh);
    let mut lines = world.query_filtered::<&mut Text, With<SystemStatusLine>>();
    if let Ok(mut line) = lines.single_mut(world) {
        line.0 = status;
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/system.rs"]
mod tests;
