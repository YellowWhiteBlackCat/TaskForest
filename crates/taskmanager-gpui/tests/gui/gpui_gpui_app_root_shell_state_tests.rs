use super::*;
use crate::gpui_app::theme::Theme;
use gpui::AppContext;
use taskmanager_application::{
    FailureKind, LatestControlRequest, ServiceAction, ServiceControlOutcome, ServiceId,
};

/// Interaction regression for the write path the Apps-page row handlers
/// and keyboard router drive: plain click collapses, ctrl-click toggles,
/// shift-click spans the display order, bare arrows collapse while
/// modifier arrows preserve, and a refresh prunes dead pids. These are the
/// same semantics the shell track's `ShellApp` selection carries — now one
/// implementation.
#[gpui::test]
fn selection_interactions_follow_the_shell_owned_rules(cx: &mut gpui::TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, _| {
        let display = [10u32, 11, 12, 13];

        // Plain click: single select.
        view.select_process_single(11);
        assert_eq!(view.selected_pid(), Some(11));
        assert!(view.selected_pids().contains(&11));
        assert_eq!(view.selected_pids().len(), 1);

        // Ctrl-click: grow the set, anchor follows.
        view.toggle_process_selection(12);
        assert_eq!(view.selected_pid(), Some(12));
        assert_eq!(view.selected_pids().len(), 2);
        // Ctrl-click the anchor off: anchor falls back to a member.
        view.toggle_process_selection(12);
        assert_eq!(view.selected_pid(), Some(11));
        assert_eq!(view.selected_pids().len(), 1);

        // Shift-click: span display order from the anchor.
        view.extend_process_selection(&display, 13);
        assert_eq!(view.selected_pid(), Some(13));
        assert_eq!(
            view.selected_pids(),
            &HashSet::from([11u32, 12, 13]),
            "the range spans anchor(11)→13 in display order"
        );

        // Keyboard move without modifiers collapses to the new anchor.
        view.move_process_selection(Some(10), false);
        assert_eq!(view.selected_pid(), Some(10));
        assert_eq!(view.selected_pids().len(), 1);
        // Modifier roaming preserves the set.
        view.move_process_selection(Some(11), true);
        assert_eq!(view.selected_pids().len(), 2);

        // Batch targets prefer the sorted set; the anchor is the fallback.
        assert_eq!(view.selected_process_pids(), vec![10, 11]);
        view.select_process_single(9);
        assert_eq!(view.selected_process_pids(), vec![9]);

        // A process refresh prunes dead pids and clears a dead anchor.
        let live = HashSet::from([9u32]);
        view.shell.selection.retain_live(&live);
        assert_eq!(view.selected_pid(), Some(9));
        let empty = HashSet::new();
        view.select_process_single(100);
        view.shell.selection.retain_live(&empty);
        assert_eq!(view.selected_pid(), None, "a dead anchor clears");
    });
}

/// The three-state widget cycle maps onto the shell sort slots verbatim:
/// first click (Descending) stores Desc, the second (Ascending) flips it,
/// and the third (Unsorted) returns the table to provider order.
#[gpui::test]
fn table_sort_events_map_verbatim_onto_the_shell_sorts(cx: &mut gpui::TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, _| {
        assert_eq!(view.inventory_sort(InfoTable::Services), None);
        view.apply_table_sort(
            InfoTable::Services,
            Some(InfoSortCol::Status),
            SortState::Descending,
        );
        assert_eq!(
            view.inventory_sort(InfoTable::Services),
            Some((InfoSortCol::Status, SortDir::Desc))
        );
        view.apply_table_sort(
            InfoTable::Services,
            Some(InfoSortCol::Status),
            SortState::Ascending,
        );
        assert_eq!(
            view.inventory_sort(InfoTable::Services),
            Some((InfoSortCol::Status, SortDir::Asc))
        );
        view.apply_table_sort(
            InfoTable::Services,
            Some(InfoSortCol::Status),
            SortState::Unsorted,
        );
        assert_eq!(view.inventory_sort(InfoTable::Services), None);
        // An unsortable column's event is ignored entirely.
        view.apply_table_sort(InfoTable::Users, None, SortState::Descending);
        assert_eq!(view.inventory_sort(InfoTable::Users), None);
    });
}

/// The typed Services outcome folds into the shared action-bar copy with
/// the target resolved from the live services list (falling back to the
/// provider id), and errors render as copy — never a raw debug blob.
#[gpui::test]
fn services_feedback_folds_the_typed_outcome_into_localized_copy(cx: &mut gpui::TestAppContext) {
    use crate::core::services::{ServiceItem, ServiceStatus};
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, _| {
        assert!(view.services_feedback().is_none());
        let request_id = LatestControlRequest::default().begin();
        let outcome = ServiceControlOutcome {
            request_id,
            service_id: ServiceId::new("fixture.service".to_owned()),
            action: ServiceAction::Stop,
            result: Err(FailureKind::PermissionDenied),
        };
        view.shell.feedback.record_service(outcome);
        view.replace_services_for_test(
            vec![ServiceItem::from_inventory(
                "fixture.service",
                "Fixture Service",
                ServiceStatus::Active,
                "",
                "",
                "",
                "",
            )],
            Vec::new(),
        );
        let feedback = view
            .services_feedback()
            .expect("typed outcome folds to copy");
        assert!(feedback.is_error());
        assert!(feedback.text().contains("Fixture Service"));
    });
}
