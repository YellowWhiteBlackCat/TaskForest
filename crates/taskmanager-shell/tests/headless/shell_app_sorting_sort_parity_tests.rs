use super::{ShellApp, SortCol, SortDir};
use crate::sort_axis;
use taskmanager_application::process_sort::{ProcessSortAxis, compare_processes};
use taskmanager_test_support::{SortFixtureMetrics, sort_fixture_row, sort_parity_fixture};

fn neutral_pids(
    items: &[taskmanager_application::ProcessItem],
    axis: ProcessSortAxis,
    ascending: bool,
) -> Vec<u32> {
    let mut sorted: Vec<&taskmanager_application::ProcessItem> = items.iter().collect();
    sorted.sort_by(|left, right| compare_processes(left, right, axis, ascending));
    sorted.iter().map(|process| process.pid).collect()
}

/// Every shell column × both directions: `visible_processes` order ≡ the
/// neutral comparator's order on the same fixture.
#[test]
fn visible_processes_match_the_neutral_comparator_on_every_column() {
    let items = sort_parity_fixture();
    for column in SortCol::ALL {
        for direction in [SortDir::Asc, SortDir::Desc] {
            let mut app = ShellApp::new();
            app.data.processes = Some(items.clone());
            app.process_sort = (column, direction);
            let shell_pids: Vec<u32> = app
                .visible_processes()
                .iter()
                .map(|process| process.pid)
                .collect();
            let want = neutral_pids(&items, sort_axis(column), direction == SortDir::Asc);
            assert_eq!(
                shell_pids, want,
                "{column:?} {direction:?} must match the neutral order"
            );
        }
    }
}

/// Absolute convergence pins (independent of the neutral module): the
/// delegation must reach `visible_processes` with the honest semantics —
/// unavailable values sink below measured ones in descending sorts,
/// missing data sorts first ascending, and a case-folded name tie breaks
/// pid-ascending in the descending direction.
#[test]
fn visible_processes_pin_absolute_converged_orders() {
    let items = sort_parity_fixture();
    let mut app = ShellApp::new();
    app.data.processes = Some(items);

    app.process_sort = (SortCol::Cpu, SortDir::Desc);
    let pids: Vec<u32> = app.visible_processes().iter().map(|row| row.pid).collect();
    assert_eq!(
        pids,
        [11, 12, 13, 14],
        "cpu desc: measured tie by pid, unavailable last"
    );

    app.process_sort = (SortCol::Memory, SortDir::Asc);
    let pids: Vec<u32> = app.visible_processes().iter().map(|row| row.pid).collect();
    assert_eq!(
        pids,
        [14, 13, 12, 11],
        "memory asc: unavailable first, typed RSS values"
    );

    app.process_sort = (SortCol::Name, SortDir::Desc);
    let pids: Vec<u32> = app.visible_processes().iter().map(|row| row.pid).collect();
    assert_eq!(
        pids,
        [14, 13, 11, 12],
        "name desc: \"Alpha\"/\"alpha\" fold equal and tie-break pid-ascending"
    );

    app.process_sort = (SortCol::StartTime, SortDir::Asc);
    let pids: Vec<u32> = app.visible_processes().iter().map(|row| row.pid).collect();
    assert_eq!(
        pids,
        [14, 11, 12, 13],
        "start asc: unavailable first (never a fabricated 0), tie by pid"
    );
}

/// A test-local variance fixture for the tie/gap shapes the shared
/// fixture does not carry: one name shared by a measured row, a
/// differently-measured row, and an all-metrics-unavailable row
/// ("dupe" = 21/22/24, distinct cpu values); the same cpu value under
/// two different names (21/23 at 7.0); an EMPTY user (22 — `user` is a
/// plain string, so the empty string is the missing-user spelling); and
/// start-time (21/22) plus threads (21/23) ties crossing the rows.
fn variance_fixture() -> Vec<taskmanager_application::ProcessItem> {
    vec![
        sort_fixture_row(
            21,
            "dupe",
            "root",
            "S",
            SortFixtureMetrics {
                cpu: Some(7.0),
                rss: Some(200),
                pss: Some(100),
                swap: Some(16),
                threads: Some(3),
                cpu_time: Some(200),
                disk_read: Some(100),
                disk_write: Some(50),
                start_time: Some(5_000_000),
                fds: Some(12),
                nice: Some(0),
            },
        ),
        sort_fixture_row(
            22,
            "dupe",
            "",
            "S",
            SortFixtureMetrics {
                cpu: Some(3.0),
                rss: Some(100),
                threads: Some(1),
                cpu_time: Some(100),
                disk_read: Some(200),
                disk_write: Some(25),
                start_time: Some(5_000_000),
                fds: Some(6),
                nice: Some(5),
                ..SortFixtureMetrics::default()
            },
        ),
        sort_fixture_row(
            23,
            "other",
            "root",
            "R",
            SortFixtureMetrics {
                cpu: Some(7.0),
                rss: Some(300),
                pss: Some(400),
                swap: Some(32),
                threads: Some(3),
                cpu_time: Some(300),
                disk_read: Some(0),
                disk_write: Some(0),
                start_time: Some(6_000_000),
                fds: Some(9),
                nice: Some(-5),
            },
        ),
        sort_fixture_row(24, "dupe", "root", "S", SortFixtureMetrics::default()),
    ]
}

/// Every shell column × both directions on the variance fixture: the
/// same-name / same-cpu / empty-user ties and the all-missing row must
/// still compose to exactly the neutral comparator's order.
#[test]
fn visible_processes_match_the_neutral_comparator_on_the_variance_fixture() {
    let items = variance_fixture();
    for column in SortCol::ALL {
        for direction in [SortDir::Asc, SortDir::Desc] {
            let mut app = ShellApp::new();
            app.data.processes = Some(items.clone());
            app.process_sort = (column, direction);
            let shell_pids: Vec<u32> = app
                .visible_processes()
                .iter()
                .map(|process| process.pid)
                .collect();
            let want = neutral_pids(&items, sort_axis(column), direction == SortDir::Asc);
            assert_eq!(
                shell_pids, want,
                "{column:?} {direction:?} must match the neutral order on the variance fixture"
            );
        }
    }
}

/// Absolute pins for the variance semantics, independent of the neutral
/// module: the same-cpu tie stays pid-ascending in BOTH directions with
/// the missing-cpu row sinking last descending; the folded name tie
/// keeps pid order under a descending sort; the empty user sorts first
/// ascending.
#[test]
fn variance_fixture_pins_tie_missing_and_empty_user_semantics() {
    let items = variance_fixture();
    let mut app = ShellApp::new();
    app.data.processes = Some(items);

    app.process_sort = (SortCol::Cpu, SortDir::Desc);
    let pids: Vec<u32> = app.visible_processes().iter().map(|row| row.pid).collect();
    assert_eq!(
        pids,
        [21, 23, 22, 24],
        "cpu desc: the 7.0 tie stays pid-ascending, the missing row sinks last"
    );

    app.process_sort = (SortCol::Cpu, SortDir::Asc);
    let pids: Vec<u32> = app.visible_processes().iter().map(|row| row.pid).collect();
    assert_eq!(
        pids,
        [24, 22, 21, 23],
        "cpu asc: missing first, then 3.0, then the 7.0 tie in pid order"
    );

    app.process_sort = (SortCol::Name, SortDir::Desc);
    let pids: Vec<u32> = app.visible_processes().iter().map(|row| row.pid).collect();
    assert_eq!(
        pids,
        [23, 21, 22, 24],
        "name desc: \"other\" first, the folded \"dupe\" tie keeps pid-ascending"
    );

    app.process_sort = (SortCol::User, SortDir::Asc);
    let pids: Vec<u32> = app.visible_processes().iter().map(|row| row.pid).collect();
    assert_eq!(
        pids,
        [22, 21, 23, 24],
        "user asc: the empty user sorts first, the folded \"root\" tie keeps pid order"
    );
}
