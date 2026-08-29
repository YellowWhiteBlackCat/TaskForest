use crate::gpui_app::processes_view::rows::columns;
use std::collections::HashSet;
use taskmanager_application::process_sort::ProcessSortAxis;
use taskmanager_shell::sort_axis;

/// The mapping covers every GPUI column exactly once (via `rows::columns`,
/// the view's own iteration source — never a duplicated list).
#[test]
fn every_sort_col_maps_to_a_distinct_neutral_axis() {
    let mapped: Vec<ProcessSortAxis> = columns().iter().copied().map(sort_axis).collect();
    let distinct: HashSet<ProcessSortAxis> = mapped.iter().copied().collect();
    assert_eq!(
        mapped.len(),
        distinct.len(),
        "two GPUI columns must not share one neutral axis"
    );
}
