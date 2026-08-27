use super::{ProcessSortAxis, compare_axis, compare_processes};
use std::cmp::Ordering;
use taskmanager_test_support::{SortFixtureMetrics, sort_fixture_row, sort_parity_fixture};

/// Pid order of the fixture sorted directly by the neutral comparator.
fn neutral_order(items: &[crate::ProcessItem], axis: ProcessSortAxis, ascending: bool) -> Vec<u32> {
    let mut sorted: Vec<&crate::ProcessItem> = items.iter().collect();
    sorted.sort_by(|left, right| compare_processes(left, right, axis, ascending));
    sorted.iter().map(|process| process.pid).collect()
}

/// The absolute order matrix on the shared fixture — every axis, both
/// directions, hand-computed expectations (ascending, descending). Ties
/// stay pid-ascending in BOTH directions; missing values (`None`) sort
/// first ascending / last descending; PSS order differs from RSS order.
#[test]
fn fixture_orders_match_the_documented_matrix_in_both_directions() {
    let items = sort_parity_fixture();
    let matrix: &[(ProcessSortAxis, &[u32], &[u32])] = &[
        (ProcessSortAxis::Pid, &[11, 12, 13, 14], &[14, 13, 12, 11]),
        (ProcessSortAxis::Name, &[11, 12, 13, 14], &[14, 13, 11, 12]),
        (ProcessSortAxis::Cpu, &[14, 13, 11, 12], &[11, 12, 13, 14]),
        (
            ProcessSortAxis::Memory,
            &[14, 13, 12, 11],
            &[11, 12, 13, 14],
        ),
        (ProcessSortAxis::Pss, &[12, 14, 11, 13], &[13, 11, 12, 14]),
        (ProcessSortAxis::Swap, &[12, 14, 13, 11], &[11, 13, 12, 14]),
        (ProcessSortAxis::User, &[13, 11, 12, 14], &[11, 12, 14, 13]),
        (
            ProcessSortAxis::Status,
            &[12, 11, 13, 14],
            &[14, 11, 13, 12],
        ),
        (
            ProcessSortAxis::Threads,
            &[14, 12, 11, 13],
            &[13, 11, 12, 14],
        ),
        (
            ProcessSortAxis::CpuTime,
            &[14, 12, 11, 13],
            &[13, 11, 12, 14],
        ),
        (
            ProcessSortAxis::DiskRead,
            &[14, 13, 11, 12],
            &[12, 11, 13, 14],
        ),
        (
            ProcessSortAxis::DiskWrite,
            &[14, 13, 12, 11],
            &[11, 12, 13, 14],
        ),
        (
            ProcessSortAxis::StartTime,
            &[14, 11, 12, 13],
            &[13, 11, 12, 14],
        ),
        (ProcessSortAxis::Fds, &[14, 13, 12, 11], &[11, 12, 13, 14]),
        (ProcessSortAxis::Nice, &[14, 13, 11, 12], &[12, 11, 13, 14]),
    ];
    // The matrix enumerates every axis exactly once — extending ALL
    // without extending the matrix fails here, not silently in CI.
    assert_eq!(matrix.len(), ProcessSortAxis::ALL.len());
    for axis in ProcessSortAxis::ALL {
        assert!(
            matrix.iter().any(|(row_axis, ..)| *row_axis == axis),
            "axis {axis:?} missing from the order matrix"
        );
    }
    for (axis, ascending_expectation, descending_expectation) in matrix {
        assert_eq!(
            neutral_order(&items, *axis, true),
            *ascending_expectation,
            "{axis:?} ascending"
        );
        assert_eq!(
            neutral_order(&items, *axis, false),
            *descending_expectation,
            "{axis:?} descending"
        );
    }
}

/// Name ordering folds ASCII case: descending puts the case-folded tie
/// (Alpha/alpha) in pid order rather than byte-reversed case order —
/// the pair that distinguishes folding from a raw byte compare.
#[test]
fn name_and_user_compare_fold_ascii_case() {
    let items = sort_parity_fixture();
    let (alpha, alpha_lower) = (&items[0], &items[1]);
    assert_eq!(
        compare_axis(alpha, alpha_lower, ProcessSortAxis::Name),
        Ordering::Equal,
        "\"Alpha\" and \"alpha\" fold equal"
    );
    let upper = sort_fixture_row(50, "Beta", "root", "S", SortFixtureMetrics::default());
    let lower = sort_fixture_row(51, "beta", "root", "S", SortFixtureMetrics::default());
    assert_eq!(
        compare_processes(&upper, &lower, ProcessSortAxis::Name, false),
        Ordering::Less,
        "descending still breaks the folded tie pid-ascending"
    );
    assert_eq!(
        compare_axis(&upper, &lower, ProcessSortAxis::User),
        Ordering::Equal,
        "\"root\" and \"root\" fold equal (same string)"
    );
    let user_upper = sort_fixture_row(52, "b", "Root", "S", SortFixtureMetrics::default());
    let user_lower = sort_fixture_row(53, "b", "root", "S", SortFixtureMetrics::default());
    assert_eq!(
        compare_axis(&user_upper, &user_lower, ProcessSortAxis::User),
        Ordering::Equal,
        "\"Root\" and \"root\" fold equal on the User axis"
    );
}

/// Honesty pin: a missing observation never fabricates a 0 — `None`
/// sorts strictly before a measured `Some(0.0)` on every optional axis,
/// so a provider failure cannot masquerade as a measured zero.
#[test]
fn missing_values_never_sort_as_measured_zero() {
    let missing = sort_fixture_row(61, "missing", "root", "S", SortFixtureMetrics::default());
    let zeroed = sort_fixture_row(
        62,
        "zeroed",
        "root",
        "S",
        SortFixtureMetrics {
            cpu: Some(0.0),
            rss: Some(0),
            pss: Some(0),
            swap: Some(0),
            threads: Some(0),
            cpu_time: Some(0),
            disk_read: Some(0),
            disk_write: Some(0),
            start_time: Some(0),
            fds: Some(0),
            nice: Some(0),
        },
    );
    let optional_axes = [
        ProcessSortAxis::Cpu,
        ProcessSortAxis::Memory,
        ProcessSortAxis::Pss,
        ProcessSortAxis::Swap,
        ProcessSortAxis::Threads,
        ProcessSortAxis::CpuTime,
        ProcessSortAxis::DiskRead,
        ProcessSortAxis::DiskWrite,
        ProcessSortAxis::StartTime,
        ProcessSortAxis::Fds,
        ProcessSortAxis::Nice,
    ];
    for axis in optional_axes {
        assert_eq!(
            compare_axis(&missing, &zeroed, axis),
            Ordering::Less,
            "{axis:?}: None must sort before Some(0)"
        );
        assert_eq!(
            compare_processes(&missing, &zeroed, axis, false),
            Ordering::Greater,
            "{axis:?}: None must sink below Some(0) when descending"
        );
    }
}

/// NaN CPU percentages order deterministically (total order): NaN above
/// every finite value ascending, below it descending, equal to itself.
#[test]
fn nan_cpu_orders_deterministically_via_total_cmp() {
    let finite = sort_fixture_row(
        71,
        "finite",
        "root",
        "S",
        SortFixtureMetrics {
            cpu: Some(1.0),
            ..SortFixtureMetrics::default()
        },
    );
    let nan = sort_fixture_row(
        70,
        "nan",
        "root",
        "S",
        SortFixtureMetrics {
            cpu: Some(f32::NAN),
            ..SortFixtureMetrics::default()
        },
    );
    assert_eq!(
        compare_axis(&nan, &finite, ProcessSortAxis::Cpu),
        Ordering::Greater,
        "NaN sorts above every finite value ascending"
    );
    assert_eq!(
        compare_axis(&nan, &nan, ProcessSortAxis::Cpu),
        Ordering::Equal,
        "NaN equals NaN under total_cmp"
    );
    let order = neutral_order(&[finite.clone(), nan], ProcessSortAxis::Cpu, false);
    assert_eq!(order, &[70, 71], "descending puts the NaN row first");
}

/// The pid tie-break is direction-independent: equal primary values keep
/// pid-ascending order in BOTH directions (it never flips with the axis).
#[test]
fn pid_tiebreak_is_direction_independent() {
    let items = sort_parity_fixture();
    // pid 11 and 12 share cpu 5.0.
    let (low, high) = (&items[0], &items[1]);
    for ascending in [true, false] {
        assert_eq!(
            compare_processes(low, high, ProcessSortAxis::Cpu, ascending),
            Ordering::Less,
            "the cpu tie must stay pid-ascending (ascending={ascending})"
        );
    }
}

/// Identical elements compare Equal on every axis in both directions, so
/// a stable sort never reorders duplicates.
#[test]
fn identical_elements_compare_equal_on_every_axis_and_direction() {
    let item = sort_parity_fixture().remove(0);
    let twin = item.clone();
    for axis in ProcessSortAxis::ALL {
        for ascending in [true, false] {
            assert_eq!(
                compare_processes(&item, &twin, axis, ascending),
                Ordering::Equal,
                "{axis:?} ascending={ascending}"
            );
        }
    }
    let mut duplicates = [item.clone(), twin, item];
    duplicates.sort_by(|left, right| compare_processes(left, right, ProcessSortAxis::Cpu, false));
    assert_eq!(
        duplicates.iter().map(|row| row.pid).collect::<Vec<_>>(),
        vec![11, 11, 11],
        "stable sort keeps duplicate rows in place"
    );
}
