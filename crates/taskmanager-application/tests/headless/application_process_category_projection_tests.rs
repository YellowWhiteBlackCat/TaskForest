use super::*;
use taskmanager_core::core::FailureKind;
use taskmanager_core::core::metrics::{ScalarAvailability, ScalarObservation};
use taskmanager_core::core::process::{ProcessCategory, ProcessItem, process_category};
use taskmanager_test_support::{
    category_fixture_with_empty_bucket, mixed_availability_category_fixture,
};

fn buckets_of(items: &[ProcessItem]) -> Vec<CategoryBucketProjection<'_, ProcessItem>> {
    category_buckets(items, process_category)
}

/// Buckets follow the fixed ALL order and partition every item exactly
/// once (no item dropped, no item duplicated).
#[test]
fn buckets_follow_the_all_order_and_partition_every_item() {
    let items = mixed_availability_category_fixture();
    let buckets = buckets_of(&items);
    let categories: Vec<ProcessCategory> = buckets.iter().map(|bucket| bucket.category()).collect();
    assert_eq!(
        categories,
        [
            ProcessCategory::Application,
            ProcessCategory::Background,
            ProcessCategory::Uncategorized,
        ]
    );
    let total: usize = buckets.iter().map(|bucket| bucket.member_count()).sum();
    assert_eq!(total, items.len(), "every item lands in exactly one bucket");
}

/// An empty bucket is omitted entirely, and empty input yields no
/// buckets at all — never a fabricated empty header.
#[test]
fn empty_buckets_are_omitted_and_empty_input_yields_no_buckets() {
    let items = category_fixture_with_empty_bucket();
    let buckets = buckets_of(&items);
    let categories: Vec<ProcessCategory> = buckets.iter().map(|bucket| bucket.category()).collect();
    assert_eq!(
        categories,
        [ProcessCategory::Application, ProcessCategory::Background],
        "the Uncategorized bucket stays absent"
    );
    assert!(buckets_of(&[]).is_empty(), "no items → no buckets");
}

/// Members keep their input order within each bucket (the caller's
/// active sort order survives the projection untouched).
#[test]
fn members_preserve_the_input_order_within_each_bucket() {
    let items = mixed_availability_category_fixture();
    let buckets = buckets_of(&items);
    let pids: Vec<Vec<u32>> = buckets
        .iter()
        .map(|bucket| bucket.members().iter().map(|process| process.pid).collect())
        .collect();
    assert_eq!(pids, [vec![11, 12], vec![30, 31], vec![40, 41, 42]]);
    for bucket in &buckets {
        assert_eq!(bucket.member_count(), bucket.members().len());
    }
}

/// The typed aggregate helpers preserve coverage states and saturate instead
/// of turning missing observations into a successful zero.
#[test]
fn aggregate_helpers_preserve_states_and_saturate() {
    struct Metric {
        cpu: ScalarObservation<f32>,
        bytes: ScalarObservation<u64>,
    }
    let items = vec![
        Metric {
            cpu: ScalarObservation::available(2.5, 1),
            bytes: ScalarObservation::available(10, 1),
        },
        Metric {
            cpu: ScalarObservation::unavailable(FailureKind::PermissionDenied),
            bytes: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        },
        Metric {
            cpu: ScalarObservation::available(1.0, 1),
            bytes: ScalarObservation::available(u64::MAX, 1),
        },
    ];
    let buckets = category_buckets(&items, |_: &Metric| ProcessCategory::Application);
    assert_eq!(buckets.len(), 1);
    let bucket = &buckets[0];
    assert_eq!(bucket.member_count(), 3);
    let cpu = bucket
        .aggregate_f32(2, |metric| &metric.cpu)
        .expect("non-empty bucket produces a typed aggregate");
    assert_eq!(
        cpu.availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(cpu.current_value(), Some(&3.5));
    assert_eq!(cpu.current_member_count(), 2);
    let bytes = bucket
        .aggregate_u64(2, |metric| &metric.bytes)
        .expect("non-empty bucket produces a typed aggregate");
    assert_eq!(
        bytes.availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(bytes.current_value(), Some(&u64::MAX));
}

/// Expansion keys are the prefixed stable keys of ALL, in order, and are
/// pairwise distinct.
#[test]
fn expansion_keys_are_prefixed_distinct_and_stable() {
    let keys: Vec<String> = ProcessCategory::ALL
        .iter()
        .map(|category| category_expansion_key(*category))
        .collect();
    assert_eq!(
        keys,
        [
            "category:application",
            "category:background",
            "category:uncategorized",
        ]
    );
    for (index, left) in keys.iter().enumerate() {
        for right in keys.iter().skip(index + 1) {
            assert_ne!(left, right, "expansion keys must be pairwise distinct");
        }
    }
}

/// The fixture really covers the six identity-observation states across
/// the three buckets — Available and Partial are Applications, Absent is
/// Background, and Unknown / Stale / Unavailable stay Uncategorized.
#[test]
fn fixture_covers_every_identity_observation_state() {
    let items = mixed_availability_category_fixture();
    let by_pid = |pid: u32| items.iter().find(|process| process.pid == pid).cloned();
    for pid in [11, 12] {
        assert_eq!(
            process_category(&by_pid(pid).expect("fixture pid")),
            ProcessCategory::Application,
            "pid {pid} must classify as Application"
        );
    }
    for pid in [30, 31] {
        assert_eq!(
            process_category(&by_pid(pid).expect("fixture pid")),
            ProcessCategory::Background,
            "pid {pid} must classify as Background"
        );
    }
    for pid in [40, 41, 42] {
        assert_eq!(
            process_category(&by_pid(pid).expect("fixture pid")),
            ProcessCategory::Uncategorized,
            "pid {pid} must classify as Uncategorized"
        );
    }
}

/// The fixture's documented aggregate totals — CPU%, RSS, PSS and the
/// PSS-preferred display memory — are all produced from typed observations.
#[test]
fn fixture_bucket_aggregates_match_the_documented_totals() {
    let items = mixed_availability_category_fixture();
    let buckets = buckets_of(&items);
    assert_eq!(
        buckets[0]
            .aggregate_process_cpu(42)
            .and_then(|metric| metric.current_value().copied()),
        Some(14.0)
    );
    assert_eq!(
        buckets[0]
            .aggregate_process_memory_rss(42)
            .and_then(|metric| metric.current_value().copied()),
        Some(500)
    );
    assert_eq!(
        buckets[0]
            .aggregate_process_memory_for_display(42)
            .and_then(|metric| metric.current_value().copied()),
        Some(350),
        "PSS-preferred fold: 250 + RSS fallback 100"
    );
    assert_eq!(
        buckets[1]
            .aggregate_process_cpu(42)
            .and_then(|metric| metric.current_value().copied()),
        Some(5.0)
    );
    assert_eq!(
        buckets[1]
            .aggregate_process_memory_rss(42)
            .and_then(|metric| metric.current_value().copied()),
        Some(300)
    );
    assert_eq!(
        buckets[2]
            .aggregate_process_cpu(42)
            .and_then(|metric| metric.current_value().copied()),
        Some(5.5)
    );
    assert_eq!(
        buckets[2]
            .aggregate_process_memory_rss(42)
            .and_then(|metric| metric.current_value().copied()),
        Some(330)
    );
}

#[test]
fn process_projection_distinguishes_zero_stale_unavailable_and_unknown() {
    let zero = ProcessItem::new(1, "zero").with_scalar_observations(
        taskmanager_core::core::process::ProcessScalarObservations {
            cpu_percentage: ScalarObservation::available(0.0, 7),
            ..Default::default()
        },
    );
    let stale = ProcessItem::new(2, "stale").with_scalar_observations(
        taskmanager_core::core::process::ProcessScalarObservations {
            cpu_percentage: ScalarObservation::available(4.0, 6)
                .transition_failure(FailureKind::TimedOut),
            ..Default::default()
        },
    );
    let unavailable = ProcessItem::new(3, "unavailable").with_scalar_observations(
        taskmanager_core::core::process::ProcessScalarObservations {
            cpu_percentage: ScalarObservation::unavailable(FailureKind::Unsupported),
            ..Default::default()
        },
    );
    let unknown = ProcessItem::new(4, "unknown");

    let zero_items = [zero];
    let zero_bucket = buckets_of(&zero_items);
    let zero_metric = zero_bucket[0]
        .aggregate_process_cpu(8)
        .expect("non-empty bucket");
    assert_eq!(zero_metric.availability(), ScalarAvailability::Available);
    assert_eq!(zero_metric.current_value(), Some(&0.0));

    let stale_items = [stale];
    let stale_bucket = buckets_of(&stale_items);
    let stale_metric = stale_bucket[0]
        .aggregate_process_cpu(8)
        .expect("non-empty bucket");
    assert_eq!(
        stale_metric.availability(),
        ScalarAvailability::Stale(FailureKind::TimedOut)
    );
    assert_eq!(stale_metric.current_value(), None);
    assert_eq!(stale_metric.last_known_value(), Some(&4.0));

    let unavailable_items = [unavailable];
    let unavailable_bucket = buckets_of(&unavailable_items);
    let unavailable_metric = unavailable_bucket[0]
        .aggregate_process_cpu(8)
        .expect("non-empty bucket");
    assert_eq!(
        unavailable_metric.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(unavailable_metric.current_value(), None);
    assert_eq!(unavailable_metric.last_known_value(), None);

    let unknown_items = [unknown];
    let unknown_bucket = buckets_of(&unknown_items);
    let unknown_metric = unknown_bucket[0]
        .aggregate_process_cpu(8)
        .expect("non-empty bucket");
    assert_eq!(unknown_metric.availability(), ScalarAvailability::Unknown);
    assert_eq!(unknown_metric.current_value(), None);
    assert_eq!(unknown_metric.last_known_value(), None);
}
