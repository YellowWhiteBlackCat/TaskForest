use super::*;
use crate::FailureKind;
use crate::core::metrics::ScalarObservation;
use crate::core::process::ProcessScalarObservations;

#[test]
fn process_memory_breakdown_computes_shared_bytes_and_ratios() {
    let rss = 100 * 1024 * 1024; // 100 MiB
    let pss = 60 * 1024 * 1024; // 60 MiB
    let uss = 40 * 1024 * 1024; // 40 MiB
    let swap = 10 * 1024 * 1024; // 10 MiB

    let breakdown = ProcessMemoryBreakdown::new(Some(rss), Some(pss), Some(uss), Some(swap));

    assert_eq!(breakdown.rss_bytes, Some(rss));
    assert_eq!(breakdown.pss_bytes, Some(pss));
    assert_eq!(breakdown.uss_bytes, Some(uss));
    assert_eq!(breakdown.shared_bytes, Some(60 * 1024 * 1024)); // 100 - 40
    assert_eq!(breakdown.swap_bytes, Some(swap));
    assert_eq!(
        breakdown.proportional_shared_bytes(),
        Some(20 * 1024 * 1024)
    ); // 60 - 40

    assert!(breakdown.has_uss());
    assert!(breakdown.has_pss());

    let uss_ratio = breakdown.uss_to_rss_ratio().expect("uss ratio");
    assert!((uss_ratio - 0.4).abs() < 1e-4);

    let pss_ratio = breakdown.pss_to_rss_ratio().expect("pss ratio");
    assert!((pss_ratio - 0.6).abs() < 1e-4);

    let shared_ratio = breakdown.shared_to_rss_ratio().expect("shared ratio");
    assert!((shared_ratio - 0.6).abs() < 1e-4);
}

#[test]
fn process_memory_breakdown_handles_missing_fields_gracefully() {
    let empty = ProcessMemoryBreakdown::default();
    assert_eq!(empty.rss_bytes, None);
    assert_eq!(empty.pss_bytes, None);
    assert_eq!(empty.uss_bytes, None);
    assert_eq!(empty.shared_bytes, None);
    assert_eq!(empty.swap_bytes, None);
    assert_eq!(empty.proportional_shared_bytes(), None);
    assert!(!empty.has_uss());
    assert!(!empty.has_pss());
    assert_eq!(empty.uss_to_rss_ratio(), None);
    assert_eq!(empty.pss_to_rss_ratio(), None);
    assert_eq!(empty.shared_to_rss_ratio(), None);

    // Only RSS, no USS -> shared_bytes is None
    let rss_only = ProcessMemoryBreakdown::new(Some(5000), None, None, None);
    assert_eq!(rss_only.shared_bytes, None);
    assert_eq!(rss_only.uss_to_rss_ratio(), None);
    assert_eq!(rss_only.shared_to_rss_ratio(), None);

    // Zero RSS -> ratios should return None rather than dividing by zero
    let zero_rss = ProcessMemoryBreakdown::new(Some(0), Some(0), Some(0), None);
    assert_eq!(zero_rss.uss_to_rss_ratio(), None);
    assert_eq!(zero_rss.pss_to_rss_ratio(), None);
    assert_eq!(zero_rss.shared_to_rss_ratio(), None);
}

#[test]
fn process_memory_breakdown_saturates_when_uss_exceeds_rss_or_pss() {
    // Unorthodox state (e.g. race condition between /proc sampling)
    let breakdown = ProcessMemoryBreakdown::new(Some(100), Some(150), Some(200), None);
    assert_eq!(breakdown.shared_bytes, Some(0));
    assert_eq!(breakdown.proportional_shared_bytes(), Some(0));
    assert_eq!(breakdown.uss_to_rss_ratio(), Some(1.0)); // clamped to 1.0
}

#[test]
fn process_memory_breakdown_serde_roundtrip() {
    let breakdown = ProcessMemoryBreakdown::new(Some(1024), Some(512), Some(256), Some(128));
    let serialized = serde_json::to_string(&breakdown).expect("serialize");
    let deserialized: ProcessMemoryBreakdown =
        serde_json::from_str(&serialized).expect("deserialize");
    assert_eq!(breakdown, deserialized);
}

#[test]
fn process_item_memory_accessors_and_breakdown() {
    let mut process = ProcessItem::new(1234, "test_proc");
    assert!(!process.has_memory_pss());
    assert!(!process.has_memory_uss());
    assert_eq!(process.memory_shared_bytes(), None);
    assert_eq!(process.memory_proportional_shared_bytes(), None);
    assert_eq!(process.memory_uss_ratio(), None);
    assert_eq!(process.current_memory_pss_ratio(), None);

    let empty_breakdown = process.memory_breakdown();
    assert_eq!(empty_breakdown.rss_bytes, None);
    assert_eq!(empty_breakdown.pss_bytes, None);
    assert_eq!(empty_breakdown.uss_bytes, None);
    assert_eq!(empty_breakdown.swap_bytes, None);

    // Apply memory observations
    let observations = ProcessScalarObservations {
        memory_bytes: ScalarObservation::available(100 * 1024 * 1024, 1000),
        memory_pss_bytes: ScalarObservation::available(70 * 1024 * 1024, 1000),
        memory_uss_bytes: ScalarObservation::available(50 * 1024 * 1024, 1000),
        swap_bytes: ScalarObservation::available(10 * 1024 * 1024, 1000),
        ..Default::default()
    };
    process.apply_scalar_observations(observations);

    assert!(process.has_memory_pss());
    assert!(process.has_memory_uss());
    assert_eq!(process.current_memory_bytes(), Some(100 * 1024 * 1024));
    assert_eq!(process.current_memory_pss_bytes(), Some(70 * 1024 * 1024));
    assert_eq!(process.memory_uss_bytes(), Some(50 * 1024 * 1024));
    assert_eq!(process.current_swap_bytes(), Some(10 * 1024 * 1024));

    assert_eq!(process.memory_shared_bytes(), Some(50 * 1024 * 1024));
    assert_eq!(
        process.memory_proportional_shared_bytes(),
        Some(20 * 1024 * 1024)
    );

    let uss_ratio = process.memory_uss_ratio().expect("uss ratio");
    assert!((uss_ratio - 0.5).abs() < 1e-4);

    let pss_ratio = process.current_memory_pss_ratio().expect("pss ratio");
    assert!((pss_ratio - 0.7).abs() < 1e-4);

    let breakdown = process.memory_breakdown();
    assert_eq!(breakdown.rss_bytes, Some(100 * 1024 * 1024));
    assert_eq!(breakdown.pss_bytes, Some(70 * 1024 * 1024));
    assert_eq!(breakdown.uss_bytes, Some(50 * 1024 * 1024));
    assert_eq!(breakdown.shared_bytes, Some(50 * 1024 * 1024));
    assert_eq!(breakdown.swap_bytes, Some(10 * 1024 * 1024));
}

#[test]
fn process_sort_key_memory_pss_and_uss_ordering() {
    let mut p1 = ProcessItem::new(1, "proc1");
    let mut p2 = ProcessItem::new(2, "proc2");
    let mut p3 = ProcessItem::new(3, "proc3");

    // p1: PSS 100 MiB, USS 80 MiB
    let o1 = ProcessScalarObservations {
        memory_pss_bytes: ScalarObservation::available(100 * 1024 * 1024, 1),
        memory_uss_bytes: ScalarObservation::available(80 * 1024 * 1024, 1),
        ..Default::default()
    };
    p1.apply_scalar_observations(o1);

    // p2: PSS 200 MiB, USS 30 MiB
    let o2 = ProcessScalarObservations {
        memory_pss_bytes: ScalarObservation::available(200 * 1024 * 1024, 1),
        memory_uss_bytes: ScalarObservation::available(30 * 1024 * 1024, 1),
        ..Default::default()
    };
    p2.apply_scalar_observations(o2);

    // p3: PSS None, USS None
    let o3 = ProcessScalarObservations::default();
    p3.apply_scalar_observations(o3);

    // Test compare_process_items with MemoryPss
    assert_eq!(
        compare_process_items(&p1, &p2, ProcessSortKey::MemoryPss),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        compare_process_items(&p2, &p1, ProcessSortKey::MemoryPss),
        std::cmp::Ordering::Greater
    );
    assert_eq!(
        compare_process_items(&p3, &p1, ProcessSortKey::MemoryPss),
        std::cmp::Ordering::Less // None < Some
    );

    // Test compare_process_items with MemoryUss
    assert_eq!(
        compare_process_items(&p1, &p2, ProcessSortKey::MemoryUss),
        std::cmp::Ordering::Greater // 80 > 30
    );
    assert_eq!(
        compare_process_items(&p2, &p1, ProcessSortKey::MemoryUss),
        std::cmp::Ordering::Less
    );

    // Test sort_processes with MemoryPss descending
    let mut items = vec![p1.clone(), p2.clone(), p3.clone()];
    sort_processes(&mut items, ProcessSortKey::MemoryPss, false);
    assert_eq!(items[0].pid, 2); // 200 MiB
    assert_eq!(items[1].pid, 1); // 100 MiB
    assert_eq!(items[2].pid, 3); // None

    // Test sort_processes with MemoryUss descending
    let mut items = vec![p1, p2, p3];
    sort_processes(&mut items, ProcessSortKey::MemoryUss, false);
    assert_eq!(items[0].pid, 1); // 80 MiB
    assert_eq!(items[1].pid, 2); // 30 MiB
    assert_eq!(items[2].pid, 3); // None
}

/// USS and the huge-page charge come from a decimated page-table walk, so
/// between samples the row carries a `Stale` observation. Every accessor and
/// every derived figure must read that retained measurement — the value beside
/// it on screen — instead of reporting an absence that would blank the row.
#[test]
fn decimated_memory_counters_keep_their_retained_value_and_derived_facts() {
    let mut process = ProcessItem::new(7, "decimated");
    let mib = 1024 * 1024;
    process.apply_scalar_observations(ProcessScalarObservations {
        memory_bytes: ScalarObservation::available(100 * mib, 1_000),
        memory_pss_bytes: ScalarObservation::available(70 * mib, 1_000),
        memory_uss_bytes: ScalarObservation::stale(
            50 * mib,
            900,
            FailureKind::TemporarilyUnavailable,
        ),
        memory_anon_huge_pages_bytes: ScalarObservation::stale(
            8 * mib,
            900,
            FailureKind::TemporarilyUnavailable,
        ),
        ..Default::default()
    });

    assert_eq!(process.memory_uss_bytes(), Some(50 * mib));
    assert_eq!(process.memory_anon_huge_pages_bytes(), Some(8 * mib));
    assert!(process.has_memory_uss());
    assert!(process.has_memory_anon_huge_pages());
    assert_eq!(process.memory_shared_bytes(), Some(50 * mib));
    assert_eq!(process.memory_proportional_shared_bytes(), Some(20 * mib));
    let uss_ratio = process
        .memory_uss_ratio()
        .expect("retained USS yields a ratio");
    assert!((uss_ratio - 0.5).abs() < 1e-4);
    let thp_ratio = process
        .memory_anon_huge_pages_ratio()
        .expect("retained huge pages yield a ratio");
    assert!((thp_ratio - 8.0).abs() < 1e-3);
    let breakdown = process.memory_breakdown();
    assert_eq!(breakdown.uss_bytes, Some(50 * mib));
    assert_eq!(breakdown.shared_bytes, Some(50 * mib));
    // RSS and PSS are read every tick and must stay fresh-only: a stale RSS is
    // not a current resident-set measurement.
    assert_eq!(process.current_memory_bytes(), Some(100 * mib));
    assert_eq!(process.current_memory_pss_bytes(), Some(70 * mib));
}

/// A counter that has never been measured is an absence, not a retained zero.
/// Decimation must not turn "no sample yet" into a fabricated value.
#[test]
fn never_measured_decimated_counters_stay_absent() {
    let mut process = ProcessItem::new(8, "cold");
    process.apply_scalar_observations(ProcessScalarObservations {
        memory_bytes: ScalarObservation::available(100 * 1024 * 1024, 1_000),
        memory_uss_bytes: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        memory_anon_huge_pages_bytes: ScalarObservation::unavailable(
            FailureKind::TemporarilyUnavailable,
        ),
        ..Default::default()
    });
    assert_eq!(process.memory_uss_bytes(), None);
    assert_eq!(process.memory_anon_huge_pages_bytes(), None);
    assert!(!process.has_memory_uss());
    assert_eq!(process.memory_shared_bytes(), None);
    assert_eq!(process.memory_proportional_shared_bytes(), None);
    assert_eq!(process.memory_uss_ratio(), None);
    assert_eq!(process.memory_anon_huge_pages_ratio(), None);
}
