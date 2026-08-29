//! Startup page — inventory table over the shared startup sort projection.
//!
//! Same architecture as [`crate::pages::services`] (the template page); the
//! differences are the data family and its semantics:
//!
//! - rows project through [`ShellApp::sorted_startup_entries`] and a visual
//!   row resolves back to its target ONLY through
//!   [`ShellApp::sorted_startup_entry_at`] (the shell's single "row N →
//!   target" translation — bypassing it is the wrong-row defect class);
//! - the state column renders the enabled/disabled chip (TUI parity:
//!   `Enabled`/`Disabled` via the shared catalog);
//! - the source column reads `source · scope` and the impact column carries
//!   its evidence (`Low · 42 ms` measured, `Low · unmeasured` otherwise —
//!   never a fabricated duration);
//! - the boot-evidence projection renders as one honest caption line: the
//!   typed unavailable marker when the provider could not instrument the
//!   boot, a chain/failed-units summary when it could, nothing before the
//!   first observation.
//!
//! 对接点 (W4 menu/dialog): the enable/disable verbs read the current target
//! from [`StartupSelection`] / [`StartupTargetSelected`]; the pointer and key
//! adapters fire [`StartupSortClicked`], [`StartupRowClicked`] and
//! [`StartupSelectionMoved`]. Colors are palette roles only — the chip seam
//! note in [`crate::pages::services`] applies here too.

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::{Add, HookContext};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Res, ResMut};
use bevy::ecs::world::{DeferredWorld, World};
use bevy::scene::{CommandsSceneExt, Scene, bsn, template_value};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;
use taskmanager_application::{SourceNotice, source_notice};
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::core::startup::{
    StartupEntry, StartupEntryId, StartupImpactEvidence, StartupScope,
};

use taskmanager_shell::presentation::control_error_detail;
use taskmanager_shell::{InfoSortCol, InfoTable, ShellApp, SortDir};

use crate::app::{FrontendTrack, Page, PageContext, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, no_wrap_text, space_2, space_4, space_8, space_24};
use crate::widgets::controls::sort_indicator_scene;
use crate::window::{Role, TextRole, WindowPalette};

pub(crate) mod menu;

// The inventory action-menu tests cover both the Startup and Sessions
// contexts; they use full crate paths, so the mount point is arbitrary.
#[cfg(test)]
#[path = "../../tests/headless/pages/inventory_menus.rs"]
mod inventory_menu_tests;

// ---- pure core: row view model, chips, copy ----

/// One Startup-table row: display material plus the opaque target id.
pub(crate) struct StartupRowModel {
    pub(crate) target: StartupEntryId,
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) source: String,
    pub(crate) impact: String,
    pub(crate) exec: String,
}

/// Project the table's whole material through the shared sort order.
pub(crate) fn startup_rows(shell: &ShellApp) -> Vec<StartupRowModel> {
    shell
        .sorted_startup_entries()
        .into_iter()
        .map(|entry| StartupRowModel {
            target: entry.id.clone(),
            name: entry.name.clone(),
            enabled: entry.enabled,
            source: startup_source_text(entry),
            impact: startup_impact_text(entry),
            exec: entry.exec.clone(),
        })
        .collect()
}

/// Source column with its scope suffix (GPUI/TUI parity: the row reads
/// `User Service · User` instead of the bare provider label).
pub(crate) fn startup_source_text(entry: &StartupEntry) -> String {
    format!(
        "{} · {}",
        entry.source.as_str(),
        startup_scope_text(entry.scope)
    )
}

fn startup_scope_text(scope: StartupScope) -> &'static str {
    match scope {
        StartupScope::User => t("startup.scope_user"),
        StartupScope::System => t("startup.scope_system"),
        StartupScope::Session => t("startup.scope_session"),
        StartupScope::Unknown => t("startup.scope_unknown"),
    }
}

/// Impact column with its evidence: a measured boot impact carries its
/// duration, an unmeasured one says so — never a fabricated number.
pub(crate) fn startup_impact_text(entry: &StartupEntry) -> String {
    match entry.impact_evidence {
        StartupImpactEvidence::Measured { duration_ms } => {
            format!("{} · {duration_ms} ms", t(entry.impact.i18n_key()))
        }
        StartupImpactEvidence::Unknown { .. } => {
            format!(
                "{} · {}",
                t(entry.impact.i18n_key()),
                t("startup.impact_unmeasured")
            )
        }
    }
}

/// Enabled-state chip: enabled is the positive attention state, disabled the
/// idle one (the TUI renders plain text; the chip is this frontend's scannable
/// equivalent, colored through palette roles only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnabledChip {
    Positive,
    Idle,
}

pub(crate) fn enabled_chip(enabled: bool) -> EnabledChip {
    if enabled {
        EnabledChip::Positive
    } else {
        EnabledChip::Idle
    }
}

/// Token-tinted fill: the token's own channels at a reduced alpha, so the
/// palette stays the only color source.
fn tinted(base: Color, alpha: f32) -> Color {
    let srgba = base.to_srgba();
    Color::srgba(srgba.red, srgba.green, srgba.blue, srgba.alpha * alpha)
}

/// Seam: re-target to the palette's success/danger tokens when they exist.
pub(crate) fn chip_fill(chip: EnabledChip, palette: &UiPalette) -> Color {
    match chip {
        EnabledChip::Positive => tinted(palette.accent, 0.22),
        EnabledChip::Idle => tinted(palette.dim_color, 0.18),
    }
}

/// Degraded-source headline (TUI source-panel parity, minus its keybinding
/// hint — this frontend has no refresh chord yet).
pub(crate) fn source_notice_text(sources: Option<&[SourceStatus]>) -> Option<String> {
    let notice = source_notice(sources?)?;
    let title = match notice {
        SourceNotice::Partial(_) => t("source.partial_title"),
        SourceNotice::Unavailable(_) => t("source.unavailable_title"),
    };
    let detail = control_error_detail(notice.failure());
    Some(format!("{title}: {detail}"))
}

/// Empty-state copy: a typed provider failure explains itself instead of
/// reading as a confirmed "no startup entries".
pub(crate) fn empty_state_text(sources: Option<&[SourceStatus]>) -> String {
    source_notice_text(sources).unwrap_or_else(|| t("empty.no_startup_reported").to_owned())
}

/// Boot-evidence caption: the typed unavailable marker when the provider
/// could not instrument this boot, a chain/failed-units summary when it
/// could, `None` before the first observation (silence is honest there).
pub(crate) fn evidence_line(shell: &ShellApp) -> Option<String> {
    let projection = shell.projection();
    if projection.startup_evidence_unavailable.is_some() {
        return Some(t("startup.evidence_unavailable").to_owned());
    }
    let evidence = projection.startup_boot_evidence.as_ref()?;
    let chain = evidence.critical_chain.len();
    let failed = evidence.failed_units.len();
    (chain > 0 || failed > 0).then(|| {
        format!(
            "{}: {} {}, {} {}",
            t("startup.timeline"),
            chain,
            t("startup.critical_chain"),
            failed,
            t("startup.failed_units"),
        )
    })
}

/// The summary line under the title: honest count plus the active sort.
pub(crate) fn status_line_text(shell: &ShellApp, rows: usize) -> String {
    let noun = t("startup.noun");
    match shell.startup_sort {
        Some((column, direction)) => {
            format!(
                "{rows} {noun} · {} {}",
                t(column.label()),
                direction.label()
            )
        }
        None => format!("{rows} {noun} · provider order"),
    }
}

// ---- pure core: id-keyed selection model ----

/// The page's selection state: the opaque target id, never a row index.
/// 对接点 (W4 menu/dialog): enable/disable verbs read the id from here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Resource)]
pub(crate) struct StartupSelection {
    pub(crate) target: Option<StartupEntryId>,
}

pub(crate) fn selected_row(
    rows: &[StartupRowModel],
    selection: &StartupSelection,
) -> Option<usize> {
    let target = selection.target.as_ref()?;
    rows.iter().position(|row| &row.target == target)
}

/// Clamp-move a row cursor: saturates at the first/last row; an empty table
/// stays unselected; a move from "nothing selected" enters at the first row.
pub(crate) fn moved_row(rows_len: usize, current: Option<usize>, delta: isize) -> Option<usize> {
    if rows_len == 0 {
        return None;
    }
    let max = (rows_len - 1) as isize;
    let moved = current.map_or(0, |row| row as isize + delta);
    Some(moved.clamp(0, max) as usize)
}

// ---- page-owned column vocabulary ----

struct Column {
    sort: Option<InfoSortCol>,
    label: String,
    width_px: f32,
}

fn columns() -> Vec<Column> {
    vec![
        Column {
            sort: Some(InfoSortCol::Name),
            label: t("common.name").to_owned(),
            width_px: 220.0,
        },
        Column {
            sort: Some(InfoSortCol::Status),
            label: t("common.state").to_owned(),
            width_px: 120.0,
        },
        Column {
            sort: None,
            label: t("startup.source").to_owned(),
            width_px: 200.0,
        },
        Column {
            sort: None,
            label: t("startup.impact").to_owned(),
            width_px: 170.0,
        },
        Column {
            sort: None,
            label: t("startup.command").to_owned(),
            width_px: 320.0,
        },
    ]
}

/// Header cell text with the active-sort arrow.
/// Header cell's pure label: the column word only. Sort direction renders as
/// a semantic plate ([`sorted_direction`]), never a text glyph.
fn header_label(column: &Column) -> String {
    column.label.clone()
}

/// The active sort's direction when it rests on this column: `Some(true)`
/// descending, `Some(false)` ascending, `None` unsorted.
fn sorted_direction(column: &Column, sort: Option<(InfoSortCol, SortDir)>) -> Option<bool> {
    match (sort, column.sort) {
        (Some((active, direction)), Some(own)) if active == own => Some(direction == SortDir::Desc),
        _ => None,
    }
}

// ---- world types: markers, events, per-page resources ----

#[derive(Clone, Component, Default)]
#[component(on_insert = bind_startup_page)]
pub(crate) struct StartupPageRoot;

#[derive(Clone, Component, Default)]
pub(crate) struct StartupBody;

#[derive(Clone, Component, Default)]
pub(crate) struct StartupStatusLine;

#[derive(Clone, Component, Default)]
pub(crate) struct StartupRowMarker(pub(crate) usize, pub(crate) StartupEntryId);

#[derive(Clone, Component, Default)]
pub(crate) struct StartupSortHeader(pub(crate) Option<InfoSortCol>);

#[derive(Resource)]
struct StartupPageBound;

#[derive(Resource)]
struct StartupRenderState {
    rendered_revision: Option<u64>,
}

/// 对接点 (W4 pointer picking): a header cell was clicked.
#[derive(Event)]
pub(crate) struct StartupSortClicked(pub(crate) InfoSortCol);

/// 对接点 (W4 pointer picking): a row was clicked; the payload is the VISUAL
/// row index, resolved through the shell's `sorted_startup_entry_at` only.
#[derive(Event)]
pub(crate) struct StartupRowClicked(pub(crate) usize);

/// 对接点 (M3 key routing): the selection moved by a row delta.
#[derive(Event)]
pub(crate) struct StartupSelectionMoved(pub(crate) isize);

/// Published on every accepted selection change. Grammar-complete today; the
/// W4 menu observers consume the payload when that surface lands.
#[derive(Event)]
pub(crate) struct StartupTargetSelected(#[allow(dead_code)] pub(crate) StartupEntryId);

// ---- render adapters (bsn!) ----

/// Content-region scene for the Startup page. The body's dynamic content is
/// painted by [`paint_startup`] — the single render authority for the rows.
pub(crate) fn content(_context: &PageContext<'_>) -> impl Scene + use<> {
    let title = Page::Startup.title();
    let waiting = t("common.waiting_inventory").to_owned();
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        StartupPageRoot
        Children [
            ( Text(title) TextRole(Role::Heading) ),
            (
                Text(waiting)
                StartupStatusLine
                TextRole(Role::Caption)
            ),
            (
                Node {
                    width: percent(100),
                    height: Val::Auto,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                StartupBody
            ),
        ]
    }
}

fn startup_body_scene(
    shell: &ShellApp,
    palette: &UiPalette,
    selection: &StartupSelection,
) -> impl Scene + use<> {
    let rows = startup_rows(shell);
    let selected = selected_row(&rows, selection);
    let sources = shell.projection().startup_source.as_deref();
    let notice = source_notice_text(sources);
    let empty = empty_state_text(sources);
    let evidence = evidence_line(shell);
    let children = body_children(&rows, selected, notice, evidence, empty, palette);
    let header = header_scene(shell.startup_sort, palette);
    bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            ( header ),
            { children },
        ]
    }
}

fn body_children(
    rows: &[StartupRowModel],
    selected: Option<usize>,
    notice: Option<String>,
    evidence: Option<String>,
    empty: String,
    palette: &UiPalette,
) -> Vec<Box<dyn Scene>> {
    let mut children = Vec::new();
    if let Some(text) = notice {
        children.push(Box::new(caption_line_scene(text)) as Box<dyn Scene>);
    }
    if let Some(text) = evidence {
        children.push(Box::new(caption_line_scene(text)) as Box<dyn Scene>);
    }
    if rows.is_empty() {
        children.push(Box::new(empty_scene(empty)) as Box<dyn Scene>);
    } else {
        for (index, row) in rows.iter().enumerate() {
            children.push(startup_row_scene(
                row,
                index,
                selected == Some(index),
                palette,
            ));
        }
    }
    children
}

/// Header row: one caption cell per column; every cell carries the
/// [`StartupSortHeader`] identity for the pointer adapter.
fn header_scene(sort: Option<(InfoSortCol, SortDir)>, palette: &UiPalette) -> impl Scene + use<> {
    let cells: Vec<Box<dyn Scene>> = columns()
        .into_iter()
        .map(|column| {
            let label = header_label(&column);
            let direction = sorted_direction(&column, sort);
            let indicator = sort_indicator_scene(direction, palette);
            let width = column.width_px;
            let sort_target = column.sort;
            Box::new(bsn! {
                Node {
                    width: px(width),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_4()),
                    overflow: Overflow::clip_x(),
                }
                StartupSortHeader(sort_target)
                Children [
                    ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) ),
                    { indicator },
                ]
            }) as Box<dyn Scene>
        })
        .collect();
    bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_8()),
            padding: UiRect::horizontal(Val::Px(space_8())),
        }
        Children [
            { cells }
        ]
    }
}

fn startup_row_scene(
    row: &StartupRowModel,
    index: usize,
    selected: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let widths = columns();
    let name = row.name.clone();
    let state = if row.enabled {
        t("common.enabled")
    } else {
        t("common.disabled")
    }
    .to_owned();
    let source = row.source.clone();
    let impact = row.impact.clone();
    let exec = row.exec.clone();
    let fill = if selected {
        palette.nav_active_bg
    } else {
        Color::NONE
    };
    let chip = chip_fill(enabled_chip(row.enabled), palette);
    let target = row.target.clone();
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    let name_width = widths[0].width_px;
    let state_width = widths[1].width_px;
    let source_width = widths[2].width_px;
    let impact_width = widths[3].width_px;
    let exec_width = widths[4].width_px;
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: px(height),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::horizontal(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor(fill)
        StartupRowMarker(index, target)
        Children [
            ( text_cell_scene(name, name_width, Role::Body) ),
            ( chip_cell_scene(state, state_width, chip, palette) ),
            ( text_cell_scene(source, source_width, Role::Body) ),
            ( text_cell_scene(impact, impact_width, Role::Body) ),
            ( text_cell_scene(exec, exec_width, Role::Body) ),
        ]
    })
}

fn text_cell_scene(text: String, width: f32, role: Role) -> impl Scene + use<> {
    bsn! {
        Node { width: px(width), align_items: AlignItems::FlexStart }
        Children [
            ( Text(text) TextRole(role) ),
        ]
    }
}

fn chip_cell_scene(
    word: String,
    width: f32,
    fill: Color,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let radius = palette.control_radius_px;
    bsn! {
        Node { width: px(width), align_items: AlignItems::Center }
        Children [
            (
                Node {
                    height: Val::Auto,
                    padding: UiRect::horizontal(Val::Px(space_8())),
                    border_radius: BorderRadius::all(Val::Px(radius)),
                }
                BackgroundColor(fill)
                Children [
                    ( Text(word) TextRole(Role::Caption) ),
                ]
            ),
        ]
    }
}

fn caption_line_scene(text: String) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100) }
        Children [
            ( Text(text) TextRole(Role::Caption) ),
        ]
    }
}

fn empty_scene(message: String) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            flex_grow: 1.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: UiRect::all(Val::Px(space_24())),
        }
        Children [
            ( Text(message) TextRole(Role::Body) ),
        ]
    }
}

// ---- observers and the single paint path ----

/// The one authoritative repaint; see `paint_services` in the template page.
fn paint_startup(world: &mut World) {
    let palette = world.resource::<WindowPalette>().inner.clone();
    let revision = world
        .non_send::<FrontendTrack>()
        .shell
        .projection()
        .startup_revision;
    let mut selection = world.resource::<StartupSelection>().clone();
    let (scene, line) = {
        let shell = &world.non_send::<FrontendTrack>().shell;
        let rows = startup_rows(shell);
        if let Some(target) = &selection.target
            && !rows.iter().any(|row| &row.target == target)
        {
            // A target that left the inventory deselects honestly.
            selection.target = None;
        }
        (
            startup_body_scene(shell, &palette, &selection),
            status_line_text(shell, rows.len()),
        )
    };
    world.resource_mut::<StartupRenderState>().rendered_revision = Some(revision);
    world.resource_mut::<StartupSelection>().target = selection.target;
    // A childless container has no `Children` component in this bevy, so the
    // join must be optional — the first paint finds an empty body.
    let mut body_query = world.query_filtered::<(Entity, Option<&Children>), With<StartupBody>>();
    let Some((body, children)) = body_query.iter(world).next() else {
        return;
    };
    let stale: Vec<Entity> = children
        .map(|children| children.iter().copied().collect())
        .unwrap_or_default();
    let mut commands = world.commands();
    for entity in stale {
        commands.entity(entity).despawn();
    }
    let fresh = commands.spawn_scene(scene).id();
    commands.entity(body).add_one_related::<ChildOf>(fresh);
    let mut line_query = world.query_filtered::<&mut Text, With<StartupStatusLine>>();
    if let Ok(mut text) = line_query.single_mut(world) {
        text.0 = line;
    }
}

/// Initial/mount paint: the body container just came to exist, so the first
/// (and every remount) row projection can bind to it.
fn on_startup_body_added(_added: On<Add, StartupBody>, mut commands: Commands) {
    commands.queue(paint_startup);
}

/// Insert hook: bind the page's observers once; the initial paint rides the
/// body-added observer registered below.
fn bind_startup_page(mut world: DeferredWorld<'_>, _context: HookContext) {
    if world.get_resource_mut::<StartupPageBound>().is_some() {
        return;
    }
    let mut commands = world.commands();
    commands.insert_resource(StartupPageBound);
    commands.init_resource::<StartupSelection>();
    commands.insert_resource(StartupRenderState {
        rendered_revision: None,
    });
    commands.add_observer(on_startup_projection_folded);
    commands.add_observer(on_startup_sort_clicked);
    commands.add_observer(on_startup_row_clicked);
    commands.add_observer(on_startup_selection_moved);
    // The initial paint rides the body's own insertion: the hook runs while
    // the page scene is still spawning (its children apply later in the same
    // command queue), so painting here would find no body yet. The observer
    // fires exactly when the body entity comes to exist — and again on every
    // route-back remount.
    commands.add_observer(on_startup_body_added);
}

/// Fold repaint with the idle gate (startup-domain revision only).
fn on_startup_projection_folded(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    rendered: Res<StartupRenderState>,
    mut commands: Commands,
) {
    let revision = track.shell().projection().startup_revision;
    if rendered.rendered_revision == Some(revision) {
        return;
    }
    commands.queue(paint_startup);
}

/// Header-sort tail: the shell's existing sort entry owns the decision.
fn on_startup_sort_clicked(
    click: On<StartupSortClicked>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    track
        .shell
        .set_info_sort(InfoTable::Startup, click.event().0);
    commands.queue(paint_startup);
}

/// Row-click tail. The visual row resolves to its target through the shell's
/// single `sorted_startup_entry_at` translation.
fn on_startup_row_clicked(
    click: On<StartupRowClicked>,
    track: ShellTrack,
    mut selection: ResMut<StartupSelection>,
    mut commands: Commands,
) {
    let Some(entry) = track.shell().sorted_startup_entry_at(click.event().0) else {
        return;
    };
    let target = entry.id.clone();
    selection.target = Some(target.clone());
    commands.trigger(StartupTargetSelected(target));
    commands.queue(paint_startup);
}

/// Keyboard-selection tail: sort-stable target id, clamped cursor.
fn on_startup_selection_moved(
    movement: On<StartupSelectionMoved>,
    track: ShellTrack,
    mut selection: ResMut<StartupSelection>,
    mut commands: Commands,
) {
    let rows = startup_rows(track.shell());
    let current = selected_row(&rows, &selection);
    let Some(next) = moved_row(rows.len(), current, movement.event().0) else {
        return;
    };
    let Some(entry) = track.shell().sorted_startup_entry_at(next) else {
        return;
    };
    let target = entry.id.clone();
    selection.target = Some(target.clone());
    commands.trigger(StartupTargetSelected(target));
    commands.queue(paint_startup);
}

#[cfg(test)]
#[path = "../../tests/headless/pages/startup.rs"]
mod tests;
