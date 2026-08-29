use std::rc::Rc;

use gpui::TestAppContext;

use super::ProcessTooltipIndex;
use crate::gpui_app::root::RootView;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_theme::Theme;

fn process(pid: u32, name: &str, cmdline: &str) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.to_owned())
        .cmdline(cmdline.to_owned())
        .build()
}

/// The memoized history series share the item memo's identity contract:
/// the same snapshot + pid hands back the SAME `Rc` pack (so the scene
/// store replays the modal's four history graphs), and a rebuilt memo
/// produces a fresh pack carrying the new item's data.
#[gpui::test]
async fn details_histories_share_the_memo_identity(cx: &mut TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |view, _window, _cx| {
        let mut with_history = process(22, "browser", "/usr/bin/browser --flag");
        with_history.cpu_history = vec![5.0, 12.0, 30.0];
        view.replace_processes_for_test(vec![with_history]);

        let (_, first) = view.process_details_target(22).expect("pid 22 present");
        assert_eq!(&*first.cpu, &[5.0, 12.0, 30.0]);
        let (_, second) = view
            .process_details_target(22)
            .expect("pid 22 still present");
        assert!(
            Rc::ptr_eq(&first, &second),
            "unchanged snapshot + pid must reuse the cached history pack"
        );

        let mut refreshed = process(22, "browser", "/usr/bin/browser --flag");
        refreshed.cpu_history = vec![7.0, 9.0];
        view.replace_processes_for_test(vec![refreshed]);
        let (_, rebuilt) = view
            .process_details_target(22)
            .expect("pid 22 present after refresh");
        assert!(
            !Rc::ptr_eq(&first, &rebuilt),
            "a replaced snapshot must rebuild the history pack"
        );
        assert_eq!(&*rebuilt.cpu, &[7.0, 9.0]);
    })
    .unwrap();
}

#[test]
fn pid_index_reuses_one_snapshot_and_rebuilds_for_a_new_one() {
    let first = Rc::new(vec![
        process(11, "short", "short"),
        process(22, "worker", "/usr/bin/worker --long"),
    ]);
    let second = Rc::new(vec![process(33, "replacement", "/bin/replacement")]);
    let mut index = ProcessTooltipIndex::default();

    assert_eq!(index.index_for(&first, 22), Some(1));
    assert_eq!(index.index_for(&first, 11), Some(0));
    assert_eq!(index.index_for(&first, 99), None);
    assert_eq!(index.index_for(&second, 33), Some(0));
    assert_eq!(index.index_for(&second, 22), None);
}

/// The Properties-modal target memo: same snapshot + pid returns the SAME
/// `Rc` (no per-frame scan or clone), a replaced snapshot or a changed pid
/// rebuilds, and a pid refreshed out of the snapshot resolves to `None`
/// (the caller clears the dialog slot).
#[gpui::test]
async fn details_item_memo_reuses_the_cached_target_and_tracks_the_snapshot(
    cx: &mut TestAppContext,
) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |view, _window, _cx| {
        view.replace_processes_for_test(vec![
            process(11, "alpha", "alpha"),
            process(22, "browser", "/usr/bin/browser --flag"),
        ]);

        let (first, _) = view.process_details_target(22).expect("pid 22 present");
        assert_eq!(first.pid, 22);
        let (second, _) = view
            .process_details_target(22)
            .expect("pid 22 still present");
        assert!(
            Rc::ptr_eq(&first, &second),
            "unchanged snapshot + pid must reuse the cached item"
        );

        // A different pid rebuilds even on the same snapshot.
        let (other, _) = view.process_details_target(11).expect("pid 11 present");
        assert_eq!(other.pid, 11);
        assert!(!Rc::ptr_eq(&first, &other));

        // The pid disappears from the next snapshot: honest None.
        view.replace_processes_for_test(vec![process(33, "replacement", "/bin/replacement")]);
        assert!(
            view.process_details_target(22).is_none(),
            "a pid refreshed out of the snapshot must clear the dialog target"
        );
    })
    .unwrap();
}
