//! Processes page — the M1 process table (the first feature page).
//!
//! **Composition model** (the page-proxy contract in [`crate::pages`]): the
//! static tree (title/status line, search input, contract header, rows root,
//! and the selected-process details panel) is one declarative `bsn!` assembly
//! seeded from the shell snapshot at mount; everything dynamic is bound
//! through observers — never polling:
//!
//! - **Data refresh**: [`bootstrap_processes_page`] runs when the rows root
//!   lands (an `on(On<Add, ProcessRowsRoot>)` entity observer) and spawns the
//!   page's global [`ShellProjectionFolded`] observer as a child of the root,
//!   so it lives and dies with the mounted page (a route change despawns the
//!   page recursively). Idle frames — no folded batches — redraw nothing.
//! - **Input seams**: keyboard/wheel/click bridges are not reachable from a
//!   page module in the M1 composition (bevy input messages need polling
//!   systems and pointer picking is not in this crate's feature closure),
//!   so the page exposes typed `EntityEvent` seams on the rows root —
//!   [`ProcessSelectStep`], [`ProcessSelectRow`], [`ProcessScrollIntent`],
//!   [`ProcessQueryCommit`] — each with an `on()` observer that reduces
//!   through the SAME public shell reducers the TUI keyboard path uses
//!   (`move_selection`, `select_row`, `push_search_text`). The W4 input wiring
//!   triggers these events; the semantics are final and headless-tested here.
//! - **Selection identity**: every accepted selection change publishes
//!   [`ProcessSelectionChanged`]. The sibling [`details`] component consumes
//!   it, reuses the shared process-details VM, and requests matching frozen
//!   process insights through the app-host client.
//!
//! Semantics align with the TUI Applications table
//! (`taskmanager-tui/src/ui/process_table.rs`): the visible set, sort, and
//! cursor are shell-owned (query + status filter + sort memoized in
//! `ShellApp::visible_processes`); the header marks the active sort; an
//! unavailable scalar renders `—`, never zero. M1 differences are deliberate
//! and declared in the capability note: no grouped tree, no per-row sparkline,
//! and no multi-select batch verbs yet. StartTime cells render `—` until a
//! local-time observation reaches this frontend; the selected-process details
//! panel is the first completed F13 slice.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::{EntityEvent, Event};
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::{Observer, On};
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut, SystemParam};
use bevy::scene::{CommandsSceneExt, Scene, bsn, on, template_value};
use bevy::text::EditableText;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, Node, Val, percent, px,
};
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, AppPage, LocalTimeRulesObservation, ProcessItem};
use taskmanager_shell::app::search_input::SEARCH_QUERY_MAX;
use taskmanager_shell::presentation::{MISSING_VALUE, bytes, optional_nice, start_clock_local};
use taskmanager_shell::{ShellApp, SortCol, SortDir};
use taskmanager_ui_contract::ProcessColumnSpec;

use crate::app::{FrontendTrack, Page, PageContext, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, space_8, space_24};
use crate::widgets::table::{
    RowWindow, SortProjection, header_scene, row_scene, row_window, rows_in_viewport,
    visible_columns,
};
use crate::window::{Role, TextRole, WindowPalette};

pub(crate) mod details;

/// Height of the scrollable rows area in px. The bevy_ui flexbox cannot report
/// a computed node height to an observer without a layout system, so M1 fixes
/// the virtual viewport at design size (the 780 px window's content slot minus
/// title/search/header chrome). The value feeds [`rows_in_viewport`] together
/// with the palette's control height; both live in [`ProcessScrollState`] so
/// tests and later milestones can resize the window dynamically.
const TABLE_VIEWPORT_HEIGHT_PX: f32 = 512.0;

// ---- pure view model ----------------------------------------------------

/// Map the shell's sort slot onto the ui-contract column token. `PSS` has no
/// contract column, so sorting by it shows no header marker — honest absence
/// rather than a fabricated nearest-column marker.
fn contract_token(column: SortCol) -> Option<&'static str> {
    match column {
        SortCol::Pid => Some("PID"),
        SortCol::Name => Some("Name"),
        SortCol::Cpu => Some("CPU"),
        SortCol::Memory => Some("Memory"),
        SortCol::Pss => None,
        SortCol::Swap => Some("Swap"),
        SortCol::User => Some("User"),
        SortCol::State => Some("Status"),
        SortCol::Threads => Some("Threads"),
        SortCol::CpuTime => Some("CPUTime"),
        SortCol::DiskRead => Some("DiskRead"),
        SortCol::DiskWrite => Some("DiskWrite"),
        SortCol::StartTime => Some("StartTime"),
        SortCol::Fds => Some("FDs"),
        SortCol::Nice => Some("Nice"),
    }
}

/// The shell sort as the widgets' header projection input.
pub(crate) fn sort_projection(sort: (SortCol, SortDir)) -> Option<SortProjection> {
    let (column, direction) = sort;
    contract_token(column).map(|column| SortProjection {
        column,
        descending: direction == SortDir::Desc,
    })
}

/// One cell's text for a contract column id. Unavailable scalars render the
/// shared `MISSING_VALUE` dash, exactly like the TUI cells — a provider
/// failure is never shown as a zero. The Start column uses the unsupported
/// local-time observation, so it renders `—` until this frontend observes a
/// timezone (the same output the TUI shows on an unsupported host).
fn cell_text(process: &ProcessItem, column: &str) -> String {
    match column {
        "Name" => process.name.clone(),
        "User" => process.current_user().unwrap_or_default(),
        "PID" => process.pid.to_string(),
        "Threads" => process
            .current_threads()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| value.to_string()),
        "StartTime" => start_clock_local(
            process.current_start_time_secs(),
            &LocalTimeRulesObservation::unsupported(0),
        ),
        "Status" => process.status.clone(),
        "CPU" => process
            .current_cpu_percentage()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| format!("{value:.1}%")),
        "Memory" => process
            .current_memory_bytes()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "Swap" => process
            .current_swap_bytes()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "DiskRead" => process
            .current_disk_read_bytes_per_sec()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "DiskWrite" => process
            .current_disk_write_bytes_per_sec()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "CPUTime" => process
            .current_cpu_time_secs()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| format!("{value:.1}s")),
        "FDs" => process
            .current_fds()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| value.to_string()),
        "Nice" => optional_nice(process.current_nice()),
        _ => MISSING_VALUE.to_owned(),
    }
}

/// One row's cell vector over the contract columns (contract order, widths,
/// and numeric alignment come from the shared vocabulary, never a local copy).
/// The selected row prefixes the Name cell with the TUI's `›` cursor marker.
fn row_cells(process: &ProcessItem, columns: &[&ProcessColumnSpec], selected: bool) -> Vec<String> {
    columns
        .iter()
        .map(|column| {
            let text = cell_text(process, column.id);
            if selected && column.id == "Name" {
                format!("› {text}")
            } else {
                text
            }
        })
        .collect()
}

/// The rendered rows of one virtual window: pure over (shell, viewport,
/// scroll intent), so every observer and the headless tests share one
/// materialization rule.
pub(crate) struct ProcessRowView {
    /// Visible-set index of the row (the shell cursor's coordinate space).
    pub(crate) index: usize,
    pub(crate) cells: Vec<String>,
    pub(crate) selected: bool,
}

/// Full window projection: the total behind the filter plus the visible slice.
pub(crate) struct ProcessRowsProjection {
    pub(crate) total: usize,
    pub(crate) window: RowWindow,
    pub(crate) rows: Vec<ProcessRowView>,
}

/// Build the window projection. The selected flag clamps the shell cursor to
/// the row space first (the same defensive clamp as the TUI `table_window`),
/// so a stale cursor after a shrinking fold can never index out of range or
/// silently select nothing.
pub(crate) fn rows_projection(
    shell: &ShellApp,
    viewport_rows: usize,
    scroll_top: usize,
) -> ProcessRowsProjection {
    let visible = shell.visible_processes();
    let total = visible.len();
    let window = row_window(total, viewport_rows, scroll_top);
    let selected = total.checked_sub(1).map(|last| shell.selected.min(last));
    let columns = visible_columns(&[]);
    let rows = visible[window.first..window.last]
        .iter()
        .enumerate()
        .map(|(offset, process)| {
            let index = window.first + offset;
            ProcessRowView {
                index,
                cells: row_cells(process, &columns, Some(index) == selected),
                selected: Some(index) == selected,
            }
        })
        .collect();
    ProcessRowsProjection {
        total,
        window,
        rows,
    }
}

/// Scroll top that keeps `selected` centered in the window — the TUI
/// `table_window` follow formula, verbatim: half a viewport above the cursor,
/// pinned to the last full page, never past either end.
pub(crate) fn centered_scroll_top(total: usize, viewport_rows: usize, selected: usize) -> usize {
    if total == 0 || viewport_rows == 0 {
        return 0;
    }
    let visible = viewport_rows.min(total);
    let selected = selected.min(total - 1);
    selected
        .saturating_sub(visible / 2)
        .min(total.saturating_sub(visible))
}

/// The status line under the search box: the shared running-count copy, plus
/// the shared match-counter copy while a query is active (the same catalog
/// strings the TUI panels use — never a frontend-local word, so the line
/// localizes with the rest of the surface).
pub(crate) fn count_line_text(visible: usize, query: &str) -> String {
    let base = t("proc.processes_running_subtitle").replacen("{count}", &visible.to_string(), 1);
    if query.trim().is_empty() {
        return base;
    }
    let key = if visible == 1 {
        "tui.search_matches_one"
    } else {
        "tui.search_matches_many"
    };
    format!(
        "{base}{}",
        t(key).replacen("{count}", &visible.to_string(), 1)
    )
}

/// Honest empty-table copy: a quiet platform (no processes reported yet) is a
/// different state from an over-narrow query — shared `empty.*` strings.
pub(crate) fn empty_state_text(query: &str) -> String {
    if query.trim().is_empty() {
        t("empty.no_processes_reported").to_owned()
    } else {
        t("empty.no_processes_match_query").to_owned()
    }
}

/// The selected row's identity for the details-panel seam.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessRowIdentity {
    pub(crate) pid: u32,
    pub(crate) name: String,
}

fn selected_identity(shell: &ShellApp) -> Option<ProcessRowIdentity> {
    let process = shell.visible_process_at(shell.selected)?;
    Some(ProcessRowIdentity {
        pid: process.pid,
        name: process.name.clone(),
    })
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

/// Wheel-equivalent scroll intent in rows (sign-carrying; clamped to the
/// last full window on rebuild).
#[derive(Clone, Debug, EntityEvent)]
pub(crate) struct ProcessScrollIntent {
    pub(crate) entity: Entity,
    pub(crate) rows: isize,
}

/// Search-box commit: replace the shell query with `text` (sanitized and
/// capped by the shell's bulk `push_search_text` contract; the cursor resets
/// exactly like per-character typing).
#[derive(Clone, Debug, EntityEvent)]
pub(crate) struct ProcessQueryCommit {
    pub(crate) entity: Entity,
    pub(crate) text: String,
}

/// Published whenever the selected row's identity actually changes — the seam
/// the later details panel observes. `None` means the table emptied or the
/// selection collapsed; it is a real transition, not a missing value.
#[derive(Clone, Debug, Event)]
pub(crate) struct ProcessSelectionChanged(
    /// Grammar-complete today (the details panel lands with a later
    /// milestone); the headless tests read the payload to prove what the page
    /// publishes — the same shape as `RouteChanged`'s reserved payload.
    #[allow(dead_code)]
    pub(crate) Option<ProcessRowIdentity>,
);

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

/// The search input node (focus target for the W4 input wiring).
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessSearchInput;

/// Shared observer parameters for the table surface, bundled to keep every
/// observer under the argument budget.
#[derive(SystemParam)]
pub(crate) struct TableSurface<'w, 's> {
    palette: Res<'w, WindowPalette>,
    scroll: ResMut<'w, ProcessScrollState>,
    roots: Query<'w, 's, Entity, With<ProcessRowsRoot>>,
    artifacts: Query<'w, 's, Entity, With<ProcessTableArtifact>>,
    count: Query<'w, 's, &'static mut Text, With<ProcessCountLine>>,
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
    trigger: On<Add, ProcessRowsRoot>,
    palette: Option<Res<WindowPalette>>,
    mut commands: Commands,
) {
    let Some(palette) = palette else {
        return;
    };
    let viewport_rows = rows_in_viewport(TABLE_VIEWPORT_HEIGHT_PX, palette.inner.control_height_px);
    commands.insert_resource(ProcessScrollState {
        viewport_rows,
        top: 0,
    });
    let fold_observer = commands.spawn(Observer::new(on_projection_folded)).id();
    commands
        .entity(trigger.event().entity)
        .add_one_related::<ChildOf>(fold_observer);
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
    surface.scroll.top = centered_scroll_top(total, surface.scroll.viewport_rows, shell.selected);
    let Ok(root) = surface.roots.single() else {
        return;
    };
    rebuild_table(&mut commands, root, shell, &mut surface);
    commands.trigger(ProcessSelectionChanged(after));
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
    surface.scroll.top = centered_scroll_top(total, surface.scroll.viewport_rows, shell.selected);
    let Ok(root) = surface.roots.single() else {
        return;
    };
    rebuild_table(&mut commands, root, shell, &mut surface);
    if before != after {
        commands.trigger(ProcessSelectionChanged(after));
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
    mut commands: Commands,
) {
    let text = trigger.event().text.clone();
    let shell = &mut track.shell;
    ensure_applications_row_context(shell);
    while !shell.query.is_empty() {
        shell.pop_search_char();
    }
    if !text.is_empty() {
        shell.push_search_text(&text);
    }
    surface.scroll.top = 0; // the cursor reset puts row 0 back in view
    let Ok(root) = surface.roots.single() else {
        return;
    };
    rebuild_table(&mut commands, root, shell, &mut surface);
    commands.trigger(ProcessSelectionChanged(selected_identity(shell)));
}

// ---- render --------------------------------------------------------------

/// Selected-row fill: the palette's elevated card surface (the same token the
/// active nav item uses — one "selected" surface per theme).
fn row_fill(selected: bool, palette: &UiPalette) -> bevy::color::Color {
    if selected {
        palette.nav_active_bg
    } else {
        bevy::color::Color::NONE
    }
}

/// One row wrapper: the virtual window's row height and selection fill, with
/// the shared widgets row scene inside (cell widths/alignment stay contract-
/// owned). The wrapper carries the link index so tests and future pointer
/// bridges can map a node to its visible-set row.
fn row_wrapper_scene(
    row: &ProcessRowView,
    columns: &[&ProcessColumnSpec],
    row_height: f32,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let fill = row_fill(row.selected, palette);
    let index = row.index;
    let cells = row.cells.clone();
    let inner = row_scene(&cells, columns);
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: px(row_height),
        }
        BackgroundColor({ fill })
        ProcessRowLink({ index })
        ProcessTableArtifact
        Children [
            ( { inner } ),
        ]
    })
}

/// The empty window's honest message node.
fn empty_state_scene(message: String) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
        }
        ProcessTableArtifact
        Children [
            ( Text({ message }) TextRole(Role::Body) ),
        ]
    })
}

/// Materialize one window: the row wrappers, or the empty-state node when the
/// filtered set is empty. One rule for the mount-time static rows and every
/// observer rebuild.
fn window_scenes(
    projection: &ProcessRowsProjection,
    palette: &UiPalette,
    query: &str,
) -> Vec<Box<dyn Scene>> {
    if projection.total == 0 {
        return vec![empty_state_scene(empty_state_text(query))];
    }
    let columns = visible_columns(&[]);
    let row_height = palette.control_height_px;
    projection
        .rows
        .iter()
        .map(|row| row_wrapper_scene(row, &columns, row_height, palette))
        .collect()
}

/// Despawn the previous window's artifacts, spawn the new one under the rows
/// root, rewrite the count line, and normalize the stored scroll intent to
/// the clamped window start. Shared by every observer.
fn rebuild_table(
    commands: &mut Commands,
    root: Entity,
    shell: &ShellApp,
    surface: &mut TableSurface,
) {
    let projection = rows_projection(shell, surface.scroll.viewport_rows, surface.scroll.top);
    surface.scroll.top = projection.window.first;
    for artifact in surface.artifacts.iter() {
        commands.entity(artifact).despawn();
    }
    for scene in window_scenes(&projection, &surface.palette.inner, &shell.query) {
        let child = commands.spawn_scene(scene).id();
        commands.entity(root).add_one_related::<ChildOf>(child);
    }
    if let Ok(mut line) = surface.count.single_mut() {
        line.0 = count_line_text(projection.total, &shell.query);
    }
}

/// The search box: an official `EditableText` (the bevy_text half of the
/// bevy_ui_widgets base) capped at the shell's search-query limit. Typing and
/// focus plumbing belong to the widget package (registered with
/// `DefaultPlugins` in the windowed composition); this page owns the box
/// chrome and the commit semantics. `template_value` carries the pre-built
/// component because `EditableText`'s editor state is not a bsn! template.
fn search_input_scene(palette: &UiPalette) -> impl Scene + use<> {
    let input = EditableText {
        max_characters: Some(SEARCH_QUERY_MAX),
        ..EditableText::default()
    };
    let width = space_24() * 12.0;
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    bsn! {
        Node {
            width: px(width),
            height: px(height),
            align_items: AlignItems::Center,
            padding: Val::Px(space_8()),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.panel_fill })
        ProcessSearchInput
        template_value(input)
        TextRole(Role::Body)
    }
}

/// The rows root scene: fixed-height virtual viewport, the five `on()` seam
/// observers (bootstrap first, so it is registered before the marker's own
/// `Add` trigger dispatches), and the mount-time static window inside.
fn rows_root_scene(
    projection: &ProcessRowsProjection,
    palette: &UiPalette,
    query: &str,
) -> impl Scene + use<> {
    let rows = window_scenes(projection, palette, query);
    bsn! {
        Node {
            width: percent(100),
            height: px(TABLE_VIEWPORT_HEIGHT_PX),
            flex_direction: FlexDirection::Column,
        }
        on(bootstrap_processes_page)
        on(on_select_step)
        on(on_select_row)
        on(on_scroll_intent)
        on(on_query_commit)
        ProcessRowsRoot
        Children [
            { rows },
        ]
    }
}

/// Content-region scene for the Processes page.
pub(crate) fn content(context: &PageContext<'_>) -> impl Scene + use<> {
    let palette = context.palette;
    let title = Page::Processes.title().to_owned();
    // Honest capability note: the table still has deliberate incubation
    // seams, while the selected-process details panel is a real projection
    // and request path. The shared placeholder census reads this declaration.
    let note = format!(
        "{} — grouping, per-row trend and batch verbs are in incubation; details follow the selected row",
        Page::Processes.nav_label()
    );
    let viewport_rows = rows_in_viewport(TABLE_VIEWPORT_HEIGHT_PX, palette.control_height_px);
    let projection = rows_projection(context.shell, viewport_rows, 0);
    let count = count_line_text(projection.total, &context.shell.query);
    let columns = visible_columns(&[]);
    let header = header_scene(&columns, sort_projection(context.shell.process_sort));
    let rows_root = rows_root_scene(&projection, palette, &context.shell.query);
    let table = bsn! {
        Node {
            width: percent(68),
            min_width: px(0.0),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
        }
        Children [
            ( { header } ),
            ( { rows_root } ),
        ]
    };
    let details = details::panel_scene(context);
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: Val::Px(space_8()),
        }
        BackgroundColor({ palette.content_bg })
        Children [
            ( Text(title) TextRole(Role::Heading) ),
            ( crate::pages::process_tree::panel_scene(context) ),
            ( Text(note) TextRole(Role::Caption) ),
            ( search_input_scene(palette) ),
            (
                Text(count)
                ProcessCountLine
                TextRole(Role::Caption)
            ),
            (
                Node {
                    width: percent(100),
                    height: px(TABLE_VIEWPORT_HEIGHT_PX),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(space_8()),
                }
                Children [
                    ( { table } ),
                    ( { details } ),
                ]
            ),
        ]
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/processes.rs"]
mod tests;
