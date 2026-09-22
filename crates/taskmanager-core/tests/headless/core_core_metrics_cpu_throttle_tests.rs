use super::*;

/// One logical CPU's counter read for a package/core pair.
fn sample(
    package_id: u32,
    core_id: Option<u32>,
    package_count: Option<u64>,
    core_count: Option<u64>,
) -> CpuThrottleCounterSample {
    CpuThrottleCounterSample {
        package_id,
        core_id,
        package_throttle_count: package_count,
        core_throttle_count: core_count,
    }
}

#[test]
fn sibling_hyperthreads_are_deduplicated_by_physical_core() {
    // Core 0 is exposed on two logical CPUs with the same kernel counter; the
    // package counter repeats on every logical CPU.
    let rows = aggregate_package_throttle_counters(&[
        sample(0, Some(0), Some(7), Some(3)),
        sample(0, Some(0), Some(7), Some(3)),
        sample(0, Some(1), Some(7), Some(4)),
    ]);

    assert_eq!(
        rows,
        vec![CpuThrottlePackageCounters {
            package_id: 0,
            package_throttle_count: Some(7),
            core_throttle_count: Some(7),
        }],
        "a sibling must not double-count its physical core's events"
    );
}

#[test]
fn a_readable_sibling_supplies_the_physical_core_value() {
    let rows = aggregate_package_throttle_counters(&[
        sample(0, Some(0), Some(7), None),
        sample(0, Some(0), Some(7), Some(3)),
    ]);

    assert_eq!(rows[0].core_throttle_count, Some(3));
    assert_eq!(rows[0].package_throttle_count, Some(7));
}

#[test]
fn package_counter_is_the_maximum_and_core_counter_is_the_sum() {
    let rows = aggregate_package_throttle_counters(&[
        sample(1, Some(0), Some(5), Some(1)),
        sample(1, Some(1), Some(9), Some(2)),
    ]);

    assert_eq!(
        rows,
        vec![CpuThrottlePackageCounters {
            package_id: 1,
            package_throttle_count: Some(9),
            core_throttle_count: Some(3),
        }]
    );
}

#[test]
fn unknown_physical_core_contributes_only_its_package_counter() {
    let rows = aggregate_package_throttle_counters(&[
        sample(0, None, Some(4), Some(99)),
        sample(0, Some(2), Some(4), Some(2)),
    ]);

    assert_eq!(rows[0].package_throttle_count, Some(4));
    assert_eq!(
        rows[0].core_throttle_count,
        Some(2),
        "an unattributable core read must not enter a core sum"
    );
}

#[test]
fn unobserved_counters_stay_absent_on_both_carriers() {
    let rows = aggregate_package_throttle_counters(&[
        sample(0, Some(0), None, None),
        sample(2, None, None, None),
    ]);

    assert_eq!(rows.len(), 2, "a row exists for every observed package");
    assert_eq!(rows[0].package_throttle_count, None);
    assert_eq!(rows[0].core_throttle_count, None);
    assert_eq!(rows[1].package_id, 2);
    assert_eq!(rows[1].package_throttle_count, None);
}

#[test]
fn packages_are_sorted_and_input_order_is_irrelevant() {
    let forward = aggregate_package_throttle_counters(&[
        sample(2, Some(0), Some(1), Some(1)),
        sample(0, Some(0), Some(2), Some(2)),
    ]);
    let reversed = aggregate_package_throttle_counters(&[
        sample(0, Some(0), Some(2), Some(2)),
        sample(2, Some(0), Some(1), Some(1)),
    ]);

    assert_eq!(forward, reversed);
    assert_eq!(
        forward.iter().map(|row| row.package_id).collect::<Vec<_>>(),
        vec![0, 2]
    );
}

#[test]
fn counter_sum_saturates_instead_of_wrapping() {
    let rows = aggregate_package_throttle_counters(&[
        sample(0, Some(0), Some(0), Some(u64::MAX)),
        sample(0, Some(1), Some(0), Some(u64::MAX)),
    ]);

    assert_eq!(rows[0].core_throttle_count, Some(u64::MAX));
}

#[test]
fn snapshot_success_and_failure_are_typed() {
    let success = CpuThrottleSnapshot::success(vec![CpuThrottlePackageCounters {
        package_id: 0,
        package_throttle_count: Some(3),
        core_throttle_count: None,
    }]);
    assert!(success.is_success());
    assert_eq!(success.packages.len(), 1);

    let failed = CpuThrottleSnapshot::failed(FailureKind::Unsupported, "no counters");
    assert!(!failed.is_success());
    assert!(failed.packages.is_empty());
    assert_eq!(
        failed.failure.as_ref().map(|failure| failure.kind),
        Some(FailureKind::Unsupported)
    );
}

#[test]
fn snapshot_round_trips_without_inventing_absent_counters() {
    let snapshot = CpuThrottleSnapshot::success(vec![CpuThrottlePackageCounters {
        package_id: 1,
        package_throttle_count: Some(7),
        core_throttle_count: None,
    }]);

    let encoded = serde_json::to_value(&snapshot).expect("serialize");
    assert_eq!(encoded["packages"][0]["package_throttle_count"], 7);
    assert!(encoded["packages"][0].get("core_throttle_count").is_none());
    assert!(encoded.get("failure").is_none());

    let decoded: CpuThrottleSnapshot = serde_json::from_value(encoded).expect("deserialize");
    assert_eq!(decoded, snapshot);
}
