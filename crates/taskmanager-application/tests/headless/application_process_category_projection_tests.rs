use super::*;
use crate::process_category;
use taskmanager_test_support::{
    category_fixture_with_empty_bucket, mixed_availability_category_fixture,
};

fn buckets_of(
    items: &[crate::ProcessItem],
) -> Vec<CategoryBucketProjection<'_, crate::ProcessItem>> {
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

/// The sum helpers skip `None` members and saturate instead of
/// overflowing.
#[test]
fn sum_helpers_skip_none_values_and_saturate() {
    struct Metric {
        cpu: Option<f32>,
        bytes: Option<u64>,
    }
    let items = vec![
        Metric {
            cpu: Some(2.5),
            bytes: Some(10),
        },
        Metric {
            cpu: None,
            bytes: None,
        },
        Metric {
            cpu: Some(1.0),
            bytes: Some(u64::MAX),
        },
    ];
    let buckets = category_buckets(&items, |_: &Metric| ProcessCategory::Application);
    assert_eq!(buckets.len(), 1);
    let bucket = &buckets[0];
    assert_eq!(bucket.member_count(), 3);
    assert_eq!(bucket.sum_f32(|metric| metric.cpu), 3.5);
    assert_eq!(bucket.sum_u64(|metric| metric.bytes), u64::MAX);
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

/// The fixture's documented aggregate totals — CPU% sums, RSS sums, and
/// the PSS-preferred memory fold — match what the closure-driven helpers
/// compute.
#[test]
fn fixture_bucket_aggregates_match_the_documented_totals() {
    let items = mixed_availability_category_fixture();
    let buckets = buckets_of(&items);
    assert_eq!(
        buckets[0].sum_f32(|process| process.current_cpu_percentage()),
        14.0
    );
    assert_eq!(
        buckets[0].sum_u64(|process| process.current_memory_bytes()),
        500
    );
    assert_eq!(
        buckets[0].sum_u64(|process| process
            .current_memory_pss_bytes()
            .or_else(|| process.current_memory_bytes())),
        350,
        "PSS-preferred fold: 250 + RSS fallback 100"
    );
    assert_eq!(
        buckets[1].sum_f32(|process| process.current_cpu_percentage()),
        5.0
    );
    assert_eq!(
        buckets[1].sum_u64(|process| process.current_memory_bytes()),
        300
    );
    assert_eq!(
        buckets[2].sum_f32(|process| process.current_cpu_percentage()),
        5.5
    );
    assert_eq!(
        buckets[2].sum_u64(|process| process.current_memory_bytes()),
        330
    );
}
