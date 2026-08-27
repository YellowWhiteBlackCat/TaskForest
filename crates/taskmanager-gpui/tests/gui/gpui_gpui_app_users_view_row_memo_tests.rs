use super::*;
use crate::gpui_app::root::RootView;
use gpui::TestAppContext;

#[gpui::test]
fn sessions_rows_reuse_the_provider_snapshot_until_generation_changes(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, _cx| {
        view.replace_sessions_for_test(
            vec![SessionItem {
                id: "session-1".to_owned(),
                user: "alice".to_owned(),
                ..SessionItem::default()
            }],
            Vec::new(),
        );

        let first = view.sessions_rows();
        assert_eq!(first[0].id, "session-1");
        let second = view.sessions_rows();
        assert!(
            Rc::ptr_eq(&first, &second),
            "unchanged session snapshots must reuse the cached rows"
        );

        view.advance_sessions_generation_for_test();
        let rebuilt = view.sessions_rows();
        assert!(
            !Rc::ptr_eq(&first, &rebuilt),
            "a new session snapshot must replace the cached rows"
        );
    });
}

/// The Users header-sort chain, end to end: the widget's `perform_sort`
/// (exactly what the header icon invokes) emits `SortChanged`, the
/// subscriber applies the post-cycle state verbatim onto the shell-owned
/// `InventorySorts` slot, and the memo — keyed on that slot — rebuilds
/// with the new row order.
#[gpui::test]
fn header_sort_click_flows_through_the_shell_slot_and_reorders_the_memo(cx: &mut TestAppContext) {
    use taskmanager_shell::{InfoSortCol, InfoTable, SortDir};
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    let table = root.update(cx, |view, cx| {
        view.replace_sessions_for_test(
            vec![
                SessionItem {
                    id: "2".to_owned(),
                    user: "alice".to_owned(),
                    ..SessionItem::default()
                },
                SessionItem {
                    id: "1".to_owned(),
                    user: "root".to_owned(),
                    ..SessionItem::default()
                },
            ],
            Vec::new(),
        );
        init_table_entity(Theme::dark(), cx)
    });
    // Provider order until a column is picked.
    root.update(cx, |view, _| {
        let rows = view.sessions_rows();
        let ids: Vec<&str> = rows.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["2", "1"]);
    });
    // First click on the Session column (ix 0): widget cycles to
    // Descending, the subscriber applies it verbatim to the shell slot.
    table.update(cx, |table, cx| table.perform_sort(0, cx));
    root.update(cx, |view, _| {
        assert_eq!(
            view.inventory_sort(InfoTable::Users),
            Some((InfoSortCol::Session, SortDir::Desc))
        );
        let rows = view.sessions_rows();
        let ids: Vec<&str> = rows.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["2", "1"], "session ids sort descending");
    });
    // Second click: Ascending — the memo rebuilds with the flipped order.
    table.update(cx, |table, cx| table.perform_sort(0, cx));
    root.update(cx, |view, _| {
        assert_eq!(
            view.inventory_sort(InfoTable::Users),
            Some((InfoSortCol::Session, SortDir::Asc))
        );
        let rows = view.sessions_rows();
        let ids: Vec<&str> = rows.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["1", "2"]);
    });
}
