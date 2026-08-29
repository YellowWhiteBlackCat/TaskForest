use super::*;
use taskmanager_shell::SortCol;

// --- Round-3 ProcessProjection fingerprint cache ---------------------------

/// Helper: seed a demo app on the Applications page with a known process set
/// that exercises every view-mode branch (multi-member group, singleton, tree
/// parent + child).
fn projection_cache_app() -> crate::app::IcedApp {
    let mib = 1024 * 1024_u64;
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let mut p1 = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(100)
        .name("zed".into())
        .current_cpu_percentage(24.8)
        .current_memory_bytes(2_640 * mib)
        .metadata_observations(
            taskmanager_core::core::process::ProcessMetadataObservations::current(
                taskmanager_core::core::process::ProcessOwner::opaque("devuser"),
                None,
                1,
            ),
        )
        .build();
    p1.parent_pid = None;
    let mut p2 = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(101)
        .name("zed-worker".into())
        .current_cpu_percentage(11.2)
        .current_memory_bytes(1_000 * mib)
        .build();
    p2.parent_pid = Some(100);
    let p3 = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(102)
        .name("gnome-shell".into())
        .current_cpu_percentage(9.6)
        .current_memory_bytes(1_120 * mib)
        .build();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![p1, p2, p3])),
    );
    // Simulate the platform batch that delivered the fixture table (the
    // documented convention for direct slot swaps: bump the process revision
    // the shell's own process memos key on).
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRevision(
            taskmanager_shell::fixture::ProjectionSeedDomain::Processes,
        ),
    );
    app
}

/// Two consecutive view calls with unchanged state reuse the same projection
/// allocation (cache HIT — the O(N) `ProcessProjection::project` rebuild is
/// skipped). Proven by comparing the row slice's pointer across calls: a hit
/// returns the cached `Vec<ProjectedRow>` without reallocating.
#[test]
fn projection_cache_hits_on_unchanged_state_and_reuses_the_allocation() {
    let app = projection_cache_app();

    let first = app.projected_rows();
    let ptr_first = first.rows().as_ptr();
    let len_first = first.len();
    drop(first);

    let second = app.projected_rows();
    let ptr_second = second.rows().as_ptr();
    let len_second = second.len();
    drop(second);

    assert_eq!(
        ptr_first, ptr_second,
        "cache hit must return the same allocation (no rebuild)"
    );
    assert_eq!(
        len_first, len_second,
        "row count is stable across cache hits"
    );
    // The cached content matches a fresh projection for
    // the same inputs — the cache only decides whether to rebuild, never what
    // to build (byte-for-byte parity with the uncached path).
    use crate::ui::process_projection::ProcessProjection;
    let visible = app.shell.visible_processes();
    let fresh = ProcessProjection::project_with_local_time(
        &visible,
        app.shell.process_sort,
        &app.process_presentation.expanded_groups,
        &app.process_presentation.expanded_tree,
        &taskmanager_core::core::time::LocalTimeRulesObservation::unsupported(0),
    );
    let cached = app.projected_rows();
    assert_eq!(
        cached.rows(),
        fresh.rows(),
        "cached rows must equal fresh rows"
    );
}

/// A data-tick revision change (the ~1 Hz telemetry refresh) invalidates the
/// cache: the next view call rebuilds the projection, and the allocation
/// address changes (cache MISS → one rebuild per real data update).
#[test]
fn projection_cache_misses_when_the_data_revision_advances() {
    let mut app = projection_cache_app();
    let before = app.projected_rows();
    let ptr_before = before.rows().as_ptr();
    let len_before = before.len();
    drop(before);

    // Simulate a platform batch that bumped the process-domain revision.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRevision(
            taskmanager_shell::fixture::ProjectionSeedDomain::Processes,
        ),
    );

    let after = app.projected_rows();
    let ptr_after = after.rows().as_ptr();
    let len_after = after.len();
    drop(after);

    assert_eq!(
        len_before, len_after,
        "same process set + sort → same row count after a rebuild"
    );
    assert_ne!(
        ptr_before, ptr_after,
        "a data-revision change must invalidate the cache and force a rebuild"
    );
}

/// An unrelated system/performance update must not rebuild the Applications
/// projection. The process-domain revision is intentionally narrower than the
/// shell's global UI/update counter.
#[test]
fn projection_cache_ignores_unrelated_system_revision() {
    let mut app = projection_cache_app();
    let first = app.projected_rows();
    let first_ptr = first.rows().as_ptr();
    drop(first);

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRevision(
            taskmanager_shell::fixture::ProjectionSeedDomain::System,
        ),
    );

    let second = app.projected_rows();
    assert_eq!(
        first_ptr,
        second.rows().as_ptr(),
        "a system-only revision must not rebuild process rows"
    );
}

/// Each frontend-local projection-state change (sort / expand / query)
/// invalidates the cache: the next view call rebuilds and the row shape
/// reflects the new state — enumerated per input so a regression on any one
/// field is caught by its own assertion.
#[test]
fn projection_cache_misses_on_each_view_state_change() {
    let mut app = projection_cache_app();
    assert_eq!(app.projected_rows().len(), 4);

    // --- sort change invalidates even when this fixture happens to keep the
    // same visible order under both columns ---
    let _ = app.update(Message::SortBy(SortCol::Name));
    let name_generation = app.projected_table_model().1;
    let _ = app.update(Message::SortBy(SortCol::Cpu));
    let cpu_generation = app.projected_table_model().1;
    assert_ne!(
        name_generation, cpu_generation,
        "sort is part of the projection fingerprint"
    );

    // --- category expansion change ---
    app.process_presentation.expanded_groups.clear();
    let collapsed = app.projected_rows().len();
    let _ = app.update(Message::ToggleGroupExpansion {
        name: "category:uncategorized".into(),
        main_pid: 100,
        flat_index: 0,
        row_key: None,
    });
    let expanded = app.projected_rows().len();
    assert_eq!(collapsed, 1, "collapsed category keeps one header");
    assert_eq!(expanded, 4, "expanded category reveals three processes");

    // --- query change (filter narrows the visible set) ---
    let _ = app.update(Message::SearchChanged("gnome".into()));
    let filtered = app.projected_rows().len();
    assert_eq!(
        filtered, 2,
        "the query change rebuilds one category header plus the matching process"
    );
    // CloseSearch only hides the search field; clearing the query restores the
    // full visible set (and invalidates the cache on the query clear).
    let _ = app.update(Message::SearchChanged(String::new()));
    let unfiltered = app.projected_rows().len();
    assert_eq!(
        unfiltered, 4,
        "clearing the query restores the full set (cache miss on query clear)"
    );
}

/// The cached projection composes end-to-end into the full Applications page
/// view for every view mode — the cache seam never breaks the renderer path
/// (the projection output is byte-identical to the uncached path, so the
/// sparkline cells, the header, and the empty state all render unchanged).
#[test]
fn projection_cache_composes_into_the_canonical_view() {
    let app = projection_cache_app();
    let _view = crate::ui::view(&app);
    let _view = crate::ui::view(&app);
}

/// Inventory facts are keyed by their own domain revision: a process refresh
/// must not rebuild unchanged Services facts.
#[test]
fn services_projection_ignores_unrelated_process_revision() {
    let mut app = crate::IcedApp::demo();
    let first = std::rc::Rc::clone(&app.services_projection("").0);

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRevision(
            taskmanager_shell::fixture::ProjectionSeedDomain::Processes,
        ),
    );

    let second = std::rc::Rc::clone(&app.services_projection("").0);
    assert!(
        std::rc::Rc::ptr_eq(&first, &second),
        "a process-only revision must not rebuild Services facts"
    );
}

/// Renderer-only state that is absent from every cache contract must preserve
/// all eight memo domains. Holding every returned `Rc` proves this through
/// allocation identity without inspecting private cache fields.
#[test]
fn unrelated_sidebar_state_preserves_every_projection_cache() {
    use std::rc::Rc;
    use taskmanager_telemetry_store::live_graph::MetricSeries;

    let mut app = crate::IcedApp::demo();
    let mut process_history = crate::perf_history::ProcessPerfHistory::new(16);
    process_history.push(44, Some(12.0), Some(2048), Some(30), Some(40));
    app.performance.process_history = Some(process_history);

    let process_perf = app.process_perf_series().unwrap().cpu;
    let history = app.cached_metric_series(taskmanager_shell::presentation::trend::TrendSeries::CpuUsagePercent);
    let processes = app.projected_table_model().0;
    let app_history = app.projected_app_history_model();
    let services = app.services_projection("").0;
    let startup = app.startup_projection().0;
    let users = app.users_projection().0;
    let devices = vec![PerfDevice::Cpu, PerfDevice::Memory];
    let window = crate::ui::VirtualWindow::for_rows(devices.len(), 0.0, 800.0, 96.0, 0.0);
    let rail = app.performance_rail_rows(&devices, window);

    app.performance.sidebar_visible = !app.performance.sidebar_visible;

    assert!(Rc::ptr_eq(
        &process_perf,
        &app.process_perf_series().unwrap().cpu
    ));
    assert!(Rc::ptr_eq(
        &history,
        &app.cached_metric_series(taskmanager_shell::presentation::trend::TrendSeries::CpuUsagePercent)
    ));
    assert!(Rc::ptr_eq(&processes, &app.projected_table_model().0));
    assert!(Rc::ptr_eq(&app_history, &app.projected_app_history_model()));
    assert!(Rc::ptr_eq(&services, &app.services_projection("").0));
    assert!(Rc::ptr_eq(&startup, &app.startup_projection().0));
    assert!(Rc::ptr_eq(&users, &app.users_projection().0));
    assert!(Rc::ptr_eq(
        &rail,
        &app.performance_rail_rows(&devices, window)
    ));
}

/// The three inventory tables have independent revision authorities. A miss
/// in one domain must not evict or rebuild either sibling cache.
#[test]
fn inventory_projection_revisions_invalidate_only_their_own_domain() {
    use std::rc::Rc;

    let mut app = crate::IcedApp::demo();
    let services = app.services_projection("").0;
    let startup = app.startup_projection().0;
    let users = app.users_projection().0;

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRevision(
            taskmanager_shell::fixture::ProjectionSeedDomain::Services,
        ),
    );
    let services_after = app.services_projection("").0;
    let startup_after_services = app.startup_projection().0;
    let users_after_services = app.users_projection().0;
    assert!(!Rc::ptr_eq(&services, &services_after));
    assert!(Rc::ptr_eq(&startup, &startup_after_services));
    assert!(Rc::ptr_eq(&users, &users_after_services));

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRevision(
            taskmanager_shell::fixture::ProjectionSeedDomain::Startup,
        ),
    );
    let services_after_startup = app.services_projection("").0;
    let startup_after = app.startup_projection().0;
    let users_after_startup = app.users_projection().0;
    assert!(Rc::ptr_eq(&services_after, &services_after_startup));
    assert!(!Rc::ptr_eq(&startup_after_services, &startup_after));
    assert!(Rc::ptr_eq(&users_after_services, &users_after_startup));

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRevision(
            taskmanager_shell::fixture::ProjectionSeedDomain::Sessions,
        ),
    );
    let services_after_users = app.services_projection("").0;
    let startup_after_users = app.startup_projection().0;
    let users_after = app.users_projection().0;
    assert!(Rc::ptr_eq(&services_after_startup, &services_after_users));
    assert!(Rc::ptr_eq(&startup_after, &startup_after_users));
    assert!(!Rc::ptr_eq(&users_after_startup, &users_after));
}

/// Performance data invalidates the rail projection without disturbing the
/// process or inventory domains that do not consume `system_revision`.
#[test]
fn system_revision_rebuilds_only_the_performance_rail_domain() {
    use std::rc::Rc;

    let mut app = crate::IcedApp::demo();
    let processes = app.projected_table_model().0;
    let services = app.services_projection("").0;
    let devices = vec![PerfDevice::Cpu, PerfDevice::Memory];
    let window = crate::ui::VirtualWindow::for_rows(devices.len(), 0.0, 800.0, 96.0, 0.0);
    let rail = app.performance_rail_rows(&devices, window);

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRevision(
            taskmanager_shell::fixture::ProjectionSeedDomain::System,
        ),
    );

    assert!(!Rc::ptr_eq(
        &rail,
        &app.performance_rail_rows(&devices, window)
    ));
    assert!(Rc::ptr_eq(&processes, &app.projected_table_model().0));
    assert!(Rc::ptr_eq(&services, &app.services_projection("").0));
}
