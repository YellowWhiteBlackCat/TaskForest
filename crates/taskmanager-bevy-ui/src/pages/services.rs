//! Services page — the inventory-table template for the list pages.
//!
//! **Data entry** (page-agent contract, `crate::pages`): rows project through
//! [`ShellApp::sorted_services`] (the memoized shared sort order — never the
//! provider slice), and a visual row resolves back to its target ONLY through
//! [`ShellApp::sorted_service_at`], the shell's single "row N → target"
//! translation. Bypassing that accessor is the wrong-row defect class this
//! page must never reintroduce.
//!
//! **Refresh**: the [`ServicesPageRoot`] insert hook registers the page's
//! observers exactly once per `World`; [`ShellProjectionFolded`] repaints the
//! table body only when the services-domain revision advanced, so idle frames
//! and unrelated batches leave the tree untouched (zero redraw at rest).
//!
//! **Interaction seams** (对接点): pointer picking and per-page key routing
//! fire the page-local events
//! [`ServiceSortClicked`], [`ServiceRowClicked`] and [`ServiceSelectionMoved`]
//! — everything downstream of those events is live here. The action-menu
//! surface reads the current target from the [`ServiceSelection`] resource;
//! destructive verbs then route through the shell's existing
//! `select_service_control` gate. This page never mutates platform state.
//!
//! **Colors**: every fill and ink comes from `context.palette` roles. The
//! palette has no success/danger tokens yet, so the status chips derive from
//! palette-owned roles (accent/scrim/dim); when [`crate::palette::UiPalette`]
//! grows semantic status tokens, [`chip_fill`] is the one function to
//! re-target.

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
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::core::target::ServiceId;

use taskmanager_shell::presentation::control_error_detail;
use taskmanager_shell::{InfoSortCol, InfoTable, ShellApp, SortDir};

use crate::app::{FrontendTrack, Page, PageContext, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, no_wrap_text, space_2, space_4, space_8, space_24};
use crate::widgets::controls::sort_indicator_scene;
use crate::window::{Role, TextRole, WindowPalette};

pub(crate) mod log_panel;
pub(crate) mod menu;

// ---- pure core: row view model, chips, empty/notice/status copy ----

/// One Services-table row: display material plus the opaque target id every
/// action (selection, future menus) addresses. Sort-stable by construction.
pub(crate) struct ServiceRowModel {
    pub(crate) target: ServiceId,
    pub(crate) name: String,
    pub(crate) status: ServiceStatus,
    pub(crate) description: String,
}

/// Project the table's whole material through the shared sort order. The
/// renderer, the selection model and the tests all consume this one function,
/// so the visible order can never drift from the shell's.
pub(crate) fn service_rows(shell: &ShellApp) -> Vec<ServiceRowModel> {
    shell
        .sorted_services()
        .into_iter()
        .map(|service: &ServiceItem| ServiceRowModel {
            target: service.id.clone(),
            name: service.name.clone(),
            status: service.status,
            description: service.description.clone(),
        })
        .collect()
}

/// Semantic status chip for one row. Pure; the fill mapping lives in
/// [`chip_fill`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusChip {
    /// Running (TUI `good` parity): active attention.
    Positive,
    /// Failed (TUI `danger` parity): blocked attention.
    Negative,
    /// Inactive/unknown: idle.
    Idle,
}

pub(crate) fn service_chip(status: ServiceStatus) -> StatusChip {
    match status {
        ServiceStatus::Active => StatusChip::Positive,
        ServiceStatus::Failed => StatusChip::Negative,
        ServiceStatus::Inactive | ServiceStatus::Unknown => StatusChip::Idle,
    }
}

/// Token-tinted fill: the token's own channels at a reduced alpha, so the
/// palette stays the only color source (no literal hues at call sites).
fn tinted(base: Color, alpha: f32) -> Color {
    let srgba = base.to_srgba();
    Color::srgba(srgba.red, srgba.green, srgba.blue, srgba.alpha * alpha)
}

/// Chip fill per semantic kind, from palette roles only. Seam: re-target to
/// the palette's success/danger/warning tokens when they exist.
pub(crate) fn chip_fill(chip: StatusChip, palette: &UiPalette) -> Color {
    match chip {
        StatusChip::Positive => tinted(palette.accent, 0.22),
        StatusChip::Negative => tinted(palette.scrim, 0.60),
        StatusChip::Idle => tinted(palette.dim_color, 0.18),
    }
}

/// Degraded-source headline rendered above the rows (TUI source-panel parity,
/// without its refresh-key hint — this frontend has no refresh chord yet).
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
/// reading as a confirmed "no services".
pub(crate) fn empty_state_text(sources: Option<&[SourceStatus]>) -> String {
    source_notice_text(sources).unwrap_or_else(|| t("empty.no_services_reported").to_owned())
}

/// The summary line under the title: honest count plus the active sort.
pub(crate) fn status_line_text(shell: &ShellApp, rows: usize) -> String {
    let noun = t("svc.noun");
    match shell.services_sort {
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

/// The page's selection state: the opaque target id, never a row index, so a
/// sort change can never make the highlight land on a different service.
/// Action-menu target: destructive verbs read the id from here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Resource)]
pub(crate) struct ServiceSelection {
    pub(crate) target: Option<ServiceId>,
}

/// Row of the selected target in the CURRENT projection (`None` when the
/// target left the inventory or nothing is selected).
pub(crate) fn selected_row(
    rows: &[ServiceRowModel],
    selection: &ServiceSelection,
) -> Option<usize> {
    let target = selection.target.as_ref()?;
    rows.iter().position(|row| &row.target == target)
}

/// Clamp-move a row cursor: `delta` saturates at the first/last row, an empty
/// table stays unselected, and a move from "nothing selected" enters at the
/// first row regardless of direction. Pure — the keyboard tail and the tests
/// share it.
pub(crate) fn moved_row(rows_len: usize, current: Option<usize>, delta: isize) -> Option<usize> {
    if rows_len == 0 {
        return None;
    }
    let max = (rows_len - 1) as isize;
    let moved = current.map_or(0, |row| row as isize + delta);
    Some(moved.clamp(0, max) as usize)
}

// ---- page-owned column vocabulary ----

/// One column: display label (shared catalog), width, and the shared sort
/// column it toggles (`None` = not sortable). Inventory columns are
/// page-owned because the shared ui-contract currently defines no inventory
/// column vocabulary; row identity and control semantics still come from the
/// shared shell contracts.
struct Column {
    sort: Option<InfoSortCol>,
    label: String,
    width_px: f32,
}

fn columns() -> Vec<Column> {
    vec![
        Column {
            sort: Some(InfoSortCol::Name),
            label: t("common.service").to_owned(),
            width_px: 260.0,
        },
        Column {
            sort: Some(InfoSortCol::Status),
            label: t("common.status").to_owned(),
            width_px: 130.0,
        },
        Column {
            sort: None,
            label: t("common.description").to_owned(),
            width_px: 420.0,
        },
    ]
}

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

/// Page root. Its insert hook binds the page's observers exactly once per
/// `World` (route remounts re-insert this component; the guard keeps the
/// registration idempotent) and queues the first authoritative body paint.
#[derive(Clone, Component, Default)]
#[component(on_insert = bind_services_page)]
pub(crate) struct ServicesPageRoot;

/// The rebuildable table block (header + notice + rows or empty state).
#[derive(Clone, Component, Default)]
pub(crate) struct ServicesBody;

/// The one summary text node the observers rewrite in place.
#[derive(Clone, Component, Default)]
pub(crate) struct ServicesStatusLine;

/// Per-row identity: visual row index plus the opaque target id, so a future
/// pointer adapter maps a clicked row to `ServiceRowClicked` without touching
/// provider order.
#[derive(Clone, Component, Default)]
pub(crate) struct ServicesRowMarker(pub(crate) usize, pub(crate) ServiceId);

/// Header-cell sort identity for the future pointer adapter: `Some` marks a
/// sortable column, `None` a display-only one. `Option` keeps the bsn!
/// template seed (`Default`) honest for the non-sortable cells.
#[derive(Clone, Component, Default)]
pub(crate) struct ServicesSortHeader(pub(crate) Option<InfoSortCol>);

/// Guard resource: the observer set for this page already exists.
#[derive(Resource)]
struct ServicesPageBound;

/// The services-domain revision the body was last painted from; the fold
/// observer's idle gate.
#[derive(Resource)]
struct ServicesRenderState {
    rendered_revision: Option<u64>,
}

/// Pointer input: a header cell was clicked. The observer
/// routes through the shell's existing sort entry (`set_info_sort`).
#[derive(Event)]
pub(crate) struct ServiceSortClicked(pub(crate) InfoSortCol);

/// Pointer input: a row was clicked; the payload is the VISUAL
/// row index, resolved through the shell's `sorted_service_at` only.
#[derive(Event)]
pub(crate) struct ServiceRowClicked(pub(crate) usize);

/// Keyboard input: the selection moved by a row delta.
#[derive(Event)]
pub(crate) struct ServiceSelectionMoved(pub(crate) isize);

// ---- render adapters (bsn!) ----

/// Content-region scene for the Services page: title, summary line and the
/// body container. The body's dynamic content is painted by
/// [`paint_services`] (queued by the insert hook and every observer), so the
/// declarative tree stays structure-only and there is exactly one render
/// authority for the rows.
pub(crate) fn content(_context: &PageContext<'_>) -> impl Scene + use<> {
    let title = Page::Services.title();
    let waiting = t("common.waiting_inventory").to_owned();
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        ServicesPageRoot
        Children [
            ( Text(title) TextRole(Role::Heading) ),
            (
                Text(waiting)
                ServicesStatusLine
                TextRole(Role::Caption)
            ),
            (
                Node {
                    width: percent(100),
                    height: Val::Auto,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                ServicesBody
            ),
            (
                // The service-log panel's mount point. The panel is a
                // page-local surface fed by the shell's log lifecycle; its
                // painter is fingerprint-gated so idle folds never respawn.
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                log_panel::ServicesLogPanelSlot
            ),
        ]
    }
}

/// The table body: header row plus (notice, rows or empty state). Rebuilt as
/// one scene by [`paint_services`].
fn services_body_scene(
    shell: &ShellApp,
    palette: &UiPalette,
    selection: &ServiceSelection,
) -> impl Scene + use<> {
    let rows = service_rows(shell);
    let selected = selected_row(&rows, selection);
    let notice = source_notice_text(shell.projection().services_source.as_deref());
    let empty = empty_state_text(shell.projection().services_source.as_deref());
    let children = body_children(&rows, selected, notice, empty, palette);
    let header = header_scene(shell.services_sort, palette);
    let toolbar = log_panel::logs_toolbar_scene(selection.target.is_some(), palette);
    bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            ( toolbar ),
            ( header ),
            { children },
        ]
    }
}

fn body_children(
    rows: &[ServiceRowModel],
    selected: Option<usize>,
    notice: Option<String>,
    empty: String,
    palette: &UiPalette,
) -> Vec<Box<dyn Scene>> {
    let mut children = Vec::new();
    if let Some(text) = notice {
        children.push(Box::new(caption_line_scene(text)) as Box<dyn Scene>);
    }
    if rows.is_empty() {
        children.push(Box::new(empty_scene(empty)) as Box<dyn Scene>);
    } else {
        for (index, row) in rows.iter().enumerate() {
            children.push(service_row_scene(
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
/// [`ServicesSortHeader`] identity (`Some` on sortable columns) for the
/// pointer adapter.
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
                ServicesSortHeader(sort_target)
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

fn service_row_scene(
    row: &ServiceRowModel,
    index: usize,
    selected: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let widths = columns();
    let name = row.name.clone();
    let description = row.description.clone();
    let status = row.status.as_str().to_owned();
    let fill = if selected {
        palette.nav_active_bg
    } else {
        Color::NONE
    };
    let chip = chip_fill(service_chip(row.status), palette);
    let target = row.target.clone();
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    let name_width = widths[0].width_px;
    let status_width = widths[1].width_px;
    let description_width = widths[2].width_px;
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
        ServicesRowMarker(index, target)
        Children [
            ( text_cell_scene(name, name_width, Role::Body) ),
            ( chip_cell_scene(status, status_width, chip, palette) ),
            ( text_cell_scene(description, description_width, Role::Body) ),
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

/// The one authoritative repaint: rebuild the body from the live projection
/// and rewrite the summary line. Queued as a command by the insert hook and
/// every page observer; nothing else mutates the body.
fn paint_services(world: &mut World) {
    let palette = world.resource::<WindowPalette>().inner.clone();
    let revision = world
        .non_send::<FrontendTrack>()
        .shell
        .projection()
        .services_revision;
    let mut selection = world.resource::<ServiceSelection>().clone();
    let (scene, line) = {
        let shell = &world.non_send::<FrontendTrack>().shell;
        let rows = service_rows(shell);
        if let Some(target) = &selection.target
            && !rows.iter().any(|row| &row.target == target)
        {
            // A target that left the inventory deselects honestly — never a
            // silent jump to a neighbor row.
            selection.target = None;
        }
        (
            services_body_scene(shell, &palette, &selection),
            status_line_text(shell, rows.len()),
        )
    };
    world
        .resource_mut::<ServicesRenderState>()
        .rendered_revision = Some(revision);
    world.resource_mut::<ServiceSelection>().target = selection.target;
    // A childless container has no `Children` component in this bevy, so the
    // join must be optional — the first paint finds an empty body.
    let mut body_query = world.query_filtered::<(Entity, Option<&Children>), With<ServicesBody>>();
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
    let mut line_query = world.query_filtered::<&mut Text, With<ServicesStatusLine>>();
    if let Ok(mut text) = line_query.single_mut(world) {
        text.0 = line;
    }
}

/// Insert hook: bind the page's observers once; the initial paint rides
/// [`on_services_body_added`] (see the comment at the registration site),
/// because the scene's children apply later in the same command queue.
/// Initial/mount paint: the body container just came to exist, so the first
/// (and every remount) row projection can bind to it.
fn on_services_body_added(_added: On<Add, ServicesBody>, mut commands: Commands) {
    commands.queue(paint_services);
}

fn bind_services_page(mut world: DeferredWorld<'_>, _context: HookContext) {
    if world.get_resource_mut::<ServicesPageBound>().is_some() {
        return;
    }
    let mut commands = world.commands();
    commands.insert_resource(ServicesPageBound);
    commands.init_resource::<ServiceSelection>();
    commands.insert_resource(ServicesRenderState {
        rendered_revision: None,
    });
    commands.init_resource::<log_panel::ServicesLogRenderState>();
    commands.add_observer(on_services_projection_folded);
    commands.add_observer(on_services_sort_clicked);
    commands.add_observer(on_services_row_clicked);
    commands.add_observer(on_services_selection_moved);
    commands.add_observer(log_panel::on_services_logs_requested);
    commands.add_observer(log_panel::on_log_panel_repaint_required);
    commands.add_observer(log_panel::on_log_panel_slot_added);
    commands.add_observer(log_panel::on_services_fold_log_gate);
    commands.add_observer(log_panel::services_logs_button_activated);
    // The initial paint rides the body's own insertion: the hook runs while
    // the page scene is still spawning (its children apply later in the same
    // command queue), so painting here would find no body yet. The observer
    // fires exactly when the body entity comes to exist — and again on every
    // route-back remount.
    commands.add_observer(on_services_body_added);
}

/// Fold repaint with the idle gate: only a services-domain revision advance
/// (new inventory or new source status) repaints; unrelated batches and idle
/// frames leave the tree untouched.
fn on_services_projection_folded(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    rendered: Res<ServicesRenderState>,
    mut commands: Commands,
) {
    let revision = track.shell().projection().services_revision;
    if rendered.rendered_revision == Some(revision) {
        return;
    }
    commands.queue(paint_services);
}

/// Header-sort tail: the shell's existing sort entry owns the decision (same
/// click-again-flips-direction semantics every frontend shares), then the
/// repaint projects the new order.
fn on_services_sort_clicked(
    click: On<ServiceSortClicked>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    track
        .shell
        .set_info_sort(InfoTable::Services, click.event().0);
    commands.queue(paint_services);
}

/// Row-click tail. The visual row resolves to its target through the shell's
/// single `sorted_service_at` translation — never a provider-order index.
fn on_services_row_clicked(
    click: On<ServiceRowClicked>,
    track: ShellTrack,
    mut selection: ResMut<ServiceSelection>,
    mut commands: Commands,
) {
    let Some(service) = track.shell().sorted_service_at(click.event().0) else {
        return;
    };
    selection.target = Some(service.id.clone());
    commands.queue(paint_services);
}

/// Keyboard-selection tail: resolve the CURRENT selected row from the target
/// id (sort-stable), clamp-move it, then translate the new row back to a
/// target through the shell accessor.
fn on_services_selection_moved(
    movement: On<ServiceSelectionMoved>,
    track: ShellTrack,
    mut selection: ResMut<ServiceSelection>,
    mut commands: Commands,
) {
    let rows = service_rows(track.shell());
    let current = selected_row(&rows, &selection);
    let Some(next) = moved_row(rows.len(), current, movement.event().0) else {
        return;
    };
    let Some(service) = track.shell().sorted_service_at(next) else {
        return;
    };
    selection.target = Some(service.id.clone());
    commands.queue(paint_services);
}

#[cfg(test)]
#[path = "../../tests/headless/pages/services.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/services_menu.rs"]
mod services_menu_tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/service_logs.rs"]
mod service_logs_tests;
