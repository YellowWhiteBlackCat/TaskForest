//! Applications consume cached shell rows and one measured table viewport.
//! Native tree inspection renders the shared hierarchy in the owned bounded modal.
//! Query, selection, batch gates and typed unavailable cells retain shell authority.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::{EntityEvent, Event};
use bevy::ecs::hierarchy::Children;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::{Observer, On};
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut, SystemParam};
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{BackgroundColor, ComputedNode, FlexDirection, Node, Val, percent, px};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Activate;
use taskforest_core::core::process::ProcessLiveKey;
use taskmanager_application::{AppAction, AppPage};

use taskmanager_shell::ShellApp;
use zhuge_ui_core::row_page::RowPage;

use crate::app::{FrontendTrack, Page, PageContext, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::palette::space_8;
use crate::widgets::table::resize::{ColumnResizeSession, ColumnWidthOverrides};
use crate::widgets::table::{header_scene_with_widths, visible_columns};
use crate::window::{Role, TextRole, WindowPalette};

pub(crate) mod affinity;
pub(crate) mod card_lane;
pub(crate) mod columns_modal;
pub(crate) mod details;
pub(crate) mod input;
pub(crate) mod menu;
pub(crate) mod projection;
pub(crate) mod properties_modal;
mod rebuild;
pub(crate) mod toolbar;

pub(crate) use projection::*;
pub(crate) use rebuild::{live_widths, rebuild_table, rows_root_scene};

pub(crate) mod layout;
use card_lane::{CardFlowBudget, ProcessViewMode};
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ProcessPageRoot;
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ProcessOptionalChrome;

use crate::scene_attach::attach_child;
use columns_modal::{ProcessColumnsModalState, ProcessHiddenColumns};

// ---- pure view model ----------------------------------------------------

fn selected_identity(shell: &ShellApp) -> Option<ProcessLiveKey> {
    let process = shell.visible_process_at(shell.selected)?;
    ProcessLiveKey::from_process(process)
}

// ---- page state, seams, and markers --------------------------------------

/// Scroll intent + viewport capacity for the rows root. `top` is the intent
/// (the row the caller wants first); every rebuild stores the clamped window
/// start back, so the stored intent can never drift past the last full page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Resource)]
pub(crate) struct ProcessScrollState {
    pub(crate) viewport_rows: usize,
    pub(crate) top: usize,
}

/// Keyboard-equivalent selection move on the rows root (arrow up/down
/// semantics; the shell owns clamping through `move_selection`).
#[derive(Clone, Debug, EntityEvent)]
pub(crate) struct ProcessSelectStep {
    pub(crate) entity: Entity,
    pub(crate) delta: isize,
}

/// Click-equivalent selection on one visible row (bounded by the shell's
/// `select_row`; an out-of-range row is rejected, never clamped).
#[derive(Clone, Debug, EntityEvent)]
pub(crate) struct ProcessSelectRow {
    pub(crate) entity: Entity,
    pub(crate) row: usize,
}

/// Multi-selection toggle on one visible row (Space / multi-select click).
#[derive(Clone, Debug, EntityEvent)]
pub(crate) struct ProcessToggleRowSelection {
    pub(crate) entity: Entity,
    pub(crate) row: usize,
}

/// Wheel-equivalent scroll intent in rows (sign-carrying; clamped to the
/// last full window on rebuild).
#[derive(Clone, Debug, EntityEvent)]
pub(crate) struct ProcessScrollIntent {
    pub(crate) entity: Entity,
    pub(crate) rows: isize,
}

#[derive(Component, Clone, Default)]
pub(crate) struct ProcessTableContainer;

#[derive(Component, Clone, Default)]
pub(crate) struct ProcessDetailsContainer;

/// The outer card-lane wrapper owns the mode's Display. Its inner lane scene
/// owns the optical dashboard and cards, so the mode query and the table query
/// can never touch the same entity.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessCardLaneRoot;

type ResponsiveTables<'w, 's> = Query<
    'w,
    's,
    &'static mut Node,
    (
        With<ProcessTableContainer>,
        bevy::ecs::query::Without<ProcessDetailsContainer>,
        bevy::ecs::query::Without<ProcessCardLaneRoot>,
    ),
>;
type ResponsiveLanes<'w, 's> = Query<
    'w,
    's,
    &'static mut Node,
    (
        With<ProcessCardLaneRoot>,
        bevy::ecs::query::Without<ProcessTableContainer>,
        bevy::ecs::query::Without<ProcessDetailsContainer>,
    ),
>;
type ResponsiveDetails<'w, 's> = Query<
    'w,
    's,
    &'static mut Node,
    (
        With<ProcessDetailsContainer>,
        bevy::ecs::query::Without<ProcessTableContainer>,
        bevy::ecs::query::Without<ProcessCardLaneRoot>,
    ),
>;

pub(crate) fn sync_processes_responsive_layout(
    pages: Query<&ComputedNode, With<ProcessPageRoot>>,
    mode: Option<Res<ProcessViewMode>>,
    card_budget: Option<Res<CardFlowBudget>>,
    mut tables: ResponsiveTables,
    mut lanes: ResponsiveLanes,
    mut details: ResponsiveDetails,
) {
    let _page_width = pages
        .single()
        .map(|page| page.size().x * page.inverse_scale_factor())
        .unwrap_or(0.0);
    let fallback = ProcessViewMode::Dense;
    let view_mode = mode.as_deref().copied().unwrap_or(fallback);
    let effective_mode = card_budget
        .as_deref()
        .copied()
        .unwrap_or_default()
        .effective_mode(view_mode);
    let (table_display, lane_visible) = match effective_mode {
        ProcessViewMode::Cards => (bevy::ui::Display::None, true),
        ProcessViewMode::Dense => (bevy::ui::Display::Flex, false),
    };
    for mut node in &mut tables {
        if node.width != percent(100) || node.display != table_display {
            node.width = percent(100);
            node.display = table_display;
        }
    }
    for mut node in &mut lanes {
        let next = if lane_visible {
            bevy::ui::Display::Flex
        } else {
            bevy::ui::Display::None
        };
        if node.display != next {
            node.display = next;
        }
    }
    for mut node in &mut details {
        if node.display != bevy::ui::Display::None {
            node.display = bevy::ui::Display::None;
        }
    }
}

/// Search-box commit: replace the shell query with `text` (sanitized and
/// capped by the shell's bulk `push_search_text` contract; the cursor resets
/// exactly like per-character typing).
#[derive(Clone, Debug, EntityEvent)]
pub(crate) struct ProcessQueryCommit {
    pub(crate) entity: Entity,
    pub(crate) text: String,
}

/// Published whenever the selected row's identity actually changes. The
/// details panel reads the shell projection for the identity; this event is
/// only its typed refresh signal, so it cannot become a second selection
/// authority.
#[derive(Clone, Debug, Event)]
pub(crate) struct ProcessSelectionChanged;

/// The virtual scroll surface: exactly one node under the mounted page.
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessRowsRoot;

/// One materialized row wrapper: carries its visible-set index.
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessRowLink(pub(crate) usize);

/// Sweep marker for everything a rebuild replaces (row wrappers and the
/// empty-state node). The rows root deliberately does NOT carry it.
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessTableArtifact;

/// The one status line the rebuilds rewrite.
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessCountLine;

/// The search display node (the shell query, kept in step by the input
/// submodule's re-render observer).
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessSearchInput;

pub(crate) fn on_search_input_activated(
    _activate: On<Activate>,
    mut track: NonSendMut<FrontendTrack>,
    mut text_state: Option<ResMut<crate::input::TextInputState>>,
    mut commands: Commands,
) {
    track.shell.open_search();
    if let Some(ref mut state) = text_state {
        state.edit.set_caret_at_end(&track.shell.query);
    }
    commands.trigger(crate::input::ShellInteractionApplied);
}

/// Marker for the table header slot node.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessTableHeaderSlot;

/// Marker for the mounted table header artifact scene.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessTableHeaderArtifact;

/// Shared observer parameters for the table surface, bundled to keep every
/// observer under the argument budget.
#[derive(SystemParam)]
pub(crate) struct TableSurface<'w, 's> {
    palette: Res<'w, WindowPalette>,
    scroll: ResMut<'w, ProcessScrollState>,
    roots: Query<'w, 's, Entity, With<ProcessRowsRoot>>,
    artifacts: Query<'w, 's, Entity, With<ProcessTableArtifact>>,
    header_slots: Query<'w, 's, Entity, With<ProcessTableHeaderSlot>>,
    headers: Query<'w, 's, Entity, With<ProcessTableHeaderArtifact>>,
    count: Query<'w, 's, &'static mut Text, With<ProcessCountLine>>,
    hidden_cols: Option<Res<'w, ProcessHiddenColumns>>,
    widths: Option<Res<'w, ColumnWidthOverrides>>,
    resize: Option<Res<'w, ColumnResizeSession>>,
    /// The presentation mode. Optional so a composition that never mounts the
    /// card lane still assembles the dense table without a second resource.
    mode: Option<Res<'w, ProcessViewMode>>,
    /// The measured card-flow budget, owned by the card lane's own system.
    card_budget: Option<Res<'w, CardFlowBudget>>,
    /// The card lane's bounded flow root: the visible home of the atomic
    /// cards when the card presentation is active.
    card_flows: Query<'w, 's, Entity, With<card_lane::ProcessCardFlow>>,
}

// ---- observers -----------------------------------------------------------

/// The data-context half of routing: the bevy `Route` stays the frontend's
/// own navigation authority (crate::app), but the shell's selection reducers
/// (`move_selection`, `select_row`, the anchor collapse) are row-count-
/// parameterized by the shared page. Without this application a fresh shell
/// still sits on the default `Performance` page and every cursor move would
/// clamp against the wrong row space — the exact `SelectPage(Applications)`
/// application the TUI performs when its Applications page opens. Applied by
/// each shell-mutating seam (not at mount) so the page never assumes
/// composition ordering between the scene spawn and the shell track.
fn ensure_applications_row_context(shell: &mut ShellApp) {
    if shell.page() != AppPage::Applications {
        let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    }
}

/// Mount bootstrap: resource-ize the viewport/scroll intent and spawn the
/// page's global [`ShellProjectionFolded`] observer as a child of the rows
/// root. Childing ties its lifetime to the page: the shared route observer
/// despawns the page content recursively, which deregisters the observer —
/// no zombie refresh path survives a route change. No shell access here, so
/// the bootstrap cannot race the track's insertion in any composition.
///
/// The palette is optional because the shared page census spawns every page
/// scene into a bare world (scene plumbing only) to assert it assembles; a
/// page without a resolved palette cannot initialize its live surface, so it
/// stays static there instead of failing the observer.
fn bootstrap_processes_page(
    trigger: On<Add<ProcessRowsRoot>>,
    palette: Option<Res<WindowPalette>>,
    mut commands: Commands,
) {
    let root = trigger.event().entity;
    commands.init_resource::<ColumnWidthOverrides>();
    commands.init_resource::<ColumnResizeSession>();
    commands.init_resource::<crate::input::TextInputState>();
    commands.init_resource::<ProcessHiddenColumns>();
    commands.init_resource::<ProcessColumnsModalState>();
    let Some(_palette) = palette else {
        return;
    };
    let viewport_rows = 1;
    commands.insert_resource(ProcessScrollState {
        viewport_rows,
        top: 0,
    });
    input::bootstrap(root, &mut commands);
    let fold_observer = commands.spawn(Observer::new(on_projection_folded)).id();
    attach_child(&mut commands, root, fold_observer);
}

/// Data-refresh observer: one non-empty drain fold re-reads the shell
/// projection and rebuilds the visible window. No fold → no work: this runs
/// only on the trigger, so idle frames redraw nothing.
fn on_projection_folded(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    mut surface: TableSurface,
    mut commands: Commands,
) {
    let Ok(root) = surface.roots.single() else {
        return; // the page is not mounted; the observer dies with it shortly
    };
    rebuild_table(&mut commands, root, track.shell(), &mut surface);
}

/// Arrow-key seam: the shell cursor reducer owns bounds and application-state
/// sync; the window follows the cursor (TUI parity), and an identity change
/// publishes the details-panel seam event.
fn on_select_step(
    trigger: On<ProcessSelectStep>,
    mut track: NonSendMut<FrontendTrack>,
    mut surface: TableSurface,
    mut commands: Commands,
) {
    let shell = &mut track.shell;
    ensure_applications_row_context(shell);
    let before = selected_identity(shell);
    shell.move_selection(trigger.event().delta);
    let after = selected_identity(shell);
    if before == after {
        return; // empty table or clamped at the edge: nothing observable moved
    }
    let total = shell.visible_process_count();
    surface.scroll.top =
        RowPage::reveal_centered(total, surface.scroll.viewport_rows, shell.selected)
            .page()
            .first();
    let Ok(root) = surface.roots.single() else {
        return;
    };
    rebuild_table(&mut commands, root, shell, &mut surface);
    commands.trigger(ProcessSelectionChanged);
}

/// Click seam: `select_row` is bounded — a stale row index is rejected
/// outright rather than clamped onto a neighboring process.
fn on_select_row(
    trigger: On<ProcessSelectRow>,
    mut track: NonSendMut<FrontendTrack>,
    mut surface: TableSurface,
    mut commands: Commands,
) {
    let shell = &mut track.shell;
    ensure_applications_row_context(shell);
    let before = selected_identity(shell);
    if !shell.select_row(trigger.event().row) {
        return;
    }
    let after = selected_identity(shell);
    let total = shell.visible_process_count();
    surface.scroll.top =
        RowPage::reveal_centered(total, surface.scroll.viewport_rows, shell.selected)
            .page()
            .first();
    let Ok(root) = surface.roots.single() else {
        return;
    };
    rebuild_table(&mut commands, root, shell, &mut surface);
    if before != after {
        commands.trigger(ProcessSelectionChanged);
    }
}

fn on_toggle_row_selection(
    trigger: On<ProcessToggleRowSelection>,
    mut track: NonSendMut<FrontendTrack>,
    mut surface: TableSurface,
    mut commands: Commands,
) {
    let shell = &mut track.shell;
    ensure_applications_row_context(shell);
    let before = selected_identity(shell);
    if !shell.toggle_row_selection(trigger.event().row) {
        return;
    }
    let after = selected_identity(shell);
    let total = shell.visible_process_count();
    surface.scroll.top =
        RowPage::reveal_centered(total, surface.scroll.viewport_rows, shell.selected)
            .page()
            .first();
    let Ok(root) = surface.roots.single() else {
        return;
    };
    rebuild_table(&mut commands, root, shell, &mut surface);
    if before != after {
        commands.trigger(ProcessSelectionChanged);
    }
}

/// Wheel seam: shift the intent, rebuild. [`rebuild_table`] stores the
/// clamped window start back into the intent — pure window math from the
/// widgets layer, never an out-of-range slice.
fn on_scroll_intent(
    trigger: On<ProcessScrollIntent>,
    track: ShellTrack,
    mut surface: TableSurface,
    mut commands: Commands,
) {
    let Ok(root) = surface.roots.single() else {
        return;
    };
    surface.scroll.top = surface
        .scroll
        .top
        .saturating_add_signed(trigger.event().rows);
    rebuild_table(&mut commands, root, track.shell(), &mut surface);
}

/// Search-commit seam: replace the query through the shell's public bulk
/// path. Clearing replays `pop_search_char` (each pop resets the cursor, so an
/// emptied box lands on row 0 exactly like backspacing); non-empty text goes
/// through `push_search_text`, which sanitizes control bytes, collapses line
/// breaks, and caps at [`SEARCH_QUERY_MAX`].
fn on_query_commit(
    trigger: On<ProcessQueryCommit>,
    mut track: NonSendMut<FrontendTrack>,
    mut surface: TableSurface,
    mut text_state: Option<ResMut<crate::input::TextInputState>>,
    mut commands: Commands,
) {
    let text = trigger.event().text.clone();
    let shell = &mut track.shell;
    ensure_applications_row_context(shell);
    crate::input::commit_query_to_shell(shell, &text);
    if let Some(ref mut state) = text_state {
        state.edit.set_caret_at_end(&shell.query);
    }
    surface.scroll.top = 0; // the cursor reset puts row 0 back in view
    let Ok(root) = surface.roots.single() else {
        return;
    };
    rebuild_table(&mut commands, root, shell, &mut surface);
    commands.trigger(ProcessSelectionChanged);
}

/// Content-region scene for the Processes page.
pub(crate) fn content(context: &PageContext<'_>) -> impl Scene + use<> {
    let palette = context.palette;
    let title = Page::Processes.title();
    let projection = rows_projection(context.shell, 1, 0);
    let count = count_line_text_for_shell(context.shell, projection.total, &context.shell.query);
    let columns = visible_columns(columns_modal::DEFAULT_HIDDEN_COLUMNS);
    let widths = live_widths(&columns, None);
    let header = header_scene_with_widths(
        &columns,
        &widths,
        Some(sort_projection(context.shell.process_sort)),
        palette,
    );
    let header_slot = bsn! {
        Node {
            width: percent(100), flex_shrink:0.0,
        }
        ProcessTableHeaderSlot
        Children [

                Node {
                    width: percent(100),
                }
                ProcessTableHeaderArtifact
                Children [
                     @{ header }
                ]

        ]
    };
    let rows_root = rows_root_scene(
        &projection,
        &columns,
        &widths,
        palette,
        &context.shell.query,
    );
    let toolbar = toolbar::toolbar_scene(context);
    let card_lane = card_lane::card_lane_scene(context);
    let table = bsn! {
        Node {
            width: percent(100),
            flex_grow: 1.0,
            flex_basis: px(0.0),
            min_width: px(0.0),
            height: percent(100), min_height:px(0.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            display: bevy::ui::Display::None,
        }
        ProcessTableContainer
        Children [
             @{ header_slot } --
             @{ rows_root }
        ]
    };
    let details = details::panel_scene(context);
    let card_lane = bsn! {
        Node {
            width: percent(100),
            min_width: px(0.0),
            min_height: px(0.0),
            flex_grow: 1.0,
            flex_basis: px(0.0),
            display: bevy::ui::Display::Flex,
            overflow: bevy::ui::Overflow::clip(),
        }
        ProcessCardLaneRoot
        Children [ @{ card_lane } ]
    };
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: Val::Px(space_8()),
        }
        BackgroundColor({ palette.canvas }) ProcessPageRoot
        Children [
             Text(title) TextRole(Role::Heading) ProcessOptionalChrome --
             @{ toolbar } --

                Text(count)
                ProcessCountLine ProcessOptionalChrome
                TextRole(Role::Caption)
            --

                Node {
                    width: percent(100),
                    min_height: px(0.0), flex_grow: 1.0, flex_basis: px(0.0),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(space_8()),
                }
                Children [
                     @{ card_lane } --
                     @{ table } --
                     @{ details }
                ]

        ]
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/processes.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/process_fixture_support.rs"]
mod process_fixture_support;

#[cfg(test)]
#[path = "../../tests/headless/pages/processes_table_parity.rs"]
mod table_parity_tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/process_card_lane.rs"]
mod card_lane_tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/process_affinity.rs"]
mod process_affinity_tests;
