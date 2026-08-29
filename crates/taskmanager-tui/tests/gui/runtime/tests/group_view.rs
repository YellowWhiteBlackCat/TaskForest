//! Canonical Applications category-tree interaction tests.

use super::super::*;
use taskmanager_application::AppAction;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::ProcessItem;

fn expected_key(
    kind: fn(taskmanager_shell::ProcessRowIdentity) -> taskmanager_shell::ProcessRowId,
    pid: u32,
) -> Option<taskmanager_shell::ProcessRowId> {
    // trustworthy_process pins the token to the pid itself
    taskmanager_shell::ProcessRowIdentity::from_parts(pid, u64::from(pid)).map(kind)
}

fn trustworthy_process(pid: u32, name: &str, parent_pid: Option<u32>) -> ProcessItem {
    trustworthy_process_with_token(pid, name, parent_pid, u64::from(pid))
}

fn trustworthy_process_with_token(
    pid: u32,
    name: &str,
    parent_pid: Option<u32>,
    start_token: u64,
) -> ProcessItem {
    let mut process = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.into())
        .parent_pid(parent_pid)
        .build();
    process.apply_scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
        start_token: ScalarObservation::available(start_token, 42),
        ..Default::default()
    });
    process
}

fn cpu_process(pid: u32, name: &str, cpu: f32, start_token: u64) -> ProcessItem {
    let mut process = trustworthy_process_with_token(pid, name, None, start_token);
    let mut observations = *process.scalar_observations();
    observations.cpu_percentage = ScalarObservation::available(cpu, 42);
    process.apply_scalar_observations(observations);
    process
}

#[test]
fn enter_and_right_toggle_the_category_header() {
    let mut app = crate::demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![trustworthy_process(
            11, "demo", None,
        )])),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.expanded_groups.clear();
    app.selected = 0;
    assert_eq!(app.visual_row_count(), 1);

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.expanded_groups.contains("category:uncategorized"));
    assert_eq!(app.visual_row_count(), 2);

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Right,
            KeyModifiers::NONE,
        ),
    );
    assert!(!app.expanded_groups.contains("category:uncategorized"));
}

#[test]
fn left_collapses_a_recursive_process_node() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            trustworthy_process(1, "root", None),
            trustworthy_process(2, "child", Some(1)),
        ])),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:uncategorized".to_string()].into_iter().collect();
    app.selected = 1;

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Left, KeyModifiers::NONE),
    );
    assert!(app.collapsed_tree.contains(&1));
    assert_eq!(app.visual_row_count(), 2, "header plus collapsed root");
}

#[test]
fn selection_motion_emits_and_then_dedupes_process_insights() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![trustworthy_process(
            1, "root", None,
        )])),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:uncategorized".to_string()].into_iter().collect();

    let first = app.move_nonflat_selection_oneshot(1);
    assert!(matches!(first, Some(PlatformEffect::ProcessInsights(_))));
    let second = app.refresh_selected_process_insights();
    assert!(second.is_none());
}

#[test]
fn application_aggregate_selection_is_pidless() {
    use taskmanager_core::core::process::{ProcessApplicationIdentity, ProcessMetadataObservation};
    let identity = ProcessApplicationIdentity::new("org.example.Editor", "Editor", None)
        .expect("identity fixture");
    let mut process = trustworthy_process(11, "editor", None);
    process.apply_application_identity(ProcessMetadataObservation::available(identity, 10));
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![process])),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:application".to_string()].into_iter().collect();

    let _ = app.move_nonflat_selection_oneshot(1);
    assert_eq!(
        app.shell.selected_row,
        expected_key(taskmanager_shell::ProcessRowId::Application, 11)
    );
    assert!(app.shell.selected_identities().is_empty());
    assert!(app.shell.selected_process_identity().is_none());
}

/// A live-style process snapshot that changes the process domain must retire
/// the TUI-local per-pid tree state for exited pids (the same timing the
/// shell prunes its stale selections): a reused pid cannot inherit a stale
/// collapse, and a dead application root's expansion key cannot linger.
#[test]
fn a_process_domain_change_prunes_stale_per_pid_tree_state() {
    use taskmanager_application::{
        CorrelatedEvent, PlatformEventBatch, PlatformEventContext, ProcessEvent,
    };
    use taskmanager_platform_contract::{CapabilityId, EventSequence, RequestId};

    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            trustworthy_process(1, "root", None),
            trustworthy_process(2, "child", Some(1)),
            trustworthy_process(3, "other", None),
        ])),
    );
    app.application.active_page = AppPage::Applications;
    // The fixture injects the initial projection directly, so explicitly seed
    // the TUI's previous-identity index before attaching local tree state.
    app.prune_stale_tree_state();
    // Local tree state: the live root's aggregate stays expanded, its child
    // and an unrelated pid are collapsed, a dead root keeps a stale
    // expansion key, and one category key carries no pid at all.
    app.expanded_groups.insert("app-tree:1".to_string());
    app.expanded_groups.insert("app-tree:9".to_string());
    app.expanded_groups
        .insert("category:application".to_string());
    app.collapsed_tree.extend([2u32, 3]);

    // The next live batch reports only pid 1: pids 2 and 3 exited (and 9
    // never existed). The fold goes through the TUI's batch entry, exactly
    // like the runtime event loop does.
    let mut batch = PlatformEventBatch::default();
    batch.process_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(1).expect("fixture request id"),
            capability: CapabilityId::PROCESS_LIST,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
        },
        ProcessEvent::Snapshot(std::sync::Arc::new(vec![trustworthy_process(
            1, "root", None,
        )])),
    ));
    app.apply_platform_batch(batch);

    assert!(
        app.expanded_groups.contains("app-tree:1"),
        "the still-live root keeps its expansion"
    );
    assert!(
        app.expanded_groups.contains("category:application"),
        "pid-less category keys are never pruned"
    );
    assert!(
        !app.expanded_groups.contains("app-tree:9"),
        "the dead root's expansion key must be retired"
    );
    assert!(
        !app.collapsed_tree.contains(&2) && !app.collapsed_tree.contains(&3),
        "exited pids must not stay collapsed (pid reuse would show them pre-collapsed)"
    );
}

#[test]
fn a_reused_pid_cannot_inherit_old_tree_expansion_state() {
    use taskmanager_application::{
        CorrelatedEvent, PlatformEventBatch, PlatformEventContext, ProcessEvent,
    };
    use taskmanager_platform_contract::{CapabilityId, EventSequence, RequestId};

    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            trustworthy_process_with_token(7, "old-process", None, 700),
        ])),
    );
    app.application.active_page = AppPage::Applications;
    // Initialize the identity index from the first observation, then attach
    // presentation state to that exact provider token.
    app.prune_stale_tree_state();
    app.collapsed_tree.insert(7);
    app.expanded_groups.insert("app-tree:7".to_string());

    let mut batch = PlatformEventBatch::default();
    batch.process_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(2).expect("fixture request id"),
            capability: CapabilityId::PROCESS_LIST,
            provider: None,
            sequence: EventSequence::new(2),
            observed_at_ms: 2,
        },
        ProcessEvent::Snapshot(std::sync::Arc::new(vec![trustworthy_process_with_token(
            7,
            "new-process",
            None,
            701,
        )])),
    ));
    app.apply_platform_batch(batch);

    assert!(
        !app.collapsed_tree.contains(&7),
        "a PID-reused process must not inherit the old collapse"
    );
    assert!(
        !app.expanded_groups.contains("app-tree:7"),
        "a PID-reused application root must not inherit old expansion"
    );
}

#[test]
fn reversing_process_sort_keeps_the_selected_row_identity() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            cpu_process(31, "low", 10.0, 310),
            cpu_process(32, "target", 90.0, 320),
        ])),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:uncategorized".to_string()].into_iter().collect();
    app.process_sort = (
        taskmanager_shell::SortCol::Cpu,
        taskmanager_shell::SortDir::Desc,
    );

    let target = app
        .process_rows_snapshot()
        .iter()
        .position(|row| {
            matches!(
                row,
                crate::process_view::ProcessRow::TreeNode { process, .. }
                    if process.pid == 32
            )
        })
        .expect("target row");
    app.selected = target;
    app.sync_grouped_application_selection();

    app.toggle_sort_direction();

    assert_eq!(
        app.selected_detail_process()
            .as_ref()
            .map(|process| process.pid),
        Some(32),
        "reversing sort must preserve the selected process identity"
    );
}

/// The owned digest of one materialized row (bit-exact aggregates), so the
/// cached projection can be compared item-by-item against a fresh rebuild.
fn canonical_row_digest(row: &crate::process_view::ProcessRow<'_>) -> String {
    use crate::process_view::ProcessRow;
    match row {
        ProcessRow::Group {
            name,
            label,
            depth,
            count,
            cpu,
            memory,
            expanded,
            row_key,
        } => format!(
            "G|{name}|{label}|{depth}|{count}|{:?}|{memory}|{expanded}|{row_key:?}",
            cpu.to_bits()
        ),
        ProcessRow::TreeNode {
            process,
            depth,
            has_children,
            collapsed,
        } => format!(
            "T|{}|{}|{:?}|{depth}|{has_children}|{collapsed}",
            process.pid,
            process.name,
            process.current_start_token()
        ),
    }
}

/// The cache-hit contract (TUI-006): with unchanged inputs the cached owned
/// id slice materializes to rows that are item-for-item IDENTICAL to a fresh
/// rebuild, and each of the five presentation inputs (expand, collapse,
/// query, sort, process revision) invalidates the entry so the next read
/// rebuilds — never serving a stale row.
#[test]
fn canonical_row_cache_hits_and_invalidates_per_input_staying_equal_to_a_fresh_rebuild() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            trustworthy_process(1, "root", None),
            trustworthy_process(2, "child", Some(1)),
            trustworthy_process(9, "other", None),
        ])),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:uncategorized".to_string()].into_iter().collect();

    // The cached projection must always match a fresh rebuild of the SAME
    // visible list, expansion set, collapse set, and sort.
    fn assert_cache_equals_rebuild(app: &TuiApp) {
        let cached = app.process_rows_snapshot();
        let fresh = crate::process_view::build_process_rows(
            &app.visible_processes(),
            &app.expanded_groups,
            &app.collapsed_tree,
            app.process_sort,
        );
        let cached: Vec<String> = cached.iter().map(canonical_row_digest).collect();
        let fresh: Vec<String> = fresh.iter().map(canonical_row_digest).collect();
        assert_eq!(cached, fresh, "the cache must equal a fresh rebuild");
    }

    // Cold build, then a repeated read (cache hit): both equal the rebuild.
    let _ = app.process_rows_snapshot();
    assert!(
        app.canonical_row_cache_is_valid_for_current_inputs(),
        "the first read must install the cache entry"
    );
    assert_cache_equals_rebuild(&app);
    assert_cache_equals_rebuild(&app);
    assert!(
        app.canonical_row_cache_is_valid_for_current_inputs(),
        "unchanged inputs must keep the entry valid (hit path)"
    );

    // Input 1 — expand: an application aggregate opens its tree.
    app.expanded_groups.insert("app-tree:1".to_string());
    assert!(
        !app.canonical_row_cache_is_valid_for_current_inputs(),
        "an expansion change must invalidate the entry"
    );
    assert_cache_equals_rebuild(&app);

    // Input 2 — collapse: a collapsed pid hides exactly its subtree.
    app.collapsed_tree.insert(1);
    assert!(!app.canonical_row_cache_is_valid_for_current_inputs());
    assert_cache_equals_rebuild(&app);
    app.collapsed_tree.clear();
    assert_cache_equals_rebuild(&app);

    // Input 3 — query: the visible list shrinks to matching rows.
    app.query = "other".to_string();
    assert!(
        !app.canonical_row_cache_is_valid_for_current_inputs(),
        "a query change must invalidate the entry"
    );
    assert_cache_equals_rebuild(&app);
    app.query.clear();
    assert_cache_equals_rebuild(&app);

    // Input 4 — sort: the visible order (and the tree order) flips.
    app.process_sort = (
        taskmanager_shell::SortCol::Pid,
        taskmanager_shell::SortDir::Asc,
    );
    assert!(
        !app.canonical_row_cache_is_valid_for_current_inputs(),
        "a sort change must invalidate the entry"
    );
    assert_cache_equals_rebuild(&app);

    // Input 5 — revision: a new provider batch invalidates even with
    // identical presentation inputs.
    let mut rebatch = vec![trustworthy_process(1, "root", None)];
    rebatch.push(trustworthy_process(2, "child", Some(1)));
    rebatch.push(trustworthy_process(9, "other", None));
    rebatch.push(trustworthy_process(10, "newcomer", None));
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(rebatch)),
    );
    assert!(
        !app.canonical_row_cache_is_valid_for_current_inputs(),
        "a process revision bump must invalidate the entry"
    );
    assert_cache_equals_rebuild(&app);
    assert_eq!(
        app.visual_row_count(),
        app.process_rows_snapshot().len(),
        "the count and the row slice share one cache entry"
    );
}

/// Toggling a group through the cached projection preserves the selected row
/// by its provider identity (pid + start-token), exercising the anchor
/// capture/restore path over the owned id slice.
#[test]
fn anchor_survives_group_toggles_through_the_cached_projection() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            trustworthy_process(1, "root", None),
            trustworthy_process(2, "child", Some(1)),
        ])),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = [
        "category:uncategorized".to_string(),
        "app-tree:1".to_string(),
    ]
    .into_iter()
    .collect();

    // Park the cursor on the child row and warm the cache.
    let child_row = app
        .process_rows_snapshot()
        .iter()
        .position(|row| {
            matches!(row, crate::process_view::ProcessRow::TreeNode { process, .. }
                if process.pid == 2)
        })
        .expect("fixture must render the child row");
    app.selected = child_row;
    app.sync_grouped_application_selection();
    let _ = app.process_rows_snapshot();
    assert!(app.canonical_row_cache_is_valid_for_current_inputs());

    // Collapse the root through the id-consuming toggle: the anchor must be
    // re-resolved by pid + start-token, not by position.
    assert!(app.collapse_tree_pid(1));
    assert_eq!(app.collapsed_tree, [1].into_iter().collect());
    let landed_pid = {
        let rows = app.process_rows_snapshot();
        let pid_of = |row: &crate::process_view::ProcessRow<'_>| match row {
            crate::process_view::ProcessRow::TreeNode { process, .. } => Some(process.pid),
            crate::process_view::ProcessRow::Group {
                row_key: Some(taskmanager_shell::ProcessRowId::Application(identity)),
                ..
            } => Some(identity.pid()),
            _ => None,
        };
        rows.get(app.selected).and_then(pid_of)
    };
    // The child subtree vanished under the collapse; the anchor restore falls
    // back to the clamped cursor honestly — but it must never invent a
    // selection on a vanished pid.
    assert_ne!(
        landed_pid,
        Some(2),
        "a collapsed child must not stay selected"
    );

    // Re-expanding restores the child row by identity.
    assert!(app.expand_tree_pid(1));
    let rows = app.process_rows_snapshot();
    assert!(rows.iter().any(|row| matches!(row,
        crate::process_view::ProcessRow::TreeNode { process, .. } if process.pid == 2)));
}
