use std::collections::HashSet;

use super::{SortCol, Toggle, category_expansion_key, category_tree_rows, memory_for_display};
use crate::core::process::{ProcessCategory, ProcessItem, process_category};
use taskmanager_application::process_category_projection::category_buckets;
use taskmanager_test_support::{
    category_fixture_with_empty_bucket, mixed_availability_category_fixture,
};

/// Collapsed: one aggregate header per non-empty neutral bucket, in the
/// same order, with the same expansion key, CPU% sum, memory fold, and
/// member count.
#[test]
fn category_rows_match_neutral_buckets_on_the_shared_fixture() {
    let items = mixed_availability_category_fixture();
    let refs: Vec<&ProcessItem> = items.iter().collect();
    let buckets = category_buckets(&refs, |process| process_category(process));
    assert_eq!(buckets.len(), 3, "every fixture bucket is non-empty");

    let rows = category_tree_rows(&refs, SortCol::Cpu, false, &HashSet::new(), &HashSet::new());
    assert_eq!(
        rows.len(),
        buckets.len(),
        "collapsed: one header per bucket"
    );
    for (row, bucket) in rows.iter().zip(&buckets) {
        let Toggle::GroupCategory(category) = &row.toggle else {
            panic!("a multi-member bucket header must carry the category toggle");
        };
        assert_eq!(*category, bucket.category());
        let key = category_expansion_key(*category);
        assert_eq!(
            key,
            format!("category:{}", bucket.category().stable_key()),
            "the neutral expansion key is the prefixed stable key"
        );
        assert_eq!(
            row.cpu,
            Some(bucket.sum_f32(|process| process.current_cpu_percentage())),
            "header CPU% is the neutral bucket sum"
        );
        assert_eq!(
            row.mem,
            Some(bucket.sum_u64(|process| memory_for_display(process))),
            "header memory is the gpui PSS-preferred fold over the same members"
        );
        assert_eq!(
            row.process_pid, None,
            "a category aggregate has no representative process PID"
        );
        assert_eq!(row.cell_text.pid, "");
        assert_eq!(
            row.badge, None,
            "category totals do not masquerade as instances"
        );
    }
}

/// Expanded: every bucket opens under its neutral expansion key. Applications
/// expose PID-less app-root aggregates first; the process rows appear only
/// after their own app expansion key is opened. Non-application buckets keep
/// their direct process-tree members.
#[test]
fn expanded_category_members_match_neutral_bucket_order() {
    let items = mixed_availability_category_fixture();
    let refs: Vec<&ProcessItem> = items.iter().collect();
    let buckets = category_buckets(&refs, |process| process_category(process));
    let expanded: HashSet<String> = ProcessCategory::ALL
        .iter()
        .map(|category| category_expansion_key(*category))
        .collect();
    let mut expanded = expanded;
    expanded.insert("app-tree:11".to_owned());
    expanded.insert("app-tree:12".to_owned());

    let rows = category_tree_rows(&refs, SortCol::Cpu, false, &expanded, &HashSet::new());
    let mut cursor = 0;
    for bucket in &buckets {
        let Toggle::GroupCategory(category) = &rows[cursor].toggle else {
            panic!("row {cursor} must be a category header");
        };
        assert_eq!(*category, bucket.category());
        assert!(!rows[cursor].collapsed, "the bucket is expanded");
        cursor += 1;
        if bucket.category() == ProcessCategory::Application {
            for (member, expected_name) in bucket
                .members()
                .iter()
                .zip(["Fixture Editor", "Fixture Helper"])
            {
                let row = &rows[cursor];
                assert_eq!(row.name, expected_name);
                assert_eq!(row.process_pid, None);
                assert_eq!(row.depth, 1, "app aggregate rows indent one level");
                assert!(matches!(row.toggle, Toggle::GroupApp(_)));
                assert_eq!(row.cell_text.pid, "");
                cursor += 1;
                assert_eq!(rows[cursor].process_pid, Some(member.pid));
                assert_eq!(
                    rows[cursor].depth, 2,
                    "process rows indent below the app total"
                );
                cursor += 1;
            }
        } else {
            for member in bucket.members() {
                let row = &rows[cursor];
                assert_eq!(row.pid, member.pid, "member order follows the bucket");
                assert_eq!(row.depth, 1, "members indent one level");
                assert!(matches!(row.toggle, Toggle::None));
                cursor += 1;
            }
        }
    }
    assert_eq!(cursor, rows.len(), "no rows outside the neutral buckets");
}

/// A bucket the neutral projection omits (empty) never gains a header
/// here either.
#[test]
fn empty_neutral_bucket_renders_no_header() {
    let items = category_fixture_with_empty_bucket();
    let refs: Vec<&ProcessItem> = items.iter().collect();
    let buckets = category_buckets(&refs, |process| process_category(process));
    assert_eq!(buckets.len(), 2, "Uncategorized stays empty");
    let rows = category_tree_rows(&refs, SortCol::Cpu, false, &HashSet::new(), &HashSet::new());
    assert_eq!(
        rows.len(),
        buckets.len(),
        "one row per non-empty neutral bucket only"
    );
}
