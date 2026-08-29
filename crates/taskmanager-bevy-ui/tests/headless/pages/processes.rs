//! test-intent: behavior
//!
//! Headless behavior tests for the M1 Processes page, mounted from
//! `src/pages/processes.rs` (its own file, never a shared test module).
//!
//! Two layers, mirroring the crate's existing test style:
//! - pure: the row view model (contract-column formatting over typed
//!   observations, honest dashes), the empty/filtered empty states, the
//!   virtual-window clamping and the TUI-parity selection-follow formula;
//! - wired: the real observer composition on a `MinimalPlugins` app — mount,
//!   the seam events (`ProcessSelectStep`/`ProcessScrollIntent`/
//!   `ProcessQueryCommit`), the shell-owned selection reducers they route
//!   through, and the `ShellProjectionFolded` rebuild (plus the idle-frame
//!   no-rebuild contract: entity identity must be stable without a fold).

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{
    ProcessItem, ProcessMetadataObservations, ProcessOwner, ProcessScalarObservations,
};

use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture;
use taskmanager_theme::Theme;

use super::details::ProcessDetailsRoot;
use super::{
    ProcessCountLine, ProcessQueryCommit, ProcessRowIdentity, ProcessRowLink, ProcessRowsRoot,
    ProcessScrollIntent, ProcessScrollState, ProcessSelectStep, ProcessSelectionChanged,
    TABLE_VIEWPORT_HEIGHT_PX, centered_scroll_top, content, count_line_text, empty_state_text,
    rows_projection, sort_projection,
};
use crate::app::{FrontendTrack, Page, PageContext};
use crate::palette::{UiPalette, ui_palette};
use crate::window::WindowPalette;

// ---- fixtures -----------------------------------------------------------

/// A process with exactly the observations the test names — every other
/// scalar stays honestly absent so the dash cells are exercised for free.
fn process(pid: u32, name: &str) -> ProcessItem {
    ProcessItem::new(pid, name)
}

fn with_cpu(process: &mut ProcessItem, cpu: f32) {
    let mut scalars = *process.scalar_observations();
    scalars.cpu_percentage = ScalarObservation::available(cpu, 1);
    process.apply_scalar_observations(scalars);
}

fn with_scalars(process: &mut ProcessItem, edit: impl FnOnce(&mut ProcessScalarObservations)) {
    let mut scalars = *process.scalar_observations();
    edit(&mut scalars);
    process.apply_scalar_observations(scalars);
}

/// A shell whose process inventory is exactly `items` (fixture seam: the same
/// typed seed the TUI tests use, revision included).
fn shell_with(items: Vec<ProcessItem>) -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| *processes = Some(items));
    shell
}

/// `shell_with` plus the shared Applications row context — the same
/// `SelectPage` application the page's mount bootstrap performs (and the TUI
/// performs when its Applications page opens). Pure tests that exercise the
/// shell's selection reducers need it: a fresh shell still sits on the
/// default Performance page and `select_row`/`move_selection` would clamp
/// against the wrong row space.
fn applications_shell(items: Vec<ProcessItem>) -> ShellApp {
    let mut shell = shell_with(items);
    let _ = shell.apply_action(taskmanager_application::AppAction::SelectPage(
        taskmanager_application::AppPage::Applications,
    ));
    shell
}

/// `MinimalPlugins` + the scene/asset plumbing the page scene resolves
/// through, the window palette, and the folded shell track — the same shape
/// the shared window plugin installs, without any window or drain system.
fn headless_page_app(palette: UiPalette, shell: ShellApp) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    // The context borrows the shell while the scene captures what it needs;
    // only then does the shell move into the world's track.
    let history = crate::pages::history::HistoryProjectionResource::default();
    let context = PageContext {
        shell: &shell,
        palette: &palette,
        body: palette.body.clone(),
        heading: palette.heading.clone(),
        history: &history.0,
    };
    let _page_root = app
        .world_mut()
        .spawn_scene(content(&context))
        .expect("the page scene resolves without assets")
        .id();
    app.insert_non_send(FrontendTrack {
        shell,
        initial_refresh_submitted: true,
    });
    app
}

/// Fire one page seam event at the mounted rows root, then flush the frame.
/// The event's `entity` field is its target — the EntityEvent derive routes
/// the trigger to the root's `on()` observers.
fn fire_seam<E>(app: &mut App, event: impl FnOnce(Entity) -> E)
where
    E: bevy::ecs::event::EntityEvent,
    for<'t> E: bevy::ecs::event::Event<Trigger<'t>: Default>,
{
    let root = rows_root(app);
    app.world_mut().commands().trigger(event(root));
    app.update();
}

/// Test-only recorder for the published selection-identity seam.
#[derive(Default, Resource)]
struct SelectionLog(Vec<Option<ProcessRowIdentity>>);

fn record_selection(change: On<ProcessSelectionChanged>, mut log: ResMut<SelectionLog>) {
    log.0.push(change.event().0.clone());
}

fn rows_root(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<ProcessRowsRoot>>()
        .single(app.world())
        .expect("the rows root is mounted")
}

fn row_links(app: &mut App) -> Vec<(Entity, usize)> {
    app.world_mut()
        .query_filtered::<(&ProcessRowLink, bevy::ecs::entity::Entity), ()>()
        .iter(app.world())
        .map(|(link, entity)| (entity, link.0))
        .collect()
}

fn row_texts(app: &mut App) -> Vec<String> {
    app.world_mut()
        .query_filtered::<&Text, ()>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect()
}

fn count_line(app: &mut App) -> String {
    app.world_mut()
        .query_filtered::<&Text, With<ProcessCountLine>>()
        .single(app.world())
        .map(|text| text.0.clone())
        .expect("exactly one count line")
}

/// Read the folded shell through the world (tests mutate it via the fixture
/// seam between frames, exactly like the TUI tests do).
fn with_shell<R>(app: &mut App, read: impl FnOnce(&ShellApp) -> R) -> R {
    // The track is non-send; go through exclusive world access.
    let world = app.world_mut();
    let track = world.non_send_mut::<FrontendTrack>();
    read(&track.shell)
}

// ---- pure: row view model ------------------------------------------------

#[test]
fn row_view_formats_contract_columns_from_typed_observations() {
    let mut item = process(4321, "zed");
    item.status = "Running".to_owned();
    with_scalars(&mut item, |scalars| {
        scalars.cpu_percentage = ScalarObservation::available(24.8, 1);
        scalars.memory_bytes = ScalarObservation::available(512 * 1024 * 1024, 1);
        scalars.threads = ScalarObservation::available(8, 1);
        scalars.nice = ScalarObservation::available(-3, 1);
        scalars.cpu_time_secs = ScalarObservation::available(91, 1);
        scalars.disk_read_bytes_per_sec = ScalarObservation::available(2 * 1024 * 1024, 1);
        // swap / fds / start_time stay absent: honest dashes, never zeros.
    });
    item.apply_metadata_observations(ProcessMetadataObservations::current(
        ProcessOwner::opaque("devuser"),
        None,
        1,
    ));

    let shell = shell_with(vec![item]);
    let projection = rows_projection(&shell, 10, 0);
    assert_eq!(projection.total, 1);
    let cells = &projection.rows[0].cells;
    let column = |id: &str| {
        let index = crate::widgets::table::visible_columns(&[])
            .iter()
            .position(|spec| spec.id == id)
            .unwrap_or_else(|| panic!("contract column {id} missing"));
        cells[index].clone()
    };
    // The single row is also the cursor row (a fresh shell anchors at 0), so
    // the Name cell carries the cursor marker like the TUI highlight symbol.
    assert_eq!(column("Name"), "› zed");
    assert_eq!(column("PID"), "4321");
    assert_eq!(column("Status"), "Running");
    assert_eq!(column("User"), "devuser");
    assert_eq!(column("CPU"), "24.8%");
    assert_eq!(column("Memory"), "512.0 MiB");
    assert_eq!(column("Threads"), "8");
    assert_eq!(column("Nice"), "-3");
    assert_eq!(column("DiskRead"), "2.0 MiB");
    // Unavailable observations are dashes — the shared TUI spelling.
    assert_eq!(column("Swap"), "—");
    assert_eq!(column("FDs"), "—");
    assert_eq!(column("StartTime"), "—", "no local-time observation yet");
    // CPUTime mirrors the TUI's `{value:.1}s` spelling verbatim; on integer
    // seconds the precision flag is inert, so the shared output is "91s".
    assert_eq!(column("CPUTime"), "91s");
}

#[test]
fn the_selected_row_is_flagged_and_carries_the_cursor_marker() {
    let items = vec![process(1, "alpha"), process(2, "beta"), process(3, "gamma")];
    let mut shell = applications_shell(items);
    // Default CPU sort is deterministic here: all values absent, so the
    // stable pid tiebreaker orders ascending.
    assert!(shell.select_row(1), "row 1 is in range");
    let projection = rows_projection(&shell, 10, 0);
    assert!(!projection.rows[0].selected);
    assert!(projection.rows[1].selected);
    assert!(!projection.rows[2].selected);
    assert!(
        projection.rows[1].cells[0].starts_with("› "),
        "the selected Name cell carries the TUI cursor marker"
    );
    assert!(
        !projection.rows[0].cells[0].starts_with("› "),
        "unselected rows do not"
    );
}

#[test]
fn empty_projection_is_honest_per_query_state() {
    let quiet = ShellApp::new();
    let projection = rows_projection(&quiet, 10, 0);
    assert_eq!(projection.total, 0);
    assert!(projection.window.is_empty());
    assert_eq!(empty_state_text(""), t("empty.no_processes_reported"));
    assert_eq!(empty_state_text("zzz"), t("empty.no_processes_match_query"));
    assert_ne!(
        empty_state_text(""),
        empty_state_text("zzz"),
        "quiet platform and over-narrow query are different states"
    );
}

#[test]
fn a_query_that_matches_nothing_empties_the_visible_set() {
    let items = vec![process(1, "alpha"), process(2, "beta")];
    let mut shell = shell_with(items);
    assert!(shell.push_search_text("nomatch"));
    let projection = rows_projection(&shell, 10, 0);
    assert_eq!(projection.total, 0, "the filter lives on the shell");
    assert!(projection.rows.is_empty());
    // And the counter text reports the zero honestly.
    assert!(count_line_text(0, "nomatch").contains("0"));
}

#[test]
fn sort_projection_maps_the_shell_sort_onto_contract_tokens() {
    let cpu_desc = sort_projection((
        taskmanager_shell::SortCol::Cpu,
        taskmanager_shell::SortDir::Desc,
    ));
    assert_eq!(
        cpu_desc,
        Some(crate::widgets::table::SortProjection {
            column: "CPU",
            descending: true,
        })
    );
    let name_asc = sort_projection((
        taskmanager_shell::SortCol::Name,
        taskmanager_shell::SortDir::Asc,
    ));
    assert_eq!(
        name_asc,
        Some(crate::widgets::table::SortProjection {
            column: "Name",
            descending: false,
        })
    );
    // PSS has no contract column: no marker, no fabrication.
    assert_eq!(
        sort_projection((
            taskmanager_shell::SortCol::Pss,
            taskmanager_shell::SortDir::Asc
        )),
        None
    );
}

// ---- pure: virtual window wiring -----------------------------------------

#[test]
fn scroll_intent_clamps_to_the_last_full_window() {
    let items: Vec<ProcessItem> = (0..7).map(|pid| process(pid, "p")).collect();
    let shell = shell_with(items);
    // Intent far past the end pins to the last full page (viewport 3, total 7).
    let projection = rows_projection(&shell, 3, 30);
    assert_eq!(projection.window.first, 4);
    assert_eq!(projection.window.last, 7);
    // The store-back rule: the intent resource normalizes to the window start.
    let mut state = ProcessScrollState {
        viewport_rows: 3,
        top: 30,
    };
    state.top = rows_projection(&shell, state.viewport_rows, state.top)
        .window
        .first;
    assert_eq!(state.top, 4, "a rebuild stores the clamped start back");
}

#[test]
fn selection_follow_matches_the_tui_table_window_formula() {
    // Half a viewport above the cursor, pinned to the last full page.
    assert_eq!(centered_scroll_top(7, 3, 0), 0);
    assert_eq!(centered_scroll_top(7, 3, 3), 2);
    assert_eq!(centered_scroll_top(7, 3, 6), 4);
    assert_eq!(centered_scroll_top(100, 10, 99), 90);
    // Degenerate spaces stay at zero, never underflow.
    assert_eq!(centered_scroll_top(0, 10, 5), 0);
    assert_eq!(centered_scroll_top(5, 0, 2), 0);
}

// ---- wired: the observer composition --------------------------------------

#[test]
fn mount_renders_the_contract_header_and_the_initial_window() {
    let items = vec![
        process(10, "alpha"),
        process(20, "beta"),
        process(30, "gamma"),
    ];
    let mut app = headless_page_app(ui_palette(&Theme::dark()), shell_with(items));
    app.update(); // flush the scene spawn, bootstrap, and Add triggers

    let links = row_links(&mut app);
    assert_eq!(links.len(), 3, "every visible row materializes");
    let indices: Vec<usize> = links.iter().map(|(_, index)| *index).collect();
    assert_eq!(indices, vec![0, 1, 2]);

    let texts = row_texts(&mut app);
    assert!(
        texts.iter().any(|text| *text == Page::Processes.title()),
        "the page renders the shared tab word as its title"
    );
    // The sorted column keeps its pure label; the direction renders as the
    // semantic down plate (never spliced glyph text — the tofu law).
    assert!(
        texts.iter().any(|text| text == "CPU"),
        "the sorted column keeps its pure label"
    );
    let down_plates = app
        .world_mut()
        .query::<&crate::icons::IconPlate>()
        .iter(app.world())
        .filter(|plate| plate.0 == taskmanager_ui_contract::IconId::NavigateDown)
        .count();
    assert!(
        down_plates >= 1,
        "the default CPU-descending sort is marked by the semantic direction plate"
    );
    assert_eq!(count_line(&mut app), count_line_text(3, ""));
}

#[test]
fn details_panel_mounts_and_follows_the_selected_identity() {
    let items = vec![process(10, "alpha"), process(20, "beta")];
    let mut app = headless_page_app(ui_palette(&Theme::dark()), shell_with(items));
    app.update();

    let detail_roots = app
        .world_mut()
        .query_filtered::<Entity, With<ProcessDetailsRoot>>()
        .iter(app.world())
        .count();
    assert_eq!(
        detail_roots, 1,
        "one page-scoped details surface is mounted"
    );
    let texts = row_texts(&mut app);
    assert!(texts.iter().any(|text| text == t("prop.process_details")));
    assert!(
        texts.iter().any(|text| text == "alpha"),
        "the details header follows the initial selected row"
    );

    fire_seam(&mut app, |root| ProcessSelectStep {
        entity: root,
        delta: 1,
    });
    let texts = row_texts(&mut app);
    assert!(
        texts.iter().any(|text| text == "beta"),
        "the details observer follows the same selection event as the table"
    );
}

#[test]
fn selection_step_moves_the_cursor_publishes_identity_and_styles_the_row() {
    let items = vec![
        process(10, "alpha"),
        process(20, "beta"),
        process(30, "gamma"),
    ];
    let mut app = headless_page_app(ui_palette(&Theme::dark()), shell_with(items));
    app.update();
    app.init_resource::<SelectionLog>();
    app.add_observer(record_selection);

    fire_seam(&mut app, |root| ProcessSelectStep {
        entity: root,
        delta: 1,
    });

    assert_eq!(with_shell(&mut app, |shell| shell.selected), 1);
    let log = app.world().resource::<SelectionLog>();
    assert_eq!(
        log.0,
        vec![Some(ProcessRowIdentity {
            pid: 20,
            name: "beta".to_owned(),
        })],
        "the identity change is published exactly once"
    );

    // The selected wrapper wears the palette's selected fill; the others none.
    let palette = ui_palette(&Theme::dark());
    let fills = app
        .world_mut()
        .query_filtered::<(&ProcessRowLink, &bevy::ui::BackgroundColor), ()>()
        .iter(app.world())
        .map(|(link, fill)| (link.0, fill.0))
        .collect::<Vec<_>>();
    for (index, fill) in fills {
        let expected = if index == 1 {
            palette.nav_active_bg
        } else {
            bevy::color::Color::NONE
        };
        assert_eq!(fill.to_srgba(), expected.to_srgba(), "fill for row {index}");
    }

    // Clamping at both edges: further steps beyond the ends never publish.
    fire_seam(&mut app, |root| ProcessSelectStep {
        entity: root,
        delta: -9,
    });
    assert_eq!(with_shell(&mut app, |shell| shell.selected), 0);
    fire_seam(&mut app, |root| ProcessSelectStep {
        entity: root,
        delta: -1,
    });
    assert_eq!(with_shell(&mut app, |shell| shell.selected), 0);
    let log = app.world().resource::<SelectionLog>();
    assert_eq!(
        log.0.len(),
        2,
        "an edge-clamped step changes no identity, so nothing publishes"
    );
}

#[test]
fn selection_on_an_empty_table_is_rejected_without_side_effects() {
    let mut app = headless_page_app(ui_palette(&Theme::dark()), ShellApp::new());
    app.update();
    app.init_resource::<SelectionLog>();
    app.add_observer(record_selection);

    fire_seam(&mut app, |root| ProcessSelectStep {
        entity: root,
        delta: 3,
    });
    assert_eq!(with_shell(&mut app, |shell| shell.selected), 0);
    assert!(
        app.world().resource::<SelectionLog>().0.is_empty(),
        "no identity exists to publish"
    );
    assert!(row_links(&mut app).is_empty(), "no rows materialize");
    let texts = row_texts(&mut app);
    assert!(
        texts
            .iter()
            .any(|text| text == t("empty.no_processes_reported")),
        "the quiet-platform empty state renders"
    );
}

#[test]
fn scroll_intent_rebuilds_the_window_and_clamps() {
    let items: Vec<ProcessItem> = (0..40)
        .map(|pid| {
            let mut item = process(pid, "p");
            with_cpu(&mut item, pid as f32);
            item
        })
        .collect();
    let mut app = headless_page_app(ui_palette(&Theme::dark()), shell_with(items));
    app.update();

    let viewport = app.world().resource::<ProcessScrollState>().viewport_rows;
    assert!(viewport >= 4, "the design viewport holds several rows");

    // Overscroll past the end: pinned to the last full page.
    fire_seam(&mut app, |root| ProcessScrollIntent {
        entity: root,
        rows: 100,
    });
    let state = *app.world().resource::<ProcessScrollState>();
    assert_eq!(state.top, 40 - state.viewport_rows, "clamped intent");
    let links = row_links(&mut app);
    assert_eq!(links.len(), state.viewport_rows);
    assert_eq!(
        links.iter().map(|(_, index)| *index).min(),
        Some(state.top),
        "the first link is the window start"
    );
    assert_eq!(
        links.iter().map(|(_, index)| *index).max(),
        Some(39),
        "the last row stays visible — never an empty tail"
    );

    // Scroll back above the top: clamps to zero, not underflow.
    fire_seam(&mut app, |root| ProcessScrollIntent {
        entity: root,
        rows: -100,
    });
    assert_eq!(app.world().resource::<ProcessScrollState>().top, 0);
    let links = row_links(&mut app);
    assert_eq!(links.iter().map(|(_, index)| *index).min(), Some(0));
}

#[test]
fn query_commit_replaces_the_shell_query_and_refilters_the_rows() {
    let items = vec![
        process(10, "alpha"),
        process(20, "beta-alloy"),
        process(30, "gamma"),
    ];
    let mut app = headless_page_app(ui_palette(&Theme::dark()), shell_with(items));
    app.update();

    fire_seam(&mut app, |root| ProcessQueryCommit {
        entity: root,
        text: "alloy".to_owned(),
    });
    assert_eq!(with_shell(&mut app, |shell| shell.query.clone()), "alloy");
    let links = row_links(&mut app);
    assert_eq!(links.len(), 1, "only the matching row survives");
    // Locale-agnostic oracle: the rendered line IS the pure formatter's
    // output over the same (count, query) pair — shared catalog copy on any
    // host language, never a hardcoded English string.
    assert_eq!(count_line(&mut app), count_line_text(1, "alloy"));

    // Committing the empty string clears the query (backspace parity) and
    // restores the full set.
    fire_seam(&mut app, |root| ProcessQueryCommit {
        entity: root,
        text: String::new(),
    });
    assert_eq!(with_shell(&mut app, |shell| shell.query.clone()), "");
    assert_eq!(row_links(&mut app).len(), 3);
    assert_eq!(count_line(&mut app), count_line_text(3, ""));
}

#[test]
fn a_query_commit_that_matches_nothing_renders_the_match_empty_state() {
    let items = vec![process(10, "alpha")];
    let mut app = headless_page_app(ui_palette(&Theme::dark()), shell_with(items));
    app.update();
    fire_seam(&mut app, |root| ProcessQueryCommit {
        entity: root,
        text: "zzz".to_owned(),
    });
    assert!(row_links(&mut app).is_empty());
    let texts = row_texts(&mut app);
    assert!(
        texts
            .iter()
            .any(|text| text == t("empty.no_processes_match_query")),
        "the over-narrow-query state is distinct from a quiet platform"
    );
}

#[test]
fn a_drain_fold_rebuilds_rows_and_idle_frames_do_not() {
    let items = vec![process(10, "alpha"), process(20, "beta")];
    let mut app = headless_page_app(ui_palette(&Theme::dark()), shell_with(items));
    app.update();
    let before = row_links(&mut app);
    assert_eq!(before.len(), 2);

    // Idle frame: no fold, no rebuild — the same entities survive.
    app.update();
    assert_eq!(
        row_links(&mut app),
        before,
        "an idle frame must not despawn/respawn rows"
    );

    // New facts on the shell alone change nothing until the fold fires.
    {
        let world = app.world_mut();
        let mut track = world.non_send_mut::<FrontendTrack>();
        fixture::edit_processes(&mut track.shell, |processes| {
            if let Some(processes) = processes.as_mut() {
                processes.push(process(30, "gamma"));
            }
        });
    }
    app.update();
    assert_eq!(row_links(&mut app).len(), 2, "no fold, no re-read");

    // The drain's refresh trigger rebuilds the window from the new snapshot.
    app.world_mut()
        .commands()
        .trigger(crate::drain::ShellProjectionFolded(1));
    app.update();
    let after = row_links(&mut app);
    assert_eq!(after.len(), 3, "the fold re-reads the projection");
    let texts = row_texts(&mut app);
    assert!(texts.iter().any(|text| text.contains("gamma")));
    assert_eq!(count_line(&mut app), count_line_text(3, ""));
}

#[test]
fn the_rows_root_survives_its_own_bootstrap_and_seams() {
    // Structural sanity: exactly one rows root, one count line, one search
    // input after any sequence of seam events — the sweep must never eat the
    // chrome it rebuilds into.
    let items: Vec<ProcessItem> = (0..12).map(|pid| process(pid, "p")).collect();
    let mut app = headless_page_app(ui_palette(&Theme::dark()), shell_with(items));
    app.update();
    fire_seam(&mut app, |root| ProcessScrollIntent {
        entity: root,
        rows: 5,
    });
    fire_seam(&mut app, |root| ProcessSelectStep {
        entity: root,
        delta: 4,
    });
    fire_seam(&mut app, |root| ProcessQueryCommit {
        entity: root,
        text: "p".to_owned(),
    });
    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<(), With<ProcessRowsRoot>>()
            .iter(world)
            .count(),
        1,
        "one rows root"
    );
    assert_eq!(
        world
            .query_filtered::<(), With<ProcessCountLine>>()
            .iter(world)
            .count(),
        1,
        "one count line"
    );
    assert_eq!(
        world
            .query_filtered::<(), With<super::ProcessSearchInput>>()
            .iter(world)
            .count(),
        1,
        "one search input"
    );
}

#[test]
fn the_search_input_displays_the_shell_query() {
    // The box is a display over the shell-owned query: the shell is the
    // single search authority (typing folds through its router, paste is
    // capped by SEARCH_QUERY_MAX there), and the input submodule keeps the
    // node in step on every shell mutation.
    let mut shell = shell_with(vec![process(10, "alpha")]);
    let _ = shell.apply_action(taskmanager_application::AppAction::FocusSearch);
    shell.push_search_char('a');
    shell.push_search_char('l');
    let mut app = headless_page_app(ui_palette(&Theme::dark()), shell);
    app.update();
    let world = app.world_mut();
    let input = world
        .query_filtered::<Entity, With<super::ProcessSearchInput>>()
        .single(world)
        .expect("exactly one search input node");
    let text = world
        .get::<Children>(input)
        .and_then(|children| children.iter().next())
        .and_then(|child| world.get::<Text>(*child))
        .expect("the search display text node");
    assert_eq!(text.0, "al", "the display mirrors the shell query");
}

#[test]
fn viewport_capacity_comes_from_the_palette_control_height() {
    // The bootstrap resource-izes viewport/scroll from the palette — the
    // design viewport divided by the theme's control height.
    let palette = ui_palette(&Theme::dark());
    let expected = crate::widgets::table::rows_in_viewport(
        TABLE_VIEWPORT_HEIGHT_PX,
        palette.control_height_px,
    );
    assert!(expected >= 8, "the design fits a useful page ({expected})");
    let items: Vec<ProcessItem> = (0..expected + 20)
        .map(|pid| process(u32::try_from(pid).expect("fixture pid"), "p"))
        .collect();
    let mut app = headless_page_app(ui_palette(&Theme::dark()), shell_with(items));
    app.update();
    assert_eq!(
        app.world().resource::<ProcessScrollState>().viewport_rows,
        expected
    );
    assert_eq!(
        row_links(&mut app).len(),
        expected,
        "the initial window is the viewport, not the whole set"
    );
}

#[test]
fn wheel_scrolls_map_to_signed_rows_by_unit() {
    // Line units map one-to-one; pixel units divide by the row height. The
    // truncation is toward zero and a zero-height row yields zero — a wheel
    // notch is a row, never a rounding surprise. (The scroll system negates
    // the signed result: wheel up moves the window toward earlier rows.)
    use super::input::wheel_rows;
    use bevy::input::mouse::MouseScrollUnit;
    assert_eq!(wheel_rows(3.0, MouseScrollUnit::Line, 34.0), 3);
    assert_eq!(wheel_rows(-1.0, MouseScrollUnit::Line, 34.0), -1);
    assert_eq!(wheel_rows(70.0, MouseScrollUnit::Pixel, 34.0), 2);
    assert_eq!(wheel_rows(10.0, MouseScrollUnit::Pixel, 0.0), 0);
}
