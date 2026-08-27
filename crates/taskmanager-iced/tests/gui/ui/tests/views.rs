//! Applications-view behavior tests: the App-history page projections and the
//! canonical category/tree view. Legacy projection behavior is covered at the
//! pure projection seam; the product selector must not expose it again.

use super::super::*;
use super::applications_table_rows;
use taskmanager_application::ProcessItem;
use taskmanager_shell::{SortCol, SortDir};

use super::super::process_projection::ProcessProjection;

/// Render the Applications table for the app's current view state and return
/// the row count (the shared seam the view-mode tests assert on).
fn rendered_row_count(app: &crate::IcedApp) -> usize {
    let process_source = app
        .shell
        .projection()
        .processes
        .as_deref()
        .unwrap_or_default();
    let visible_indices = app.shell.visible_process_indices();
    let visible: Vec<_> = visible_indices
        .iter()
        .filter_map(|&index| process_source.get(index))
        .collect();
    let projection = ProcessProjection::project_with_local_time(
        &visible,
        app.shell.process_sort,
        &app.process_presentation.expanded_groups,
        &app.process_presentation.expanded_tree,
        &taskmanager_application::LocalTimeRulesObservation::unsupported(0),
    );
    let hidden_columns = std::collections::HashSet::new();
    let ctx = RowRender {
        theme: *app.theme(),
        query: String::new(),
        search_active: false,
        swap_visible: true,
        compact: app.compact_density(),
        ui_size: app.ui_size(),
        selected_pids: std::rc::Rc::new(app.shell.selected_pids().clone()),
        selected_row: app.shell.selected_process_row,
        gray_zero: app.preferences().gray_zero_values,
        hidden_columns: std::rc::Rc::new(hidden_columns),
        column_widths: std::rc::Rc::new(crate::app::ColumnWidthOverrides::default()),
    };
    applications_table_rows(&ctx, &projection).len()
}

#[test]
fn applications_lazy_body_key_tracks_only_visual_invalidations() {
    let app = crate::IcedApp::demo();
    let mut render = RowRender {
        theme: *app.theme(),
        query: String::new(),
        search_active: false,
        swap_visible: true,
        compact: false,
        ui_size: taskmanager_theme::tokens::UiSize::Standard,
        selected_pids: std::rc::Rc::new(std::collections::HashSet::new()),
        selected_row: None,
        gray_zero: false,
        hidden_columns: std::rc::Rc::new(std::collections::HashSet::new()),
        column_widths: std::rc::Rc::new(crate::app::ColumnWidthOverrides::default()),
    };
    let base = applications_table_key(7, &render);
    assert_eq!(base, applications_table_key(7, &render));

    render.query = String::from("fire");
    assert_ne!(base, applications_table_key(7, &render));
    render.query.clear();
    let mut selected = std::collections::HashSet::new();
    selected.insert(1);
    render.selected_pids = std::rc::Rc::new(selected);
    assert_ne!(base, applications_table_key(7, &render));
    render.selected_pids = std::rc::Rc::new(std::collections::HashSet::new());
    let mut hidden = std::collections::HashSet::new();
    hidden.insert(SortCol::Fds);
    render.hidden_columns = std::rc::Rc::new(hidden);
    assert_ne!(base, applications_table_key(7, &render));
    assert_ne!(base, applications_table_key(8, &render));
}

#[test]
fn root_view_uses_the_shared_frame_lifecycle_for_first_paint() {
    use taskmanager_shell::TelemetryFrameState;

    let mut app = crate::IcedApp::demo();
    let committed = app
        .shell
        .projection()
        .snapshot
        .clone()
        .expect("demo fixture starts with a committed frame");
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(None)),
    );
    app.shell.application.active_page = taskmanager_application::AppPage::Applications;

    assert_eq!(
        app.shell.telemetry_frame_state(),
        TelemetryFrameState::Collecting
    );
    // The root view must compose its warm-up body while the shared store has
    // only partial/non-frame data; page-local process facts remain hidden.
    {
        let _warmup = view(&app);
    }

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(committed))),
    );
    assert_eq!(
        app.shell.telemetry_frame_state(),
        TelemetryFrameState::Ready
    );
    let _ready = view(&app);
}

#[test]
fn applications_row_materialization_is_bounded_to_the_virtual_window() {
    let app = crate::IcedApp::demo();
    let processes: Vec<ProcessItem> = (0..1_000)
        .map(|index| {
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(10_000 + index)
                .name(format!("worker-{index}"))
                .current_cpu_percentage(index as f32)
                .build()
        })
        .collect();
    let references: Vec<&ProcessItem> = processes.iter().collect();
    let expanded = std::collections::HashSet::from(["category:uncategorized".to_owned()]);
    let projection = ProcessProjection::project_with_local_time(
        &references,
        (SortCol::Cpu, SortDir::Desc),
        &expanded,
        &std::collections::HashSet::new(),
        &taskmanager_application::LocalTimeRulesObservation::unsupported(0),
    );
    let ctx = RowRender {
        theme: *app.theme(),
        query: String::new(),
        search_active: false,
        swap_visible: true,
        compact: false,
        ui_size: taskmanager_theme::tokens::UiSize::Standard,
        selected_pids: std::rc::Rc::new(std::collections::HashSet::new()),
        selected_row: None,
        gray_zero: false,
        hidden_columns: std::rc::Rc::new(std::collections::HashSet::new()),
        column_widths: std::rc::Rc::new(crate::app::ColumnWidthOverrides::default()),
    };
    let window = VirtualWindow::for_rows(
        projection.len(),
        0.0,
        240.0,
        application_row_height(false),
        APPLICATION_HEADER_HEIGHT,
    );
    let rows = applications_table_rows_range(&ctx, &projection, window.start, window.end);

    assert_eq!(rows.len(), window.materialized_len());
    assert!(rows.len() < projection.len());
}

#[test]
fn app_history_page_tab_registers_a_stable_focus_id() {
    // The App-history tab participates in Iced focus traversal like every other
    // page tab; its operation id is page-bound and matches the page->string map.
    assert_eq!(
        crate::focus::focus_id(crate::app::FocusTarget::PageTab(AppPage::AppHistory)),
        "iced-page-tab-app-history"
    );
    assert_eq!(
        crate::capture::page_name(AppPage::AppHistory),
        "app-history"
    );
}

/// Build a minimal canonical `ProcessItem` for grouped-view tests with only
/// the identity, owner, and current observations consumed by the projection.
fn grouped_process_fixture(pid: u32, name: &str, cpu: f32, memory_bytes: u64) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.into())
        .current_cpu_percentage(cpu)
        .current_memory_bytes(memory_bytes)
        .metadata_observations(
            taskmanager_application::ProcessMetadataObservations::current(
                taskmanager_application::ProcessOwner::opaque("devuser"),
                None,
                1,
            ),
        )
        .build()
}

#[test]
fn canonical_hierarchy_control_composes_without_selector_state() {
    taskmanager_test_support::pin_english();
    let app = crate::IcedApp::demo();
    let _control = process_view_selector(app.theme());
    assert_eq!(t("proc.mode_category_tree"), "Categories · Tree");
}

#[test]
fn process_status_filter_selector_is_localized_focusable_and_filters_rows() {
    use crate::app::{FocusTarget, Message, ProcessStatusFilter};

    taskmanager_test_support::pin_english();
    for filter in ProcessStatusFilter::ALL {
        assert!(!filter.label().is_empty());
        let id = crate::focus::focus_id(FocusTarget::ProcessStatusFilterTab(filter));
        assert!(
            id.starts_with("iced-process-status-filter-"),
            "status filter id must be namespaced: {id}"
        );
        assert!(id.ends_with(filter.key()));
    }

    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let all_count = app.shell.visible_processes().len();
    let _ = app.update(Message::SelectProcessStatusFilter(
        ProcessStatusFilter::Running,
    ));
    assert_eq!(app.process_status_filter(), ProcessStatusFilter::Running);
    let visible = app.shell.visible_processes();
    assert!(!visible.is_empty());
    assert!(visible.len() < all_count);
    assert!(
        visible
            .iter()
            .all(|process| ProcessStatusFilter::Running.matches(&process.status))
    );
    assert_eq!(rendered_row_count(&app), visible.len() + 1);
}

#[test]
fn category_tree_groups_processes_without_an_alternate_selector() {
    let mib = 1024 * 1024_u64;
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    // Two same-app processes (both normalize to "Zed") + one distinct app to
    // prove the collapse is per-app, not global.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            grouped_process_fixture(100, "zed", 24.8, 2_640 * mib),
            grouped_process_fixture(101, "zed-worker", 11.2, 1_000 * mib),
            grouped_process_fixture(102, "gnome-shell", 9.6, 1_120 * mib),
        ])),
    );

    let category = "category:uncategorized";
    assert!(app.is_group_expanded(category));
    assert_eq!(rendered_row_count(&app), 4, "header plus three tree nodes");

    // Toggling the category header hides and reveals its recursive children.
    let _ = app.update(Message::ToggleGroupExpansion {
        name: category.into(),
        main_pid: 100,
        flat_index: 0,
        row_key: None,
    });
    assert!(!app.is_group_expanded(category));
    assert_eq!(
        rendered_row_count(&app),
        1,
        "collapsed category keeps its header"
    );
    let _ = app.update(Message::ToggleGroupExpansion {
        name: category.into(),
        main_pid: 100,
        flat_index: 0,
        row_key: None,
    });
    assert!(app.is_group_expanded(category));
    assert_eq!(rendered_row_count(&app), 4);

    // The full page still composes after the hierarchy transitions.
    app.shell.selected = 0;
    let _view = view(&app);
}

#[test]
fn category_tree_keeps_one_first_level_category_axis() {
    let mib = 1024 * 1024_u64;
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    // Two userspace + two kernel processes so each type-group is multi-member
    // and exercises the expandable header path.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            grouped_process_fixture(200, "gnome-shell", 9.6, 1_120 * mib),
            grouped_process_fixture(202, "zed", 5.0, 800 * mib),
            grouped_process_fixture(201, "[kworker/u8:1]", 0.2, 8 * mib),
            grouped_process_fixture(203, "[ksoftirqd/0]", 0.1, 6 * mib),
        ])),
    );

    let category = "category:uncategorized";
    assert_eq!(rendered_row_count(&app), 5, "header plus four tree nodes");

    // The category header remains the only first-level toggle; no Userspace /
    // Kernel grouping is created by the product selector.
    let _ = app.update(Message::ToggleGroupExpansion {
        name: category.into(),
        main_pid: 200,
        flat_index: 0,
        row_key: None,
    });
    assert!(!app.is_group_expanded(category));
    assert_eq!(rendered_row_count(&app), 1);
    let _view = view(&app);
}

#[test]
fn canonical_view_renders_one_header_plus_each_visible_process() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let visible_len = app.shell.visible_processes().len();
    let row_count = rendered_row_count(&app);
    assert_eq!(
        row_count,
        visible_len + 1,
        "canonical hierarchy adds one non-empty category header"
    );
    let _selector = process_view_selector(app.theme());
    let _view = view(&app);
}
