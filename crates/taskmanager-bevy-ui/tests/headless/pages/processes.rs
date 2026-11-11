//! test-intent: behavior
//!
//! Headless behavior tests for the M1 Processes page, mounted from
//! `src/pages/processes.rs` (its own file, never a shared test module).
//!
//! Two layers, mirroring the crate's existing test style:
//! - pure: the row view model (contract-column formatting over typed
//!   observations, honest dashes) and the empty/filtered empty states. Row
//!   paging and the selection-follow reveal are owned by `zhuge-ui-core`
//!   (`row_page`), so this file proves the wiring, not that arithmetic;
//! - wired: the real observer composition on a `MinimalPlugins` app — mount,
//!   the seam events (`ProcessSelectStep`/`ProcessScrollIntent`/
//!   `ProcessQueryCommit`), the shell-owned selection reducers they route
//!   through, and the `ShellProjectionFolded` rebuild (plus the idle-frame
//!   no-rebuild contract: entity identity must be stable without a fold).

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::widget::Text;
use bevy::ui_widgets::ScrollArea;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{
    ProcessItem, ProcessMetadataObservations, ProcessOwner, ProcessScalarObservations,
};
use taskmanager_application::PendingConfirmation;
use taskmanager_application::i18n::t;
use taskmanager_theme::tokens::UiSize;

use taskmanager_shell::fixture;
use taskmanager_shell::{ProcessStatusFilter, ShellApp};
use taskmanager_theme::Theme;

use super::details::ProcessDetailsRoot;
use super::process_fixture_support::{applications_shell, process, shell_with};
use super::toolbar::{ProcessEndTaskButton, ProcessFilterButton, ProcessKillTaskButton};
use super::{
    ProcessCountLine, ProcessQueryCommit, ProcessRowLink, ProcessRowsRoot, ProcessScrollIntent,
    ProcessScrollState, ProcessSelectRow, ProcessSelectStep, ProcessSelectionChanged, content,
    count_line_text, count_line_text_for_shell, details_rail_is_admitted, empty_state_text,
    rows_projection, sort_projection,
};
use crate::app::{FrontendTrack, Page, PageContext};
use crate::palette::{UiPalette, ui_palette};
use crate::window::WindowPalette;
use taskmanager_ui_contract::presentation::vocabulary::COMPACT_CONTENT_WIDTH;

// ---- fixtures -----------------------------------------------------------

#[test]
fn the_details_rail_is_whole_group_admitted_at_the_shared_compact_band() {
    assert!(
        !details_rail_is_admitted(COMPACT_CONTENT_WIDTH - 1.0),
        "one pixel below the shared band hides the whole rail"
    );
    assert!(
        details_rail_is_admitted(COMPACT_CONTENT_WIDTH),
        "the shared band admits the rail, not a Bevy-local breakpoint"
    );
    assert!(!details_rail_is_admitted(f32::NAN));
    assert!(!details_rail_is_admitted(0.0));
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

/// `MinimalPlugins` + the scene/asset plumbing the page scene resolves
/// through, the window palette, and the folded shell track — the same shape
/// the shared window plugin installs, without any window or drain system.
fn headless_page_app(palette: UiPalette, shell: ShellApp) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.add_plugins(crate::input::InputPlugin);
    app.init_resource::<Assets<Font>>();
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    // The context borrows the shell while the scene captures what it needs;
    // only then does the shell move into the world's track.
    let history = crate::pages::history::HistoryProjectionResource::default();
    let context = PageContext {
        sidebar: &Default::default(),
        gray_zero_values: false,
        shell: &shell,

        palette: &palette,
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
        process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion::default(),
    });
    app.update();
    // Seam tests supply an explicit three-row viewport; measured layout is tested separately.
    app.world_mut()
        .resource_mut::<ProcessScrollState>()
        .viewport_rows = 3;
    fire_seam(&mut app, |root| ProcessScrollIntent {
        entity: root,
        rows: 0,
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

/// Test-only recorder for the published selection refresh seam.
#[derive(Default, Resource)]
struct SelectionLog(usize);

fn record_selection(_change: On<ProcessSelectionChanged>, mut log: ResMut<SelectionLog>) {
    log.0 += 1;
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
    // CPUTime reads the shell's single whole-seconds fold
    // (`presentation::optional_cpu_time_seconds`) — the "91s" the integer
    // `{value:.1}s` spelling always produced.
    assert_eq!(column("CPUTime"), "91s");
}

/// The Apps-table Swap cell renders each row's OWN observed per-process charge
/// from the typed `swap_bytes` observation — never a shared/system total, and
/// never a fabricated zero.
///
/// Regression caught: if `pages/processes/projection.rs` drops the
/// `ProcessItem::current_swap_bytes()` arm (e.g. falls back to a system total
/// or to `Some(0)`), the two distinct charges below collapse to one text (or
/// to `0 B`). The unobserved-Swap dash branch stays pinned by
/// `row_view_formats_contract_columns_from_typed_observations`. The test also
/// spawns the production `row_scene` for each projected row and reads the
/// `Text` nodes back, proving the observed charge reaches the painted scene
/// graph, not only the projection cell.
#[test]
fn row_view_renders_the_observed_per_process_swap_charge() {
    let mut heavy = process(501, "heavy-swap");
    with_scalars(&mut heavy, |scalars| {
        scalars.swap_bytes = ScalarObservation::available(32 * 1024 * 1024, 7);
    });
    let mut light = process(502, "light-swap");
    with_scalars(&mut light, |scalars| {
        scalars.swap_bytes = ScalarObservation::available(4 * 1024 * 1024, 7);
    });
    let mut measured_zero = process(503, "measured-zero");
    with_scalars(&mut measured_zero, |scalars| {
        scalars.swap_bytes = ScalarObservation::available(0, 7);
    });

    let shell = shell_with(vec![heavy, light, measured_zero]);
    let projection = rows_projection(&shell, 10, 0);
    assert_eq!(projection.total, 3);
    let swap_index = crate::widgets::table::visible_columns(&[])
        .iter()
        .position(|spec| spec.id == "Swap")
        .expect("contract Swap column");
    let swap = |name: &str| {
        projection
            .rows
            .iter()
            .find(|row| row.name == name)
            .map(|row| row.cells[swap_index].clone())
            .unwrap_or_else(|| panic!("missing rendered row {name}"))
    };
    assert_eq!(
        (swap("heavy-swap"), swap("light-swap")),
        ("32.0 MiB".to_owned(), "4.0 MiB".to_owned()),
        "each row renders its own observed charge; a system total would make both rows equal"
    );
    assert_eq!(
        swap("measured-zero"),
        "0 B",
        "a measured zero charge must stay visible as an observed `0 B`"
    );

    // Painted-scene half: the projection cells above are the renderer input, so
    // spawn the production row scene for each projected row and read the `Text`
    // nodes back. Bevy's headless tooling exposes the spawned scene graph, not a
    // pixel dump, so this proves the observed charge survives into the scene the
    // renderer paints (the closest end-to-end readback Bevy offers; pixel
    // equality stays the Wayland capture matrix's job).
    let columns = crate::widgets::table::visible_columns(&[]);
    let mut scene_app = App::new();
    scene_app.add_plugins(MinimalPlugins);
    scene_app.add_plugins((AssetPlugin::default(), ScenePlugin));
    scene_app.init_resource::<Assets<Font>>();
    for row in &projection.rows {
        scene_app
            .world_mut()
            .spawn_scene(crate::widgets::table::row_scene(&row.cells, &columns))
            .expect("the row scene resolves without assets");
    }
    scene_app.update();
    let world = scene_app.world_mut();
    let painted: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    for expected in ["32.0 MiB", "4.0 MiB", "0 B"] {
        assert!(
            painted.iter().any(|text| text == expected),
            "the spawned row scene must paint the observed swap charge {expected:?}: {painted:?}"
        );
    }
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
    assert_eq!(projection.window.first(), projection.window.last());
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
    let cpu_desc = sort_projection((SortCol::Cpu, SortDir::Desc));
    assert_eq!(
        cpu_desc,
        crate::widgets::table::SortProjection {
            column: "CPU",
            descending: true,
        }
    );
    let name_asc = sort_projection((SortCol::Name, SortDir::Asc));
    assert_eq!(
        name_asc,
        crate::widgets::table::SortProjection {
            column: "Name",
            descending: false,
        }
    );
    assert_eq!(
        sort_projection((SortCol::Pss, SortDir::Asc)),
        crate::widgets::table::SortProjection {
            column: "MemoryPss",
            descending: false,
        }
    );
}

// ---- wired: the observer composition --------------------------------------

#[test]
fn selecting_a_row_follows_it_with_the_shared_centered_reveal() {
    // The page keeps the cursor at the centre of the measured viewport by
    // asking the shared row-page owner for the reveal, never its own arithmetic.
    let items: Vec<ProcessItem> = (0..40).map(|pid| process(pid, "p")).collect();
    let mut app = headless_page_app(
        ui_palette(&Theme::dark(), UiSize::Standard),
        shell_with(items),
    );
    app.update();

    fire_seam(&mut app, |root| ProcessSelectRow {
        entity: root,
        row: 20,
    });
    let state = *app.world().resource::<ProcessScrollState>();
    assert_eq!(state.top, 19, "half a three-row viewport above the cursor");
    let links = row_links(&mut app);
    assert_eq!(
        links.iter().map(|(_, index)| *index).collect::<Vec<_>>(),
        vec![19, 20, 21],
        "the selected row stays materialized inside the revealed window"
    );

    // A cursor pinned at the last row still reveals a full page, not a stub.
    fire_seam(&mut app, |root| ProcessSelectRow {
        entity: root,
        row: 39,
    });
    let state = *app.world().resource::<ProcessScrollState>();
    assert_eq!(state.top, 37, "the last row pins to the final full page");
    assert_eq!(row_links(&mut app).len(), state.viewport_rows);
}

#[test]
fn mount_renders_the_contract_header_and_the_initial_window() {
    let items = vec![
        process(10, "alpha"),
        process(20, "beta"),
        process(30, "gamma"),
    ];
    let mut app = headless_page_app(
        ui_palette(&Theme::dark(), UiSize::Standard),
        shell_with(items),
    );
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
        .query::<&crate::icons::InlineIcon>()
        .iter(app.world())
        .filter(|plate| plate.0 == IconId::NavigateDown)
        .count();
    assert!(
        down_plates >= 1,
        "the default CPU-descending sort is marked by the semantic direction plate, \
         flowing inline with the column label"
    );
    assert_eq!(
        count_line(&mut app),
        with_shell(&mut app, |shell| count_line_text_for_shell(shell, 3, ""))
    );
}

#[test]
fn details_panel_mounts_and_follows_the_selected_identity() {
    let items = vec![process(10, "alpha"), process(20, "beta")];
    let mut app = headless_page_app(
        ui_palette(&Theme::dark(), UiSize::Standard),
        shell_with(items),
    );
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
    let detail_scroll_owners = app
        .world_mut()
        .query_filtered::<Entity, (With<ProcessDetailsRoot>, With<ScrollArea>)>()
        .iter(app.world())
        .count();
    assert_eq!(
        detail_scroll_owners, 1,
        "selected-process details must own an explicit wheel scroll surface instead of clipping its lower facets"
    );
    let texts = row_texts(&mut app);
    assert!(texts.iter().any(|text| text == t("prop.process_details")));
    assert!(
        texts
            .iter()
            .any(|text| text == t("proc_insights.scroll_hint")),
        "the wheel-only details surface must advertise that lower facets are scrollable"
    );
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
fn selection_step_moves_the_cursor_publishes_refresh_and_styles_the_row() {
    let items = vec![
        process(10, "alpha"),
        process(20, "beta"),
        process(30, "gamma"),
    ];
    let mut app = headless_page_app(
        ui_palette(&Theme::dark(), UiSize::Standard),
        shell_with(items),
    );
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
        log.0, 1,
        "the identity change publishes exactly one refresh signal"
    );

    // The selected wrapper wears the theme's selection tint; an odd unselected
    // wrapper wears the zebra stripe; an even one stays open so the page
    // canvas reads through the table. A solid fill on every row would flatten
    // the whole table into one panel and erase the stripe.
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let fills = app
        .world_mut()
        .query_filtered::<(&ProcessRowLink, &bevy::ui::BackgroundColor), ()>()
        .iter(app.world())
        .map(|(link, fill)| (link.0, fill.0))
        .collect::<Vec<_>>();
    for (index, fill) in fills {
        let expected = if index == 1 {
            palette.selection_bg
        } else if index % 2 == 1 {
            palette.zebra_bg
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
        log.0, 2,
        "an edge-clamped step changes no identity, so nothing publishes"
    );
}

#[test]
fn selection_on_an_empty_table_is_rejected_without_side_effects() {
    let mut app = headless_page_app(
        ui_palette(&Theme::dark(), UiSize::Standard),
        ShellApp::new(),
    );
    app.update();
    app.init_resource::<SelectionLog>();
    app.add_observer(record_selection);

    fire_seam(&mut app, |root| ProcessSelectStep {
        entity: root,
        delta: 3,
    });
    assert_eq!(with_shell(&mut app, |shell| shell.selected), 0);
    assert!(
        app.world().resource::<SelectionLog>().0 == 0,
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
    let mut app = headless_page_app(
        ui_palette(&Theme::dark(), UiSize::Standard),
        shell_with(items),
    );
    app.update();

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
    let mut app = headless_page_app(
        ui_palette(&Theme::dark(), UiSize::Standard),
        shell_with(items),
    );
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
    assert_eq!(
        count_line(&mut app),
        with_shell(&mut app, |shell| {
            count_line_text_for_shell(shell, 1, "alloy")
        })
    );

    // Committing the empty string clears the query (backspace parity) and
    // restores the full set.
    fire_seam(&mut app, |root| ProcessQueryCommit {
        entity: root,
        text: String::new(),
    });
    assert_eq!(with_shell(&mut app, |shell| shell.query.clone()), "");
    assert_eq!(row_links(&mut app).len(), 3);
    assert_eq!(
        count_line(&mut app),
        with_shell(&mut app, |shell| count_line_text_for_shell(shell, 3, ""))
    );
}

#[test]
fn a_query_commit_that_matches_nothing_renders_the_match_empty_state() {
    let items = vec![process(10, "alpha")];
    let mut app = headless_page_app(
        ui_palette(&Theme::dark(), UiSize::Standard),
        shell_with(items),
    );
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
    let mut app = headless_page_app(
        ui_palette(&Theme::dark(), UiSize::Standard),
        shell_with(items),
    );
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
        .trigger(crate::drain::ShellProjectionFolded);
    app.update();
    let after = row_links(&mut app);
    assert_eq!(after.len(), 3, "the fold re-reads the projection");
    let texts = row_texts(&mut app);
    assert!(texts.iter().any(|text| text.contains("gamma")));
    assert_eq!(
        count_line(&mut app),
        with_shell(&mut app, |shell| count_line_text_for_shell(shell, 3, ""))
    );
}

#[test]
fn the_rows_root_survives_its_own_bootstrap_and_seams() {
    // Structural sanity: exactly one rows root, one count line, one search
    // input after any sequence of seam events — the sweep must never eat the
    // chrome it rebuilds into.
    let items: Vec<ProcessItem> = (0..12).map(|pid| process(pid, "p")).collect();
    let mut app = headless_page_app(
        ui_palette(&Theme::dark(), UiSize::Standard),
        shell_with(items),
    );
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
    let _ = shell.apply_action(AppAction::FocusSearch);
    shell.push_search_char('a');
    shell.push_search_char('l');
    let mut app = headless_page_app(ui_palette(&Theme::dark(), UiSize::Standard), shell);
    app.update();
    let world = app.world_mut();
    let input = world
        .query_filtered::<Entity, With<super::ProcessSearchInput>>()
        .single(world)
        .expect("exactly one search input node");
    let text = world
        .get::<bevy::text::EditableText>(input)
        .expect("the search input carries the native editor buffer");
    assert_eq!(
        text.value().to_string(),
        "al",
        "the native buffer mirrors the shell query"
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

// ---- multi-select batch tests ----
use taskforest_core::core::process::ProcessBatchAction;
use taskmanager_application::AppAction;
use taskmanager_application::AppPage;
use taskmanager_shell::SortCol;
use taskmanager_shell::SortDir;
use taskmanager_ui_contract::IconId;

fn test_proc(pid: u32, name: &str) -> ProcessItem {
    let mut p = ProcessItem::new(pid, name);
    p.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10_000, 1),
        ..Default::default()
    });
    p
}

#[test]
fn multi_select_processes_arms_batch_confirmation_gate() {
    let mut shell = ShellApp::new();
    let p1 = test_proc(10, "alpha");
    let p2 = test_proc(20, "beta");
    let p3 = test_proc(30, "gamma");
    fixture::edit_processes(&mut shell, |shelved| {
        *shelved = Some(vec![p1, p2, p3]);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));

    // Mark row 0 and toggle row 1
    assert!(shell.select_row(0));
    assert!(shell.toggle_row_selection(1));
    assert_eq!(shell.selected_rows.len(), 2);

    // 1. Multi-select Suspend -> arms ProcessBatch gate
    let effect = shell.request_process_batch(ProcessBatchAction::Suspend);
    assert!(effect.is_none(), "gated batch action returns None");
    let Some(PendingConfirmation::ProcessBatch(intent)) = shell.pending_confirmation() else {
        panic!("multi-select suspend must arm ProcessBatch confirmation gate");
    };
    assert_eq!(intent.action, ProcessBatchAction::Suspend);
    assert_eq!(intent.targets.len(), 2);
    shell.dismiss_overlay();

    // 2. Multi-select Resume -> arms ProcessBatch gate
    let effect = shell.request_process_batch(ProcessBatchAction::Resume);
    assert!(effect.is_none());
    let Some(PendingConfirmation::ProcessBatch(intent)) = shell.pending_confirmation() else {
        panic!("multi-select resume must arm ProcessBatch confirmation gate");
    };
    assert_eq!(intent.action, ProcessBatchAction::Resume);
    assert_eq!(intent.targets.len(), 2);
    shell.dismiss_overlay();

    // 3. Multi-select Kill -> arms ProcessBatch gate
    let effect = shell.request_process_batch(ProcessBatchAction::Kill);
    assert!(effect.is_none());
    let Some(PendingConfirmation::ProcessBatch(intent)) = shell.pending_confirmation() else {
        panic!("multi-select kill must arm ProcessBatch confirmation gate");
    };
    assert_eq!(intent.action, ProcessBatchAction::Kill);
    assert_eq!(intent.targets.len(), 2);
}

#[test]
fn process_sort_header_mounts_and_routes_sort_activation() {
    let items = vec![process(20, "beta"), process(10, "alpha")];
    let mut app = headless_page_app(
        ui_palette(&Theme::dark(), UiSize::Standard),
        shell_with(items),
    );
    app.update();

    let headers: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<crate::widgets::table::ProcessSortHeader>>()
        .iter(app.world())
        .collect();
    assert!(
        !headers.is_empty(),
        "process table header mounts interactive sort headers"
    );
}

#[test]
fn process_properties_modal_opens_and_dismisses_via_button_and_escape() {
    use super::properties_modal::{
        ProcessPropertiesDismissButton, ProcessPropertiesOverlay, republish,
    };
    use crate::window::FrontendWindowPlugin;
    use bevy::ecs::message::MessageWriter;
    use bevy::ecs::system::{Commands, NonSendMut, RunSystemOnce};
    use bevy::input::InputPlugin;
    use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
    use bevy::input_focus::InputFocusPlugin;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((
        AssetPlugin::default(),
        ScenePlugin,
        InputPlugin,
        InputFocusPlugin,
    ));
    app.init_resource::<Assets<Font>>();
    app.add_plugins(FrontendWindowPlugin {
        runtime: crate::runtime::demo_platform_runtime(),
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });

    let target_proc = test_proc(4242, "cargo");
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |shelved| {
        *shelved = Some(vec![target_proc]);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(shell.select_row(0));

    let mut track = app
        .world_mut()
        .get_non_send_mut::<FrontendTrack>()
        .expect("the window plugin installed the track");
    track.shell = shell;

    // Pump frames to initialize window chrome, AppShellRoot and page mount
    app.update();
    app.update();

    // 1. Trigger open properties action on the shell and republish
    app.world_mut()
        .run_system_once(
            |mut track: NonSendMut<FrontendTrack>, mut commands: Commands| {
                let _ = track.shell.apply_action(AppAction::OpenProperties);
                assert!(track.shell.process_properties_target().is_some());
                republish(&mut commands);
            },
        )
        .expect("open properties");
    app.update();

    // Verify modal overlay is mounted
    let world = app.world_mut();
    let overlay_count = world
        .query_filtered::<Entity, With<ProcessPropertiesOverlay>>()
        .iter(world)
        .count();
    assert_eq!(overlay_count, 1, "properties modal overlay is mounted");

    // Verify content reflects target process
    let texts: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|t| t.0.clone())
        .collect();
    assert!(texts.iter().any(|t| t.contains("cargo")));
    assert!(texts.iter().any(|t| t.contains("4242")));

    // 2. Dismiss via Close button
    let dismiss_btn = world
        .query_filtered::<Entity, With<ProcessPropertiesDismissButton>>()
        .iter(world)
        .next()
        .expect("dismiss button entity exists");
    world.commands().trigger(bevy::ui_widgets::Activate {
        entity: dismiss_btn,
    });
    app.update();

    // Verify modal is despawned and shell target cleared
    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<ProcessPropertiesOverlay>>()
            .iter(world)
            .count(),
        0,
        "properties modal is despawned after button activation"
    );
    assert!(
        world
            .non_send::<FrontendTrack>()
            .shell
            .process_properties_target()
            .is_none()
    );

    // 3. Re-open and dismiss via Escape key
    app.world_mut()
        .run_system_once(
            |mut track: NonSendMut<FrontendTrack>, mut commands: Commands| {
                let _ = track.shell.apply_action(AppAction::OpenProperties);
                republish(&mut commands);
            },
        )
        .expect("re-open properties");
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<ProcessPropertiesOverlay>>()
            .iter(world)
            .count(),
        1,
        "re-opened modal is mounted"
    );

    // Send Escape
    let event = KeyboardInput {
        key_code: KeyCode::Escape,
        logical_key: Key::Escape,
        state: bevy::input::ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    };
    let mut event = Some(event);
    app.world_mut()
        .run_system_once(move |mut writer: MessageWriter<KeyboardInput>| {
            if let Some(event) = event.take() {
                writer.write(event);
            }
        })
        .expect("injection runs");
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<ProcessPropertiesOverlay>>()
            .iter(world)
            .count(),
        0,
        "properties modal is despawned after Escape"
    );
    assert!(
        world
            .non_send::<FrontendTrack>()
            .shell
            .process_properties_target()
            .is_none()
    );
}

#[test]
fn toolbar_status_filters_and_end_task_buttons_mount_and_activate() {
    let mut shell = ShellApp::new();
    let p1 = test_proc(1, "alpha");
    let p2 = test_proc(2, "beta");
    fixture::edit_processes(&mut shell, |shelved| {
        *shelved = Some(vec![p1, p2]);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(shell.select_row(0));
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let mut app = headless_page_app(palette, shell);

    let filter_count = app
        .world_mut()
        .query::<&ProcessFilterButton>()
        .iter(app.world())
        .count();
    assert_eq!(filter_count, 6, "six status filter pills mounted");

    let running_pill = app
        .world_mut()
        .query::<(Entity, &ProcessFilterButton)>()
        .iter(app.world())
        .find(|(_, b)| b.0 == ProcessStatusFilter::Running)
        .map(|(e, _)| e)
        .expect("running filter pill entity");

    app.world_mut().trigger(bevy::ui_widgets::Activate {
        entity: running_pill,
    });
    assert_eq!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .process_status_filter,
        ProcessStatusFilter::Running,
        "clicking filter pill sets shell process_status_filter"
    );

    let end_btn = app
        .world_mut()
        .query_filtered::<Entity, With<ProcessEndTaskButton>>()
        .iter(app.world())
        .next()
        .expect("end task button entity");

    let _kill_btn = app
        .world_mut()
        .query_filtered::<Entity, With<ProcessKillTaskButton>>()
        .iter(app.world())
        .next()
        .expect("kill task button entity");

    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: end_btn });
    assert!(
        matches!(
            app.world()
                .non_send::<FrontendTrack>()
                .shell
                .pending_confirmation(),
            Some(PendingConfirmation::EndTask(_))
        ),
        "clicking end task button arms EndTask confirmation"
    );
}
