//! Pure (no-gpui) unit tests for the Apps-page view-layer sort + status-filter
//! functions exported from `gpui_app::processes_view`. These functions are pure
//! over `&[ProcessItem]` / primitive inputs — the gpui render path (`proc_row`,
//! `AnyElement`, `Entity<RootView>`) is NOT exercised here, so the tests are
//! deterministic and need no window/context.
//!
//! Coverage:
//! - the canonical category-tree projection: every `SortCol` variant ×
//!   {asc, desc}, plus a real parent/child tree to verify per-level sort,
//!   depth-first flatten, depth/affordance fields, and the collapse set.
//! - the shell `ProcessViewing::click_sort_column` header-click rule: toggle
//!   on the active column; conventional initial direction on a new column.
//! - `ProcessStatusFilter::matches`: each first-letter bucket (R/S/T/Z), full
//!   words, single letters, case-insensitivity, `Other` fallthrough, empty +
//!   whitespace strings, leading-whitespace tolerance, the `All` wildcard, and
//!   negative cross-bucket cases. (`classify` is private — covered via `matches`.)
//! - the shared `taskmanager_shell::matches_process_query` grammar (via the
//!   flat-list helper below): empty-query fast path, name substring,
//!   case-folding, pid-as-digits, and no-match.

use std::collections::HashSet;

use taskmanager::core::process::ProcessItem;
use taskmanager_gpui::gpui_app::processes_view::{
    ProcessStatusFilter, SortCol, VisibleRow, category_tree_rows, default_category_expansions,
};
use taskmanager_shell::{ProcessViewing, SortDir, matches_process_query};

/// The shell query grammar's flat-list equivalent of the retired local
/// `filter_processes`: same predicate the Apps projection applies per row.
fn query_filter<'a>(procs: &[&'a ProcessItem], query: &str) -> Vec<&'a ProcessItem> {
    procs
        .iter()
        .copied()
        .filter(|process| matches_process_query(process, query))
        .collect()
}

fn refs(items: &[ProcessItem]) -> Vec<&ProcessItem> {
    items.iter().collect()
}

/// Project the only runtime hierarchy and retain its selectable process rows.
/// Structural category headers deliberately stay out of PID-order assertions.
fn canonical_process_rows(
    processes: &[&ProcessItem],
    column: SortCol,
    ascending: bool,
    collapsed: &HashSet<u32>,
) -> Vec<VisibleRow> {
    category_tree_rows(
        processes,
        column,
        ascending,
        &default_category_expansions(),
        collapsed,
    )
    .into_iter()
    .filter(|row| row.process_pid.is_some())
    .collect()
}

// ── fixtures ─────────────────────────────────────────────────────────────

/// Build a `ProcessItem` with every sort-relevant field set. `parent_pid = Some(1)`
/// and pid 1 is NOT in any fixture below, so each item is an orphan ROOT inside
/// the canonical uncategorized bucket. Each column is pinned to a STRICTLY DISTINCT value across the
/// five pids, so every sort key yields an unambiguous total order with no
/// stable-sort ties.
#[allow(clippy::too_many_arguments)]
fn mk(
    pid: u32,
    name: &str,
    user: &str,
    threads: u32,
    start: u64,
    status: &str,
    cpu: f32,
    mem: u64,
    dr: u64,
    dw: u64,
) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .scalar_observations(ProcessScalarObservations {
            swap_bytes: ScalarObservation::available(u64::from(pid), 1),
            ..ProcessScalarObservations::default()
        })
        .pid(pid)
        .parent_pid(Some(1))
        .name(name.to_string())
        .cmdline(String::new())
        .current_cpu_percentage(cpu)
        .current_memory_bytes(mem)
        .current_disk_read_bytes_per_sec(dr)
        .current_disk_write_bytes_per_sec(dw)
        .status(status.to_string())
        .metadata_observations(
            taskmanager_application::ProcessMetadataObservations::current(
                taskmanager_application::ProcessOwner::opaque(user.to_string()),
                None,
                1,
            ),
        )
        .current_threads(threads)
        .current_start_time_secs(start)
        .cpu_history(Vec::new())
        .build()
}

/// The canonical 5-process dataset (all roots). Column → pid mapping is
/// deliberately unique per column so the expected order tables below are exact.
///
/// | pid | name    | user  | thr | start | status    | cpu   | mem(B)      | dr   | dw   |
/// |----:|---------|-------|----:|------:|-----------|------:|------------:|-----:|-----:|
/// |  11 | charlie | zoe   |   3 |   300 | Sleeping  |  5.0  |  100_000_000| 1000 |   10 |
/// |  22 | alpha   | bob   |   1 |   100 | Running   | 50.0  |   10_000_000|   10 | 1000 |
/// |  33 | echo    | alice |   5 |   500 | Zombie    |  1.0  |  500_000_000|  500 |  500 |
/// |  44 | delta   | eve   |   2 |   200 | Stopped   | 25.0  |   50_000_000|  100 |  100 |
/// |  55 | bravo   | dale  |   4 |   400 | Waiting   | 10.0  |  200_000_000| 2000 | 2000 |
fn sample() -> Vec<ProcessItem> {
    vec![
        mk(
            11,
            "charlie",
            "zoe",
            3,
            300,
            "Sleeping",
            5.0,
            100_000_000,
            1000,
            10,
        ),
        mk(
            22, "alpha", "bob", 1, 100, "Running", 50.0, 10_000_000, 10, 1000,
        ),
        mk(
            33,
            "echo",
            "alice",
            5,
            500,
            "Zombie",
            1.0,
            500_000_000,
            500,
            500,
        ),
        mk(
            44, "delta", "eve", 2, 200, "Stopped", 25.0, 50_000_000, 100, 100,
        ),
        mk(
            55,
            "bravo",
            "dale",
            4,
            400,
            "Waiting",
            10.0,
            200_000_000,
            2000,
            2000,
        ),
    ]
}

/// Expected pid order for `sample()` under each `SortCol`, ascending and
/// descending. desc is the EXACT reverse of asc because every key is distinct
/// (no equal keys → no stable-sort tie-preservation to account for).
///
/// NOTE on `Status`: the view-layer comparator (`rows.rs`) does a RAW
/// case-sensitive `a.status.cmp(&b.status)` (NOT lowercased). All five statuses
/// here are Capitalized distinct words, so the byte order is
/// Running < Sleeping < Stopped < Waiting < Zombie.
fn expected_orders() -> &'static [(SortCol, &'static [u32], &'static [u32])] {
    &[
        (SortCol::Name, &[22, 55, 11, 44, 33], &[33, 44, 11, 55, 22]),
        (SortCol::User, &[33, 22, 55, 44, 11], &[11, 44, 55, 22, 33]),
        (SortCol::Pid, &[11, 22, 33, 44, 55], &[55, 44, 33, 22, 11]),
        (
            SortCol::Threads,
            &[22, 44, 11, 55, 33],
            &[33, 55, 11, 44, 22],
        ),
        (
            SortCol::StartTime,
            &[22, 44, 11, 55, 33],
            &[33, 55, 11, 44, 22],
        ),
        (SortCol::State, &[22, 11, 44, 55, 33], &[33, 55, 44, 11, 22]),
        (SortCol::Cpu, &[33, 11, 55, 44, 22], &[22, 44, 55, 11, 33]),
        (
            SortCol::Memory,
            &[22, 44, 11, 55, 33],
            &[33, 55, 11, 44, 22],
        ),
        (SortCol::Swap, &[11, 22, 33, 44, 55], &[55, 44, 33, 22, 11]),
        (
            SortCol::DiskRead,
            &[22, 44, 33, 11, 55],
            &[55, 11, 33, 44, 22],
        ),
        (
            SortCol::DiskWrite,
            &[11, 44, 33, 22, 55],
            &[55, 22, 33, 44, 11],
        ),
    ]
}

// ── canonical category tree: every SortCol × {asc, desc} ─────────────────

#[test]
fn canonical_tree_orders_each_sortcol_asc_and_desc() {
    let procs = sample();
    let collapsed = HashSet::new();
    for &(col, asc_want, desc_want) in expected_orders() {
        let got_asc: Vec<u32> = canonical_process_rows(&refs(&procs), col, true, &collapsed)
            .iter()
            .map(|r| r.pid)
            .collect();
        assert_eq!(
            got_asc, asc_want,
            "canonical tree asc  {col:?}: got {got_asc:?}, want {asc_want:?}"
        );
        let got_desc: Vec<u32> = canonical_process_rows(&refs(&procs), col, false, &collapsed)
            .iter()
            .map(|r| r.pid)
            .collect();
        assert_eq!(
            got_desc, desc_want,
            "canonical tree desc {col:?}: got {got_desc:?}, want {desc_want:?}"
        );
    }
}

#[test]
fn canonical_tree_tiebreak_contract_on_equal_primary_keys() {
    // Every branch of the canonical recursive tree uses the same shared
    // comparator. Equal primary keys therefore resolve by the neutral PID
    // tiebreak, independent of sort direction (except when PID itself is the
    // primary key).
    let procs = vec![
        mk(7, "same", "u", 1, 1, "X", 1.0, 1, 1, 1),
        mk(3, "same", "u", 1, 1, "X", 1.0, 1, 1, 1),
    ];
    let collapsed = HashSet::new();
    for &(col, _, _) in expected_orders() {
        let asc: Vec<u32> = canonical_process_rows(&refs(&procs), col, true, &collapsed)
            .iter()
            .map(|r| r.pid)
            .collect();
        let desc: Vec<u32> = canonical_process_rows(&refs(&procs), col, false, &collapsed)
            .iter()
            .map(|r| r.pid)
            .collect();
        match col {
            SortCol::Pid | SortCol::Swap => {
                assert_eq!(asc, vec![3, 7]);
                assert_eq!(desc, vec![7, 3]);
            }
            _ => {
                assert_eq!(
                    asc,
                    vec![3, 7],
                    "canonical tree asc  {col:?}: neutral comparator tiebreak puts lower pid first"
                );
                assert_eq!(
                    desc,
                    vec![3, 7],
                    "canonical tree desc {col:?}: neutral comparator tiebreak is direction-independent (lower pid first)"
                );
            }
        }
    }
}

// ── canonical category tree: structural behavior ─────────────────────────

#[test]
fn canonical_tree_all_roots_share_the_recursive_sort_contract() {
    // With no parent/child links every member is a root beneath the one honest
    // category header, so its process rows isolate the recursive comparator.
    let procs = sample();
    let collapsed = HashSet::new();
    for &(col, asc_want, desc_want) in expected_orders() {
        let got_asc: Vec<u32> = canonical_process_rows(&refs(&procs), col, true, &collapsed)
            .iter()
            .map(|r| r.pid)
            .collect();
        assert_eq!(
            got_asc, asc_want,
            "canonical roots asc  {col:?}: got {got_asc:?}, want {asc_want:?}"
        );
        let got_desc: Vec<u32> = canonical_process_rows(&refs(&procs), col, false, &collapsed)
            .iter()
            .map(|r| r.pid)
            .collect();
        assert_eq!(
            got_desc, desc_want,
            "canonical roots desc {col:?}: got {got_desc:?}, want {desc_want:?}"
        );
    }
}

// ── canonical category tree: real parent/child structure ─────────────────

/// Build a tree node `ProcessItem` with only name + user populated (the two
/// columns exercised by the structural tree tests). Other fields default.
fn mk_tree(pid: u32, parent: Option<u32>, name: &str, user: &str) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .parent_pid(parent)
        .name(name.to_string())
        .metadata_observations(
            taskmanager_application::ProcessMetadataObservations::current(
                taskmanager_application::ProcessOwner::opaque(user.to_string()),
                None,
                1,
            ),
        )
        .build()
}

/// Three roots (100/200/300); root 200 has two children (201/202). Names and
/// users are chosen so the Name-asc and User-asc orders DIFFER, proving the
/// test actually exercises each column's comparator (not a coincidence).
///
/// ```text
/// 100 parent_b  (u_q)   200 parent_a (u_x)   300 parent_c (u_a)
///                      ├── 201 child_z (u_z)
///                      └── 202 child_y (u_y)
/// ```
fn tree_sample() -> Vec<ProcessItem> {
    vec![
        mk_tree(100, None, "parent_b", "u_q"),
        mk_tree(200, None, "parent_a", "u_x"),
        mk_tree(201, Some(200), "child_z", "u_z"),
        mk_tree(202, Some(200), "child_y", "u_y"),
        mk_tree(300, None, "parent_c", "u_a"),
    ]
}

#[test]
fn canonical_tree_sorts_per_level_and_flattens_depth_first() {
    let procs = tree_sample();
    let collapsed = HashSet::new();

    // Name asc (data-layer sort_nodes):
    // roots [parent_a(200), parent_b(100), parent_c(300)];
    // children of 200 sorted asc [child_y(202), child_z(201)];
    // DFS emits each parent immediately followed by its (sorted) children.
    let pids: Vec<u32> = canonical_process_rows(&refs(&procs), SortCol::Name, true, &collapsed)
        .iter()
        .map(|r| r.pid)
        .collect();
    assert_eq!(pids, vec![200, 202, 201, 100, 300]);

    // Name desc: roots reversed, children reversed.
    let pids: Vec<u32> = canonical_process_rows(&refs(&procs), SortCol::Name, false, &collapsed)
        .iter()
        .map(|r| r.pid)
        .collect();
    assert_eq!(pids, vec![300, 100, 200, 201, 202]);

    // User asc (view-layer sort_nodes_by — a DIFFERENT code path than Name):
    // roots by user [u_a(300), u_q(100), u_x(200)];
    // children of 200 by user [u_y(202), u_z(201)].
    let pids: Vec<u32> = canonical_process_rows(&refs(&procs), SortCol::User, true, &collapsed)
        .iter()
        .map(|r| r.pid)
        .collect();
    assert_eq!(pids, vec![300, 100, 200, 202, 201]);
}

#[test]
fn canonical_tree_depth_and_affordance_fields_include_category_offset() {
    let procs = tree_sample();
    let collapsed = HashSet::new();
    let rows = canonical_process_rows(&refs(&procs), SortCol::Name, true, &collapsed);

    // Parent root: depth 1 below the category, has_children true, not collapsed.
    let r200 = rows.iter().find(|r| r.pid == 200).unwrap();
    assert_eq!(r200.depth, 1);
    assert!(r200.has_children);
    assert!(!r200.collapsed);

    // Leaf child: depth 2, no children.
    let r202 = rows.iter().find(|r| r.pid == 202).unwrap();
    assert_eq!(r202.depth, 2);
    assert!(!r202.has_children);
    assert!(!r202.collapsed);

    // Childless roots report has_children = false.
    assert!(!rows.iter().find(|r| r.pid == 100).unwrap().has_children);
    assert!(!rows.iter().find(|r| r.pid == 300).unwrap().has_children);
}

#[test]
fn canonical_tree_collapse_set_hides_descendants_but_keeps_parent() {
    let procs = tree_sample();
    let collapsed: HashSet<u32> = [200].into_iter().collect();

    let pids: Vec<u32> = canonical_process_rows(&refs(&procs), SortCol::Name, true, &collapsed)
        .iter()
        .map(|r| r.pid)
        .collect();
    // parent_a(200) is still emitted (collapsed parents stay visible) but its
    // descendants 201/202 are pruned by flatten_tree_visible.
    assert_eq!(pids, vec![200, 100, 300]);

    // The collapsed parent row reports collapsed = true (drives the chevron glyph).
    let rows = canonical_process_rows(&refs(&procs), SortCol::Name, true, &collapsed);
    let r200 = rows.iter().find(|r| r.pid == 200).unwrap();
    assert!(r200.collapsed);
    assert!(r200.has_children);
}

// ── header-click sort rule (shell `ProcessViewing::click_sort_column`) ────

#[test]
fn click_sort_column_toggles_direction_on_active_column() {
    // Clicking the column that is ALREADY active flips asc/desc, regardless of
    // whether the column is text-like or numeric.
    let mut viewing = ProcessViewing::default();
    for (col, direction) in [
        (SortCol::Name, SortDir::Asc),
        (SortCol::Name, SortDir::Desc),
        (SortCol::Cpu, SortDir::Asc),
        (SortCol::Cpu, SortDir::Desc),
        (SortCol::Memory, SortDir::Asc),
    ] {
        viewing.set_sort(col, direction);
        viewing.click_sort_column(col);
        assert_eq!(viewing.sort(), (col, direction.toggle()));
    }
}

#[test]
fn click_sort_column_new_column_gets_conventional_initial_direction() {
    // Clicking a currently-INACTIVE column activates it with the conventional
    // initial direction: ASC for text-like columns (Name/User/Pid/State), DESC
    // for numeric columns (Threads/StartTime/Cpu/Memory/Swap/DiskRead/DiskWrite).
    //
    // NOTE: `Pid` is classified TEXT here (asc) — even though PIDs are numeric.
    // The convention is "columns whose natural reading is lexicographic start
    // ascending".
    //
    // The previous (column, direction) is irrelevant on this branch: only the
    // clicked column's text/numeric classification decides the direction.
    let cases: &[(SortCol, SortDir)] = &[
        (SortCol::User, SortDir::Asc),
        (SortCol::Pid, SortDir::Asc),
        (SortCol::State, SortDir::Asc),
        (SortCol::Threads, SortDir::Desc),
        (SortCol::StartTime, SortDir::Desc),
        (SortCol::Cpu, SortDir::Desc),
        (SortCol::Memory, SortDir::Desc),
        (SortCol::Swap, SortDir::Desc),
        (SortCol::DiskRead, SortDir::Desc),
        (SortCol::DiskWrite, SortDir::Desc),
    ];
    for &(col, want) in cases {
        let mut viewing = ProcessViewing::default();
        viewing.set_sort(SortCol::Name, SortDir::Asc);
        viewing.click_sort_column(col);
        assert_eq!(
            viewing.sort(),
            (col, want),
            "new-column initial direction for {col:?}"
        );
    }

    // Sanity: the same target column yields the same initial direction no matter
    // what the previous active column/direction was.
    let mut viewing = ProcessViewing::default();
    viewing.set_sort(SortCol::Cpu, SortDir::Desc);
    viewing.click_sort_column(SortCol::Name);
    assert_eq!(viewing.sort(), (SortCol::Name, SortDir::Asc));
    viewing.set_sort(SortCol::State, SortDir::Desc);
    viewing.click_sort_column(SortCol::Cpu);
    assert_eq!(viewing.sort(), (SortCol::Cpu, SortDir::Desc));
}

#[test]
fn memory_projection_prefers_current_pss_and_falls_back_to_typed_rss() {
    let pss = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(71)
        .scalar_observations(ProcessScalarObservations {
            memory_bytes: ScalarObservation::available(900, 1),
            memory_pss_bytes: ScalarObservation::available(123, 1),
            swap_bytes: ScalarObservation::available(7, 1),
            ..ProcessScalarObservations::default()
        })
        .build();
    let fallback = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(72)
        .scalar_observations(ProcessScalarObservations {
            memory_bytes: ScalarObservation::available(456, 1),
            memory_pss_bytes: ScalarObservation::unavailable(
                taskmanager::core::FailureKind::TemporarilyUnavailable,
            ),
            ..ProcessScalarObservations::default()
        })
        .build();

    let rows = canonical_process_rows(
        &refs(&[pss, fallback]),
        SortCol::Memory,
        false,
        &HashSet::new(),
    );
    assert_eq!(rows[0].mem, Some(123));
    assert_eq!(rows[0].swap, Some(7));
    assert_eq!(rows[1].mem, Some(456));
}

// ── ProcessStatusFilter::matches ──────────────────────────────────────────
//
// `classify` is private in filter.rs; it is fully exercised here through the
// public `matches` (which delegates: `All` short-circuits, otherwise
// `Self::classify(status) == self`).

#[test]
fn status_filter_all_matches_every_status() {
    for s in [
        "Running",
        "Sleeping",
        "Stopped",
        "Zombie",
        "",
        "   ",
        "anything",
        "Disk sleep",
    ] {
        assert!(ProcessStatusFilter::All.matches(s), "All must match {s:?}");
    }
}

#[test]
fn status_filter_running_bucket() {
    // Full word, single letter, case-insensitive, leading whitespace tolerated.
    assert!(ProcessStatusFilter::Running.matches("Running"));
    assert!(ProcessStatusFilter::Running.matches("R"));
    assert!(ProcessStatusFilter::Running.matches("r"));
    assert!(ProcessStatusFilter::Running.matches("running"));
    assert!(ProcessStatusFilter::Running.matches("RUNNING"));
    assert!(ProcessStatusFilter::Running.matches("  Running"));
    assert!(ProcessStatusFilter::Running.matches("\tr"));
    // A multi-word status whose first significant word starts with R also buckets here.
    assert!(ProcessStatusFilter::Running.matches("Running (cpu)"));
}

#[test]
fn status_filter_sleeping_bucket() {
    assert!(ProcessStatusFilter::Sleeping.matches("Sleeping"));
    assert!(ProcessStatusFilter::Sleeping.matches("S"));
    assert!(ProcessStatusFilter::Sleeping.matches("s"));
    assert!(ProcessStatusFilter::Sleeping.matches("SLEEPING"));
    assert!(ProcessStatusFilter::Sleeping.matches("  sleeping"));
}

#[test]
fn status_filter_stopped_bucket() {
    // classify() matches the full word "Stopped" AND any status whose first
    // letter is 't' (the /proc state letter "T", plus "Traced"/"tracing stop")
    // — all land in the Stopped bucket.
    assert!(ProcessStatusFilter::Stopped.matches("T"));
    assert!(ProcessStatusFilter::Stopped.matches("t"));
    assert!(ProcessStatusFilter::Stopped.matches("Traced"));
    assert!(ProcessStatusFilter::Stopped.matches(" traced"));
    assert!(ProcessStatusFilter::Stopped.matches("tracing stop"));
}

#[test]
fn status_filter_stopped_word_buckets_to_stopped() {
    // classify() matches the FULL WORD "Stopped" first (the data layer emits
    // ProcessStatus::Stop as "Stopped"), so it lands in the Stopped bucket —
    // NOT Sleeping, despite the first letter being 'S'.
    assert!(ProcessStatusFilter::Stopped.matches("Stopped"));
    assert!(ProcessStatusFilter::Stopped.matches("STOPPED"));
    assert!(ProcessStatusFilter::Stopped.matches("stopped"));
    // ...and it does NOT match the Sleeping bucket.
    assert!(!ProcessStatusFilter::Sleeping.matches("Stopped"));
    assert!(!ProcessStatusFilter::Sleeping.matches("STOPPED"));
}

#[test]
fn status_filter_zombie_bucket() {
    assert!(ProcessStatusFilter::Zombie.matches("Zombie"));
    assert!(ProcessStatusFilter::Zombie.matches("Z"));
    assert!(ProcessStatusFilter::Zombie.matches("z"));
    assert!(ProcessStatusFilter::Zombie.matches("ZOMBIE"));
    assert!(ProcessStatusFilter::Zombie.matches("  zombie"));
}

#[test]
fn status_filter_other_bucket_catches_unknown_letters_and_empty() {
    // Any first letter outside R/S/T/Z, plus empty / whitespace-only strings,
    // all classify to Other.
    assert!(ProcessStatusFilter::Other.matches("X"));
    assert!(ProcessStatusFilter::Other.matches("?weird"));
    assert!(ProcessStatusFilter::Other.matches("Disk sleep")); // 'D'
    assert!(ProcessStatusFilter::Other.matches("Idle")); // 'I'
    assert!(ProcessStatusFilter::Other.matches(""));
    assert!(ProcessStatusFilter::Other.matches("   ")); // trims to empty → no first char
    assert!(ProcessStatusFilter::Other.matches("\t\n"));
}

#[test]
fn status_filter_cross_bucket_negatives() {
    // A status that classifies to one bucket must not match a different bucket.
    assert!(!ProcessStatusFilter::Sleeping.matches("Running"));
    assert!(!ProcessStatusFilter::Stopped.matches("Running"));
    assert!(!ProcessStatusFilter::Zombie.matches("Running"));
    assert!(!ProcessStatusFilter::Other.matches("Running"));

    assert!(!ProcessStatusFilter::Running.matches("Sleeping"));
    assert!(!ProcessStatusFilter::Running.matches("S"));
    assert!(!ProcessStatusFilter::Running.matches("T"));
    assert!(!ProcessStatusFilter::Running.matches("Z"));

    // Empty / unknown statuses classify to Other, so they match ONLY Other + All.
    assert!(!ProcessStatusFilter::Running.matches(""));
    assert!(!ProcessStatusFilter::Running.matches("   "));
    assert!(!ProcessStatusFilter::Running.matches("Disk sleep"));
    assert!(!ProcessStatusFilter::Sleeping.matches("Disk sleep"));
    assert!(!ProcessStatusFilter::Stopped.matches("Disk sleep"));
    assert!(!ProcessStatusFilter::Zombie.matches("Disk sleep"));
}

#[test]
fn status_filter_label_id_and_all_constant() {
    // The pill row is rendered from ALL; its length + labels are part of the UI
    // contract, so pin them. Labels are localized through `i18n::t`, so the
    // contract asserts the en catalog (the fallback locale) explicitly.
    taskmanager::i18n::set_language(taskmanager::i18n::Language::En);
    assert_eq!(ProcessStatusFilter::All.label(), "All");
    assert_eq!(ProcessStatusFilter::Running.label(), "Running");
    assert_eq!(ProcessStatusFilter::Sleeping.label(), "Sleeping");
    assert_eq!(ProcessStatusFilter::Stopped.label(), "Stopped");
    assert_eq!(ProcessStatusFilter::Zombie.label(), "Zombie");
    assert_eq!(ProcessStatusFilter::Other.label(), "Other");

    // The zh catalog translates every pill label (a missing key falls back to
    // the key literal, which would break the pill row in the zh UI).
    taskmanager::i18n::set_language(taskmanager::i18n::Language::Zh);
    assert_eq!(ProcessStatusFilter::All.label(), "全部");
    assert_eq!(ProcessStatusFilter::Running.label(), "运行中");
    assert_eq!(ProcessStatusFilter::Sleeping.label(), "睡眠");
    assert_eq!(ProcessStatusFilter::Stopped.label(), "已停止");
    assert_eq!(ProcessStatusFilter::Zombie.label(), "僵尸");
    assert_eq!(ProcessStatusFilter::Other.label(), "其他");

    // Each variant's id() is a distinct stable string (the Hover::Static
    // discriminator relies on uniqueness).
    let ids: Vec<&'static str> = ProcessStatusFilter::ALL.iter().map(|f| f.key()).collect();
    let uniq: HashSet<&str> = ids.iter().copied().collect();
    assert_eq!(
        ids.len(),
        uniq.len(),
        "ProcessStatusFilter ids must be distinct: {ids:?}"
    );
}

// ── shared query grammar (flat-list helper over matches_process_query) ──

#[test]
fn query_empty_returns_all_in_input_order() {
    let procs = sample();
    let want: Vec<u32> = procs.iter().map(|p| p.pid).collect();
    let got: Vec<u32> = query_filter(&refs(&procs), "")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(got, want);

    // Whitespace-only query trims to empty → same fast path.
    let got: Vec<u32> = query_filter(&refs(&procs), "   ")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(got, want);
}

#[test]
fn query_substring_is_case_insensitive_on_name() {
    let procs = sample();
    let got: Vec<u32> = query_filter(&refs(&procs), "alph")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(got, vec![22]);
    // Case-folding: uppercase query still hits the lowercased name.
    let got: Vec<u32> = query_filter(&refs(&procs), "ALPH")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(got, vec![22]);
    // Mixed-case source data would be matched case-insensitively too.
    let got: Vec<u32> = query_filter(&refs(&procs), "ECH")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(got, vec![33]);
}

#[test]
fn query_matches_pid_digits() {
    let procs = sample();
    let got: Vec<u32> = query_filter(&refs(&procs), "33")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(got, vec![33]);
    // "3" is a substring of both "33" and "300"-ish... in sample() only pid 33 contains "3"
    // as a substring (pids are 11,22,33,44,55). Verify the exact set.
    let got: Vec<u32> = query_filter(&refs(&procs), "3")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(got, vec![33]);
}

#[test]
fn query_no_match_returns_empty() {
    let procs = sample();
    assert!(query_filter(&refs(&procs), "zzz-no-such").is_empty());
}

// ── typed observations drive visible rows ─────────────────────────────────

use taskmanager::core::process::ProcessScalarObservations;
use taskmanager::core::{FailureKind, ScalarObservation};

/// Build a process whose typed observations carry either current values or an
/// explicit unavailable state.
#[allow(clippy::too_many_arguments)]
fn typed_item(
    pid: u32,
    name: &str,
    cpu: Option<f32>,
    threads: Option<u32>,
    fds: Option<u32>,
    nice: Option<i32>,
    cpu_time: Option<u64>,
    mem: Option<u64>,
) -> ProcessItem {
    let observation = |value: Option<u64>| match value {
        Some(value) => ScalarObservation::available(value, 10),
        None => ScalarObservation::unavailable(FailureKind::PermissionDenied),
    };
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        // Keep every typed fixture as an orphan root so this helper's sort
        // tests exercise one sibling level instead of an accidental pid-1
        // parent/child hierarchy.
        .parent_pid(Some(10_000))
        .name(name.to_string())
        .scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(7_500, 10),
            cpu_percentage: match cpu {
                Some(value) => ScalarObservation::available(value, 10),
                None => ScalarObservation::unavailable(FailureKind::PermissionDenied),
            },
            memory_bytes: observation(mem),
            threads: match threads {
                Some(value) => ScalarObservation::available(value, 10),
                None => ScalarObservation::unavailable(FailureKind::PermissionDenied),
            },
            fds: match fds {
                Some(value) => ScalarObservation::available(value, 10),
                None => ScalarObservation::unavailable(FailureKind::PermissionDenied),
            },
            nice: match nice {
                Some(value) => ScalarObservation::available(value, 10),
                None => ScalarObservation::unavailable(FailureKind::PermissionDenied),
            },
            cpu_time_secs: observation(cpu_time),
            ..Default::default()
        })
        .build()
}

#[test]
fn visible_rows_carry_canonical_typed_observations() {
    let procs = vec![
        typed_item(
            1,
            "typed",
            Some(5.0),
            Some(4),
            None,
            Some(-3),
            Some(120),
            Some(200),
        ),
        typed_item(2, "denied", None, None, Some(7), None, None, None),
    ];

    let rows = canonical_process_rows(&refs(&procs), SortCol::Name, true, &HashSet::new());
    let typed = rows.iter().find(|r| r.pid == 1).expect("typed row");
    assert_eq!(typed.cpu, Some(5.0));
    assert_eq!(typed.mem, Some(200));
    assert_eq!(typed.threads, Some(4));
    assert_eq!(typed.fds, None);
    assert_eq!(typed.nice, Some(-3));
    assert_eq!(typed.cpu_time_secs, Some(120));

    let denied = rows.iter().find(|r| r.pid == 2).expect("denied row");
    assert_eq!(denied.cpu, None);
    assert_eq!(denied.threads, None);
    assert_eq!(denied.mem, None);
    assert_eq!(denied.cpu_time_secs, None);
    assert_eq!(denied.nice, None);
    assert_eq!(denied.fds, Some(7));
}

#[test]
fn typed_sorting_places_unavailable_values_first_ascending_and_last_descending() {
    let procs = vec![
        typed_item(
            1,
            "a",
            Some(5.0),
            Some(4),
            None,
            Some(-3),
            Some(120),
            Some(200),
        ),
        typed_item(2, "b", None, None, None, None, None, None),
        typed_item(
            3,
            "c",
            Some(1.0),
            Some(1),
            None,
            Some(0),
            Some(60),
            Some(100),
        ),
    ];

    let asc_cpu: Vec<u32> =
        canonical_process_rows(&refs(&procs), SortCol::Cpu, true, &HashSet::new())
            .iter()
            .map(|r| r.pid)
            .collect();
    // None (unavailable) sorts before every measured value; ties stable.
    assert_eq!(asc_cpu, vec![2, 3, 1]);
    let desc_cpu: Vec<u32> =
        canonical_process_rows(&refs(&procs), SortCol::Cpu, false, &HashSet::new())
            .iter()
            .map(|r| r.pid)
            .collect();
    assert_eq!(desc_cpu, vec![1, 3, 2]);

    let asc_threads: Vec<u32> =
        canonical_process_rows(&refs(&procs), SortCol::Threads, true, &HashSet::new())
            .iter()
            .map(|r| r.pid)
            .collect();
    assert_eq!(asc_threads, vec![2, 3, 1]);
}

#[test]
fn canonical_category_root_sums_only_available_members() {
    let procs = vec![
        typed_item(
            11,
            "same-app",
            Some(10.0),
            Some(3),
            None,
            Some(-5),
            Some(30),
            Some(100),
        ),
        typed_item(
            12,
            "same-app",
            Some(20.0),
            None,
            Some(7),
            None,
            None,
            Some(200),
        ),
    ];

    let rows = category_tree_rows(
        &refs(&procs),
        SortCol::Name,
        true,
        &default_category_expansions(),
        &HashSet::new(),
    );
    let aggregate = rows
        .iter()
        .find(|row| row.depth == 0 && row.process_pid.is_none())
        .expect("category aggregate row");
    assert_eq!(aggregate.cpu, Some(30.0));
    assert_eq!(aggregate.mem, Some(300));
    assert_eq!(aggregate.threads, Some(3));
    assert_eq!(aggregate.fds, Some(7));
    assert_eq!(aggregate.cpu_time_secs, Some(30));
    assert_eq!(aggregate.nice, Some(-5));

    let instances: Vec<_> = rows
        .iter()
        .filter(|row| row.depth == 1 && row.process_pid.is_some())
        .collect();
    assert_eq!(instances.len(), 2);
    assert_eq!(instances[0].threads, Some(3));
    assert_eq!(instances[0].fds, None);
    assert_eq!(instances[1].threads, None);
    assert_eq!(instances[1].fds, Some(7));
}

/// `visible_rows` precomputes each row's search-highlight ranges into
/// `VisibleRow::name_highlights` so the per-row render replays them instead of
/// re-running the shared match engine on every repaint. The ranges must use
/// the SAME engine and trimmed-query semantics the per-frame path used, and
/// an empty/whitespace query must leave them empty (the plain-text fast path).
#[test]
fn visible_rows_precomputes_name_highlight_ranges_for_the_active_query() {
    use std::collections::HashSet;

    use taskmanager_gpui::gpui_app::processes_view::{VisibleRowsProps, visible_rows};

    let procs = sample();
    let proc_refs = refs(&procs);
    let empty_collapsed = HashSet::new();
    let expanded = default_category_expansions();
    fn props<'a>(
        query: &'a str,
        processes: &'a [&'a ProcessItem],
        collapsed: &'a HashSet<u32>,
        expanded_apps: &'a HashSet<String>,
    ) -> VisibleRowsProps<'a> {
        VisibleRowsProps {
            processes,
            query,
            sort_col: SortCol::Name,
            sort_asc: true,
            filter: ProcessStatusFilter::All,
            collapsed,
            expanded_apps,
        }
    }

    // "alph" keeps only "alpha" through the filter, highlighted exactly.
    // (The shared grammar's bare fallback also searches user/cmdline, so the
    // shorter "al" would additionally match user "alice"/"dale" — the name
    // isolation case needs a name-only fragment.)
    let rows = visible_rows(props("  alph  ", &proc_refs, &empty_collapsed, &expanded));
    let alpha = rows
        .iter()
        .find(|row| row.process_pid.is_some())
        .expect("filtered canonical process row");
    assert_eq!(
        rows.len(),
        2,
        "the matching process keeps its structural category header"
    );
    assert_eq!(alpha.name, "alpha");
    assert_eq!(alpha.name_highlights, vec![0..4]);

    // A case-flipped query fragment highlights through the shared engine.
    let rows = visible_rows(props("HaR", &proc_refs, &empty_collapsed, &expanded));
    let charlie = rows
        .iter()
        .find(|row| row.process_pid.is_some())
        .expect("filtered canonical process row");
    assert_eq!(rows.len(), 2);
    assert_eq!(charlie.name, "charlie");
    assert_eq!(
        charlie.name_highlights,
        vec![1..4],
        "the range is a byte offset into the original name"
    );
    let rows = visible_rows(props("charlie", &proc_refs, &empty_collapsed, &expanded));
    assert_eq!(
        rows.iter()
            .find(|row| row.process_pid.is_some())
            .expect("filtered canonical process row")
            .name_highlights,
        vec![0..7],
        "a full-name match highlights the entire name"
    );

    // Empty and whitespace-only queries take the plain-text path: no ranges
    // and no filtering.
    for query in ["", "   "] {
        let rows = visible_rows(props(query, &proc_refs, &empty_collapsed, &expanded));
        assert_eq!(rows.len(), 6, "query {query:?} must not filter");
        assert!(
            rows.iter().all(|row| row.name_highlights.is_empty()),
            "query {query:?} must not precompute ranges"
        );
    }
}
