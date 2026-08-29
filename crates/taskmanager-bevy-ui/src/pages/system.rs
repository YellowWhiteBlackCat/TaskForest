//! The System page: one typed key/value projection of the host identity,
//! firmware, CPU, memory, and session facts the shell already owns.
//!
//! **Composition model** (the page-proxy contract in [`crate::pages`]): the
//! static tree is the title + status line + the EMPTY body container; the
//! body's only author is [`paint_system`], bound by the root's on-insert
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
use bevy::ui::prelude::{BackgroundColor, BorderRadius, FlexDirection, Node, UiRect, Val, percent};
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_shell::presentation::missing_value;

use crate::app::{FrontendTrack, Page, PageContext};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, no_wrap_text, space_2, space_8};
use crate::window::{Role, TextRole, WindowPalette};

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
pub(crate) fn system_fact_rows(hardware: Option<&HardwareInfo>) -> Vec<SystemFactRow> {
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
            value: format!("{memory} MiB"),
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
    rows
}

/// The status line: an inventory that has not arrived states that; one that
/// has states its fact count — never a fabricated zero.
fn status_line_text(hardware: Option<&HardwareInfo>) -> String {
    match hardware {
        None => t("common.waiting_inventory").to_owned(),
        Some(_) => {
            let count = system_fact_rows(Some(hardware.expect("checked"))).len();
            t("system.facts_ready").replacen("{count}", &count.to_string(), 1)
        }
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

fn system_body_scene(hardware: Option<&HardwareInfo>, palette: &UiPalette) -> impl Scene + use<> {
    let rows = system_fact_rows(hardware);
    let children: Vec<Box<dyn bevy::scene::Scene>> = if rows.is_empty() {
        vec![Box::new(bsn! {
            Node { width: percent(100) }
            Children [
                ( Text({ t("common.waiting_inventory").to_owned() }) TextRole(Role::Body) ),
            ]
        })]
    } else {
        rows.iter()
            .map(|row| Box::new(fact_row_scene(row, palette)) as Box<dyn bevy::scene::Scene>)
            .collect()
    };
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        // Deliberately NOT marked SystemBody: that marker belongs to the
        // container alone, and a second carrier here would re-trigger the
        // body-added paint observer forever.
        BackgroundColor({ palette.panel_fill })
        Children [
            { children },
        ]
    }
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
    let (hardware, status) = {
        let shell = &world.non_send::<FrontendTrack>().shell;
        (
            shell.projection().hardware.clone(),
            status_line_text(shell.projection().hardware.as_ref()),
        )
    };
    let scene = system_body_scene(hardware.as_ref(), &palette);
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
    let fresh = world
        .spawn_scene(scene)
        .expect("the system body resolves without assets")
        .id();
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
