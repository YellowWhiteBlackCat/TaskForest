use super::{InventoryDataFingerprint, InventoryProjection};
use taskmanager_shell::{InfoSortCol, SortDir};

#[test]
fn inventory_projection_reuses_facts_when_only_the_filter_changes() {
    let fingerprint = InventoryDataFingerprint {
        watermark: 7,
        source_len: 3,
        sort: Some((InfoSortCol::Name, SortDir::Asc)),
    };
    let mut memo = InventoryProjection::default();
    let mut builds = 0;

    builds += 1;
    let (rows, indices, first_generation) = memo.replace_rows_and_project(
        fingerprint,
        "",
        vec!["alpha".to_owned(), "beta".to_owned(), "alphabet".to_owned()],
        |row, query| row.contains(query),
    );
    assert_eq!(builds, 1);
    assert_eq!(indices.as_ref(), &[0, 1, 2]);

    assert!(memo.matches_data(fingerprint));
    let (same_rows, filtered, filtered_generation) =
        memo.project_query("alpha", |row, query| row.contains(query));
    assert_eq!(builds, 1, "filtering must not rebuild owned row facts");
    assert_eq!(same_rows.as_ref(), rows.as_ref());
    assert_eq!(filtered.as_ref(), &[0, 2]);
    assert!(filtered_generation > first_generation);

    let (_, same_filter, same_generation) =
        memo.project_query("alpha", |row, query| row.contains(query));
    assert_eq!(same_filter.as_ref(), &[0, 2]);
    assert_eq!(same_generation, filtered_generation);
    assert_eq!(builds, 1, "an unchanged filter must retain the same facts");
}
