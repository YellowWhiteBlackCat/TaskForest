//! Sessions page — inventory table over the shared sessions sort projection.
//!
//! Same architecture as [`crate::pages::services`] (the template page); the
//! differences are the data family and its semantics:
//!
//! - rows project through [`ShellApp::sorted_sessions`] and a visual row
//!   resolves back to its target ONLY through
//!   [`ShellApp::sorted_session_at`] (the shell's single "row N → target"
//!   translation — bypassing it is the wrong-row defect class);
//! - the seat/tty summary renders per row with the shared `MISSING_VALUE`
//!   marker for unobserved fields (never a fabricated empty string), and the
//!   type column reads Local/Remote from the shared catalog;
//! - an empty list from a FAILED source renders the typed reason via the
//!   source-status notice — never "no sessions";
//! - the last accepted session-control outcome renders as one caption line
//!   under the table (GPUI feedback-status parity, read-only: the Disconnect/
//!   Lock verbs are routed by the action menu).
//!
//! Action-menu seam: the disconnect/lock verbs read the current target
//! from [`SessionSelection`]; the pointer and key adapters fire
//! [`SessionSortClicked`], [`SessionRowClicked`] and [`SessionSelectionMoved`].
//! Colors are palette roles only.

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::{Add, HookContext};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::Query;
use bevy::ecs::system::{Commands, NonSendMut, Res, ResMut};
use bevy::ecs::world::{DeferredWorld, World};
use bevy::scene::{CommandsSceneExt, Scene, bsn, on, template_value};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_application::{SessionControlOutcome, SourceNotice, source_notice};
use taskmanager_core::core::session::{SessionControlAction, SessionItem};
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::core::target::SessionId;

use taskmanager_shell::presentation::{MISSING_VALUE, control_error_detail};
use taskmanager_shell::{InfoSortCol, InfoTable, ShellApp, SortDir};

use crate::app::{FrontendTrack, Page, PageContext, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, no_wrap_text, space_2, space_4, space_8, space_24};
use crate::widgets::controls::sort_indicator_scene;
use crate::window::{Role, TextRole, WindowPalette};

pub(crate) mod menu;

// ---- pure core: row view model, seat/tty summary, copy ----

/// One Users-table row: display material plus the provider session id.
pub(crate) struct SessionRowModel {
    pub(crate) target: SessionId,
    pub(crate) session: String,
    pub(crate) user: String,
    pub(crate) seat: String,
    pub(crate) tty: String,
    pub(crate) kind: &'static str,
    pub(crate) since: String,
}

/// Project the table's whole material through the shared sort order.
pub(crate) fn session_rows(shell: &ShellApp) -> Vec<SessionRowModel> {
    shell
        .sorted_sessions()
        .into_iter()
        .map(|session: &SessionItem| SessionRowModel {
            target: session.id.clone(),
            session: session.id.to_string(),
            user: session.user.clone(),
            seat: session_seat_text(session),
            tty: session_tty_text(session),
            kind: if session.remote {
                t("users.remote")
            } else {
                t("users.local")
            },
            since: session
                .timestamp
                .clone()
                .unwrap_or_else(|| MISSING_VALUE.to_owned()),
        })
        .collect()
}

/// Seat summary: the observed seat, or the shared missing-value marker — an
/// unobserved seat is never fabricated as an empty string.
pub(crate) fn session_seat_text(session: &SessionItem) -> String {
    session
        .seat
        .clone()
        .unwrap_or_else(|| MISSING_VALUE.to_owned())
}

/// Tty summary, same honesty rule as the seat.
pub(crate) fn session_tty_text(session: &SessionItem) -> String {
    session
        .tty
        .clone()
        .unwrap_or_else(|| MISSING_VALUE.to_owned())
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
/// reading as a confirmed "no sessions" (GPUI empty-state-failure parity).
pub(crate) fn empty_state_text(sources: Option<&[SourceStatus]>) -> String {
    source_notice_text(sources).unwrap_or_else(|| t("users.no_sessions").to_owned())
}

/// One accepted session-control outcome as display text (GPUI/TUI feedback
/// parity: success and failure both name the action and the target).
pub(crate) fn feedback_line_text(outcome: &SessionControlOutcome) -> String {
    let target = outcome.session_id.to_string();
    let action = match outcome.action {
        SessionControlAction::Disconnect => t("users.disconnect"),
        SessionControlAction::Lock => t("users.lock"),
    };
    match outcome.result {
        Ok(()) => t("feedback.action_succeeded")
            .replace("{action}", action)
            .replace("{target}", &target),
        Err(error) => t("feedback.action_failed_detail")
            .replace("{action}", action)
            .replace("{target}", &target)
            .replace("{detail}", control_error_detail(error)),
    }
}

/// The summary line under the title: honest count plus the active sort.
pub(crate) fn status_line_text(shell: &ShellApp, rows: usize) -> String {
    let noun = t("users.sessions");
    match shell.sessions_sort {
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

/// The page's selection state: the provider session id, never a row index.
/// Action-menu target: disconnect/lock verbs read the id from here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Resource)]
pub(crate) struct SessionSelection {
    pub(crate) target: Option<SessionId>,
}

pub(crate) fn selected_row(
    rows: &[SessionRowModel],
    selection: &SessionSelection,
) -> Option<usize> {
    let target = selection.target.as_ref()?;
    rows.iter().position(|row| row.target == *target)
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
            sort: Some(InfoSortCol::Session),
            label: t("users.session").to_owned(),
            width_px: 100.0,
        },
        Column {
            sort: Some(InfoSortCol::Name),
            label: t("common.user").to_owned(),
            width_px: 160.0,
        },
        Column {
            sort: Some(InfoSortCol::Seat),
            label: t("users.seat").to_owned(),
            width_px: 110.0,
        },
        Column {
            sort: None,
            label: t("users.tty").to_owned(),
            width_px: 110.0,
        },
        Column {
            sort: None,
            label: t("common.type").to_owned(),
            width_px: 100.0,
        },
        Column {
            sort: None,
            label: t("users.since").to_owned(),
            width_px: 200.0,
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
#[component(on_insert = bind_sessions_page)]
pub(crate) struct SessionsPageRoot;

#[derive(Clone, Component, Default)]
pub(crate) struct SessionsBody;

#[derive(Clone, Component, Default)]
pub(crate) struct SessionsStatusLine;

#[derive(Clone, Component, Default)]
pub(crate) struct SessionsRowMarker(pub(crate) usize, pub(crate) SessionId);

#[derive(Clone, Component, Default)]
pub(crate) struct SessionsSortHeader(pub(crate) Option<InfoSortCol>);

#[derive(Resource)]
struct SessionsPageBound;

#[derive(Resource)]
struct SessionsRenderState {
    rendered_revision: Option<u64>,
}

/// Pointer input: a header cell was clicked.
#[derive(Event)]
pub(crate) struct SessionSortClicked(pub(crate) InfoSortCol);

/// Pointer input: a row was clicked; the payload is the VISUAL
/// row index, resolved through the shell's `sorted_session_at` only.
#[derive(Event)]
pub(crate) struct SessionRowClicked(pub(crate) usize);

/// Keyboard input: the selection moved by a row delta.
#[derive(Event)]
pub(crate) struct SessionSelectionMoved(pub(crate) isize);

// ---- render adapters (bsn!) ----

/// Content-region scene for the Sessions page. The body's dynamic content is
/// painted by [`paint_sessions`] — the single render authority for the rows.
pub(crate) fn content(_context: &PageContext<'_>) -> impl Scene + use<> {
    let title = Page::Sessions.title();
    let waiting = t("common.waiting_inventory").to_owned();
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        SessionsPageRoot
        Children [
            ( Text(title) TextRole(Role::Heading) ),
            (
                Text(waiting)
                SessionsStatusLine
                TextRole(Role::Caption)
            ),
            (
                Node {
                    width: percent(100),
                    height: Val::Auto,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                SessionsBody
            ),
        ]
    }
}

fn sessions_body_scene(
    shell: &ShellApp,
    palette: &UiPalette,
    selection: &SessionSelection,
) -> impl Scene + use<> {
    let rows = session_rows(shell);
    let selected = selected_row(&rows, selection);
    let sources = shell.projection().sessions_source.as_deref();
    let notice = source_notice_text(sources);
    let empty = empty_state_text(sources);
    let feedback = shell
        .projection()
        .session_control_feedback
        .as_ref()
        .map(feedback_line_text);
    let children = body_children(&rows, selected, notice, feedback, empty, palette);
    let header = header_scene(shell.sessions_sort, palette);
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
    rows: &[SessionRowModel],
    selected: Option<usize>,
    notice: Option<String>,
    feedback: Option<String>,
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
            children.push(session_row_scene(
                row,
                index,
                selected == Some(index),
                palette,
            ));
        }
    }
    if let Some(text) = feedback {
        children.push(Box::new(caption_line_scene(text)) as Box<dyn Scene>);
    }
    children
}

/// Header row: one caption cell per column; every cell carries the
/// [`SessionsSortHeader`] identity for the pointer adapter.
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
                SessionsSortHeader(sort_target)
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

fn session_row_scene(
    row: &SessionRowModel,
    index: usize,
    selected: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let widths = columns();
    let session = row.session.clone();
    let user = row.user.clone();
    let seat = row.seat.clone();
    let tty = row.tty.clone();
    let kind = row.kind.to_owned();
    let since = row.since.clone();
    let fill = if selected {
        palette.nav_active_bg
    } else {
        Color::NONE
    };
    let target = row.target.clone();
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    let session_width = widths[0].width_px;
    let user_width = widths[1].width_px;
    let seat_width = widths[2].width_px;
    let tty_width = widths[3].width_px;
    let kind_width = widths[4].width_px;
    let since_width = widths[5].width_px;
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
        SessionsRowMarker(index, target)
        Button
        on(on_sessions_row_activated)
        Children [
            ( text_cell_scene(session, session_width, Role::Body) ),
            ( text_cell_scene(user, user_width, Role::Body) ),
            ( text_cell_scene(seat, seat_width, Role::Body) ),
            ( text_cell_scene(tty, tty_width, Role::Body) ),
            ( text_cell_scene(kind, kind_width, Role::Body) ),
            ( text_cell_scene(since, since_width, Role::Body) ),
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
fn paint_sessions(world: &mut World) {
    let palette = world.resource::<WindowPalette>().inner.clone();
    let revision = world
        .non_send::<FrontendTrack>()
        .shell
        .projection()
        .sessions_revision;
    let mut selection = world.resource::<SessionSelection>().clone();
    let (scene, line) = {
        let shell = &world.non_send::<FrontendTrack>().shell;
        let rows = session_rows(shell);
        if let Some(target) = &selection.target
            && !rows.iter().any(|row| &row.target == target)
        {
            // A target that left the inventory deselects honestly.
            selection.target = None;
        }
        (
            sessions_body_scene(shell, &palette, &selection),
            status_line_text(shell, rows.len()),
        )
    };
    world
        .resource_mut::<SessionsRenderState>()
        .rendered_revision = Some(revision);
    world.resource_mut::<SessionSelection>().target = selection.target;
    // A childless container has no `Children` component in this bevy, so the
    // join must be optional — the first paint finds an empty body.
    let mut body_query = world.query_filtered::<(Entity, Option<&Children>), With<SessionsBody>>();
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
    let mut line_query = world.query_filtered::<&mut Text, With<SessionsStatusLine>>();
    if let Ok(mut text) = line_query.single_mut(world) {
        text.0 = line;
    }
}

/// Initial/mount paint: the body container just came to exist, so the first
/// (and every remount) row projection can bind to it.
fn on_sessions_body_added(_added: On<Add, SessionsBody>, mut commands: Commands) {
    commands.queue(paint_sessions);
}

/// Insert hook: bind the page's observers once; the initial paint rides the
/// body-added observer registered below.
fn bind_sessions_page(mut world: DeferredWorld<'_>, _context: HookContext) {
    if world.get_resource_mut::<SessionsPageBound>().is_some() {
        return;
    }
    let mut commands = world.commands();
    commands.insert_resource(SessionsPageBound);
    commands.init_resource::<SessionSelection>();
    commands.insert_resource(SessionsRenderState {
        rendered_revision: None,
    });
    commands.add_observer(on_sessions_projection_folded);
    commands.add_observer(on_sessions_sort_clicked);
    commands.add_observer(on_sessions_row_clicked);
    commands.add_observer(on_sessions_selection_moved);
    // The initial paint rides the body's own insertion: the hook runs while
    // the page scene is still spawning (its children apply later in the same
    // command queue), so painting here would find no body yet. The observer
    // fires exactly when the body entity comes to exist — and again on every
    // route-back remount.
    commands.add_observer(on_sessions_body_added);
}

/// Fold repaint with the idle gate (sessions-domain revision only).
fn on_sessions_projection_folded(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    rendered: Res<SessionsRenderState>,
    mut commands: Commands,
) {
    let revision = track.shell().projection().sessions_revision;
    if rendered.rendered_revision == Some(revision) {
        return;
    }
    commands.queue(paint_sessions);
}

/// Header-sort tail: the shell's existing sort entry owns the decision.
fn on_sessions_sort_clicked(
    click: On<SessionSortClicked>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    track.shell.set_info_sort(InfoTable::Users, click.event().0);
    commands.queue(paint_sessions);
}

/// Row-click tail. The visual row resolves to its target through the shell's
/// single `sorted_session_at` translation.
fn on_sessions_row_clicked(
    click: On<SessionRowClicked>,
    track: ShellTrack,
    mut selection: ResMut<SessionSelection>,
    mut commands: Commands,
) {
    let Some(session) = track.shell().sorted_session_at(click.event().0) else {
        return;
    };
    selection.target = Some(session.id.clone());
    commands.queue(paint_sessions);
}

/// Keyboard-selection tail: sort-stable target id, clamped cursor.
fn on_sessions_selection_moved(
    movement: On<SessionSelectionMoved>,
    track: ShellTrack,
    mut selection: ResMut<SessionSelection>,
    mut commands: Commands,
) {
    let rows = session_rows(track.shell());
    let current = selected_row(&rows, &selection);
    let Some(next) = moved_row(rows.len(), current, movement.event().0) else {
        return;
    };
    let Some(session) = track.shell().sorted_session_at(next) else {
        return;
    };
    selection.target = Some(session.id.clone());
    commands.queue(paint_sessions);
}

#[cfg(test)]
#[path = "../../tests/headless/pages/sessions.rs"]
mod tests;

fn on_sessions_row_activated(
    activate: On<Activate>,
    markers: Query<&SessionsRowMarker>,
    mut commands: Commands,
) {
    if let Ok(marker) = markers.get(activate.event().entity) {
        commands.trigger(SessionRowClicked(marker.0));
    }
}
