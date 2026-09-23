//! Services page — the inventory-table template for the list pages.
//!
//! **Data entry** (page-agent contract, `crate::pages`): rows project through
//! [`ShellApp::sorted_services`] (the memoized shared sort order — never the
//! provider slice), and a visual row resolves back to its target ONLY through
//! [`ShellApp::sorted_service_at`], the shell's single "row N → target"
//! translation. Bypassing that accessor is the wrong-row defect class this
//! page must never reintroduce.
//!
//! **Refresh**: the `ServicesPageRoot` insert hook registers the page's
//! observers exactly once per `World`; `ShellProjectionFolded` repaints the
//! table body only when the services-domain revision advanced, so idle frames
//! and unrelated batches leave the tree untouched (zero redraw at rest).
//!
//! **Interaction seams** (对接点): pointer picking and per-page key routing
//! fire the page-local events
//! `ServiceSortClicked`, `ServiceRowClicked` and `ServiceSelectionMoved`
//! — everything downstream of those events is live here. The action-menu
//! surface reads the current target from the `ServiceSelection` resource;
//! destructive verbs then route through the shell's existing
//! `select_service_control` gate. This page never mutates platform state.
//!
//! **Colors**: every fill and ink comes from `context.palette` roles. The
//! palette has no success/danger tokens yet, so the status chips derive from
//! palette-owned roles (accent/scrim/dim); when `crate::palette::UiPalette`
//! grows semantic status tokens, `chip_fill` is the one function to
//! re-target.

use crate::widgets::controls::{ControlTone, ControlVisual};
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::{Add, HookContext};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use bevy::ecs::world::{DeferredWorld, World};
use bevy::picking::Pickable;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on, template_value};
use bevy::text::TextColor;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::AppAction;
use taskmanager_application::i18n::t;
use taskmanager_application::{SourceNotice, source_notice};
use taskmanager_core::core::services::{ServiceAction, ServiceItem, ServiceStatus};
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::core::target::ServiceId;

use taskmanager_shell::presentation::control_error_detail;
use taskmanager_shell::{InfoSortCol, InfoTable, ShellApp, SortDir};

use crate::app::{FrontendTrack, Page, PageContext, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, no_wrap_text, space_2, space_4, space_8, space_12, space_24};
use crate::widgets::controls::sort_indicator_scene;
use crate::window::{Role, TextRole, WindowPalette};
use taskmanager_ui_contract::IconId;

pub(crate) mod dependencies_panel;
pub(crate) mod details_modal;
pub(crate) mod log_panel;
pub(crate) mod menu;
mod scene;

use scene::{services_body_scene, services_search_input_scene};

// ---- pure core: row view model, chips, empty/notice/status copy ----

/// One Services-table row: display material plus the opaque target id every
/// action (selection, future menus) addresses. Sort-stable by construction.
pub(crate) struct ServiceRowModel {
    pub(crate) target: ServiceId,
    pub(crate) name: String,
    pub(crate) status: ServiceStatus,
    pub(crate) description: String,
    pub(crate) cycle: bool,
}

/// Project the table's whole material through the shared sort order and query filter.
pub(crate) fn service_rows(shell: &ShellApp) -> Vec<ServiceRowModel> {
    service_rows_filtered(shell, &shell.query)
}

pub(crate) fn service_rows_filtered(shell: &ShellApp, query: &str) -> Vec<ServiceRowModel> {
    let q = query.trim().to_lowercase();
    let cycle_members = shell
        .projection()
        .services
        .as_deref()
        .map(taskmanager_shell::service_cycle_members)
        .unwrap_or_default();
    shell
        .sorted_services()
        .into_iter()
        .filter(|service| {
            if q.is_empty() {
                true
            } else {
                service.name.to_lowercase().contains(&q)
                    || service.description.to_lowercase().contains(&q)
            }
        })
        .map(|service: &ServiceItem| ServiceRowModel {
            target: service.id.clone(),
            name: service.name.clone(),
            status: service.status,
            description: service.description.clone(),
            cycle: cycle_members.contains(&service.id),
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

#[derive(Component, Clone, Default)]
pub(crate) struct ServicesSearchInput;

// ---- render adapters (bsn!) ----

/// Content-region scene for the Services page: title, summary line and the
/// body container. The body's dynamic content is painted by
/// [`paint_services`] (queued by the insert hook and every observer), so the
/// declarative tree stays structure-only and there is exactly one render
/// authority for the rows.
pub(crate) fn content(context: &PageContext<'_>) -> impl Scene + use<> {
    let title = Page::Services.title();
    let waiting = t("common.waiting_inventory").to_owned();
    let search = services_search_input_scene(context.palette, &context.shell.query);
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
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                }
                Children [
                    (
                        Text(waiting)
                        ServicesStatusLine
                        TextRole(Role::Caption)
                    ),
                    ( { search } ),
                ]
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
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                dependencies_panel::ServicesDependenciesPanelSlot
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

#[derive(Component, Clone, Default)]
pub(crate) struct ServiceStartButton;

#[derive(Component, Clone, Default)]
pub(crate) struct ServiceStopButton;

#[derive(Component, Clone, Default)]
pub(crate) struct ServiceRestartButton;

fn on_service_start_button_activated(
    activate: On<Activate>,
    buttons: Query<&ServiceStartButton>,
    mut track: NonSendMut<FrontendTrack>,
    selection: Res<ServiceSelection>,
    mut commands: Commands,
) {
    let button = buttons
        .get(activate.entity)
        .or_else(|_| buttons.get(activate.event().entity));
    if button.is_err() {
        return;
    }
    if let Some(target) = &selection.target {
        let service = track
            .shell
            .sorted_services()
            .into_iter()
            .find(|s| &s.id == target)
            .cloned();
        if let Some(service) = service {
            if track
                .shell
                .select_service_control(&service, ServiceAction::Start)
            {
                let _ = track.shell.apply_action(AppAction::RequestServiceControl);
                crate::confirmation::republish(&track.shell, &mut commands);
                commands.trigger(crate::input::ShellInteractionApplied);
            }
            commands.queue(paint_services);
        }
    }
}

fn on_service_stop_button_activated(
    activate: On<Activate>,
    buttons: Query<&ServiceStopButton>,
    mut track: NonSendMut<FrontendTrack>,
    selection: Res<ServiceSelection>,
    mut commands: Commands,
) {
    let button = buttons
        .get(activate.entity)
        .or_else(|_| buttons.get(activate.event().entity));
    if button.is_err() {
        return;
    }
    if let Some(target) = &selection.target {
        let service = track
            .shell
            .sorted_services()
            .into_iter()
            .find(|s| &s.id == target)
            .cloned();
        if let Some(service) = service {
            if track
                .shell
                .select_service_control(&service, ServiceAction::Stop)
            {
                let _ = track.shell.apply_action(AppAction::RequestServiceControl);
                crate::confirmation::republish(&track.shell, &mut commands);
                commands.trigger(crate::input::ShellInteractionApplied);
            }
            commands.queue(paint_services);
        }
    }
}

fn on_service_restart_button_activated(
    activate: On<Activate>,
    buttons: Query<&ServiceRestartButton>,
    mut track: NonSendMut<FrontendTrack>,
    selection: Res<ServiceSelection>,
    mut commands: Commands,
) {
    let button = buttons
        .get(activate.entity)
        .or_else(|_| buttons.get(activate.event().entity));
    if button.is_err() {
        return;
    }
    if let Some(target) = &selection.target {
        let service = track
            .shell
            .sorted_services()
            .into_iter()
            .find(|s| &s.id == target)
            .cloned();
        if let Some(service) = service {
            if track
                .shell
                .select_service_control(&service, ServiceAction::Restart)
            {
                let _ = track.shell.apply_action(AppAction::RequestServiceControl);
                crate::confirmation::republish(&track.shell, &mut commands);
                commands.trigger(crate::input::ShellInteractionApplied);
            }
            commands.queue(paint_services);
        }
    }
}

fn on_services_sort_header_activated(
    activate: On<Activate>,
    headers: Query<&ServicesSortHeader>,
    mut commands: Commands,
) {
    let header = headers
        .get(activate.entity)
        .or_else(|_| headers.get(activate.event().entity));
    if let Ok(ServicesSortHeader(Some(col))) = header {
        commands.trigger(ServiceSortClicked(*col));
    }
}

fn on_services_row_activated(
    activate: On<Activate>,
    markers: Query<&ServicesRowMarker>,
    mut commands: Commands,
) {
    let marker = markers
        .get(activate.entity)
        .or_else(|_| markers.get(activate.event().entity));
    if let Ok(marker) = marker {
        commands.trigger(ServiceRowClicked(marker.0));
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
    let shell_query = world.non_send::<FrontendTrack>().shell.query.clone();
    let new_text = if shell_query.is_empty() {
        t("search.services").to_owned()
    } else {
        shell_query
    };
    let mut search_query = world.query_filtered::<&mut Text, With<ServicesSearchInput>>();
    for mut text_node in search_query.iter_mut(world) {
        if text_node.0 != new_text {
            text_node.0 = new_text.clone();
        }
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
    commands.init_resource::<dependencies_panel::ServicesDependenciesRenderState>();
    commands.init_resource::<log_panel::ServicesLogRenderState>();
    commands.add_observer(on_services_projection_folded);
    commands.add_observer(on_services_sort_clicked);
    commands.add_observer(on_services_row_clicked);
    commands.add_observer(on_services_selection_moved);
    commands.add_observer(dependencies_panel::on_services_dependencies_requested);
    commands.add_observer(dependencies_panel::on_dependencies_panel_repaint_required);
    commands.add_observer(dependencies_panel::on_dependencies_panel_slot_added);
    commands.add_observer(dependencies_panel::on_services_fold_dependencies_gate);
    commands.add_observer(log_panel::on_services_logs_requested);
    commands.add_observer(log_panel::on_log_panel_repaint_required);
    commands.add_observer(log_panel::on_log_panel_slot_added);
    commands.add_observer(log_panel::on_services_fold_log_gate);
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

#[cfg(test)]
#[path = "../../tests/headless/pages/service_dependencies.rs"]
mod service_dependencies_tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/service_details_modal.rs"]
mod service_details_modal_tests;
