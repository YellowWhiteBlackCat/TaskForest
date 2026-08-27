use super::*;

#[test]
fn scan_id_is_monotonic_and_never_overflows_silently() {
    assert_eq!(DirectoryScanId::default().get(), 0);
    let id = DirectoryScanId::new(7);
    assert_eq!(id.checked_next(), Some(DirectoryScanId::new(8)));
    assert_eq!(
        DirectoryScanId::new(u64::MAX).checked_next(),
        None,
        "overflow must be explicit"
    );
}

#[test]
fn bounds_are_hardened_against_hostile_requests() {
    assert_eq!(
        DirectoryScanBounds {
            max_depth: 0,
            max_entries: 0,
            max_reported: 0,
        }
        .hardened(),
        DirectoryScanBounds {
            max_depth: 1,
            max_entries: 1,
            max_reported: 1,
        }
    );
    assert_eq!(
        DirectoryScanBounds {
            max_depth: u32::MAX,
            max_entries: u64::MAX,
            max_reported: usize::MAX,
        }
        .hardened(),
        DirectoryScanBounds {
            max_depth: MAX_DIRECTORY_SCAN_DEPTH,
            max_entries: MAX_DIRECTORY_SCAN_ENTRIES,
            max_reported: MAX_DIRECTORY_SCAN_REPORTED,
        }
    );
    assert_eq!(
        DirectoryScanBounds::default().hardened(),
        DirectoryScanBounds::default(),
        "the default policy must already be inside the ceilings"
    );
}

#[test]
fn totals_keep_measured_zero_distinct_from_unreadable() {
    let mut totals = DirectoryScanTotals::fresh(10);
    assert!(totals.record_file(0, 100), "measured zero is a real count");
    assert_eq!(totals.files_counted, 1);
    assert_eq!(totals.bytes_counted.current_value(), Some(&0));
    assert_eq!(
        totals.bytes_counted.availability(),
        crate::core::metrics::ScalarAvailability::Available,
        "an empty-but-readable file set must stay current zero"
    );

    totals.record_unreadable(FailureKind::PermissionDenied);
    assert_eq!(totals.unreadable_directories, 1);
    let current = totals.bytes_counted.current_value();
    assert_eq!(current, Some(&0));
    assert_eq!(
        totals.bytes_counted.availability(),
        crate::core::metrics::ScalarAvailability::Partial(FailureKind::PermissionDenied),
        "an unreadable subtree must mark the sum partial, not complete"
    );
}

#[test]
fn entry_cap_stops_counting_instead_of_overflowing() {
    let mut totals = DirectoryScanTotals::fresh(10);
    assert!(totals.record_file(1, 2));
    assert!(totals.record_directory(2));
    assert!(
        !totals.record_file(1, 2),
        "a third entry must stop at the cap, not fabricate"
    );
    assert_eq!(totals.files_counted, 1);
    assert_eq!(totals.directories_visited, 1);
}

#[test]
fn unreadable_marking_is_monotonic_and_keeps_the_strongest_failure() {
    let mut totals = DirectoryScanTotals::fresh(10);
    totals.record_unreadable(FailureKind::TimedOut);
    totals.record_unreadable(FailureKind::PermissionDenied);
    assert_eq!(
        totals.bytes_counted.availability(),
        crate::core::metrics::ScalarAvailability::Partial(FailureKind::PermissionDenied),
        "PermissionDenied must outrank TimedOut"
    );
}

#[test]
fn failure_priority_table_is_total() {
    assert_eq!(
        stronger_of(FailureKind::Rejected, FailureKind::Unsupported),
        FailureKind::Unsupported
    );
    assert_eq!(
        stronger_of(FailureKind::Unsupported, FailureKind::RequiresEscalation),
        FailureKind::RequiresEscalation
    );
    assert_eq!(
        stronger_of(FailureKind::TimedOut, FailureKind::TimedOut),
        FailureKind::TimedOut
    );
}

#[test]
fn report_entries_sort_largest_first_then_path_and_cap() {
    let entry = |path: &str, size: u64| DirectoryUsageEntry {
        path: path.to_string(),
        depth: 1,
        size_bytes: ScalarObservation::available(size, 10),
        file_count: ScalarObservation::available(1, 10),
        unreadable: None,
    };
    let reported = report_entries(
        vec![
            entry("a", 5),
            entry("b", 100),
            entry("c", 5),
            entry("d", 50),
        ],
        3,
    );
    assert_eq!(
        reported.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
        ["b", "d", "a"],
        "largest first; size ties break on path, and the cap drops the tail"
    );
}

#[test]
fn unreadable_entries_never_outrank_measured_ones_in_the_report() {
    let entry = |path: &str, size: ScalarObservation<u64>, unreadable| DirectoryUsageEntry {
        path: path.to_string(),
        depth: 1,
        size_bytes: size,
        file_count: ScalarObservation::available(1, 10),
        unreadable,
    };
    let reported = report_entries(
        vec![
            entry("big", ScalarObservation::available(1_000, 10), None),
            entry(
                "denied",
                ScalarObservation::unavailable(FailureKind::PermissionDenied),
                Some(FailureKind::PermissionDenied),
            ),
        ],
        2,
    );
    assert_eq!(reported[0].path, "big");
    assert_eq!(reported[1].path, "denied");
}
