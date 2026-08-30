//! `#[gpui::test]` companion to the pure `projection.rs` group: exercises the
//! real `RootView` row-model cache through the GPUI state pipeline. Split out
//! of `tests.rs` only to satisfy the source-line guard; behavior is unchanged.
//! Helpers (`wrapped_root`) and the row/sort types come from the parent module.

use super::*;

#[gpui::test]
async fn projection_cache_reuses_rows_until_state_or_data_changes(cx: &mut TestAppContext) {
    let (_, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.replace_processes_for_test(
            (1..=4)
                .map(|pid| {
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(pid)
                        .name(format!("proc-{pid}"))
                        .build()
                })
                .collect(),
        );
        cx.notify();
    });
    let first = view.update(cx, |v, _cx| v.processes_projection());
    let second = view.update(cx, |v, _cx| v.processes_projection());
    assert!(
        std::rc::Rc::ptr_eq(&first.0, &second.0),
        "unchanged state must reuse the cached row model"
    );
    assert_eq!(
        first
            .1
            .iter()
            .map(|identity| identity.pid())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "default CPU-descending state: the all-zero CPU tie breaks pid-ASCENDING \
         (the neutral comparator's direction-independent tie-break)"
    );

    // A data tick (new generation) must rebuild.
    view.update(cx, |v, cx| {
        v.replace_processes_for_test(
            (10..=12)
                .map(|pid| {
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(pid)
                        .name(format!("proc-{pid}"))
                        .build()
                })
                .collect(),
        );
        cx.notify();
    });
    let after_tick = view.update(cx, |v, _cx| v.processes_projection());
    assert!(
        !std::rc::Rc::ptr_eq(&first.0, &after_tick.0),
        "a data tick must invalidate the cached row model"
    );
    assert_eq!(
        after_tick
            .1
            .iter()
            .map(|identity| identity.pid())
            .collect::<Vec<_>>(),
        vec![10, 11, 12],
        "a data tick rebuilds with the same sort state (CPU tie, pid ascending)"
    );

    // A sort change must rebuild too (through the shell-owned sort state).
    view.update(cx, |v, cx| {
        v.set_process_sort(SortCol::Pid, taskmanager_shell::SortDir::Desc);
        cx.notify();
    });
    let after_sort = view.update(cx, |v, _cx| v.processes_projection());
    assert_eq!(
        after_sort
            .1
            .iter()
            .map(|identity| identity.pid())
            .collect::<Vec<_>>(),
        vec![12, 11, 10],
        "descending pid sort must reorder the projection"
    );
}
