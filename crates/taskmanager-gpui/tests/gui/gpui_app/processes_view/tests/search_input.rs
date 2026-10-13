//! Apps search behavior through the real GPUI input and RootView consumer.

use super::*;
use std::time::Duration;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_ui::init;

fn visible_process_pids(cx: &mut TestAppContext, view: &Entity<RootView>) -> Vec<u32> {
    view.update(cx, |view, _cx| {
        let (rows, _, _) = view.processes_projection();
        rows.iter()
            .filter_map(|row| row.process_identity.map(ProcessLiveKey::pid))
            .collect()
    })
}

async fn settle_debounced_search(cx: &mut TestAppContext) {
    // `gpui::Timer` is a wall-clock timer; the TestDispatcher's virtual clock
    // cannot resolve it. Wait beyond the real 100 ms debounce, then drain the
    // scheduled RootView update.
    cx.executor().allow_parking();
    cx.executor().timer(Duration::from_millis(125)).await;
    cx.executor().forbid_parking();
    cx.run_until_parked();
}

#[gpui::test]
async fn escape_clears_apps_query_restores_rows_and_preserves_frozen_selection(
    cx: &mut TestAppContext,
) {
    set_language(Language::En);
    cx.update(init);
    let (win, view) = wrapped_root(cx);
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Apps;
        view.set_process_sort(SortCol::Pid, SortDir::Asc);
        view.replace_processes_for_test(vec![
            ProcessItemFixtureBuilder::new()
                .pid(301)
                .name("alpha-worker".to_owned())
                .build(),
            ProcessItemFixtureBuilder::new()
                .pid(302)
                .name("beta-worker".to_owned())
                .build(),
        ]);
        cx.notify();
    });
    draw(cx, win);

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let search = view.read_with(cx, |view, _cx| {
        view.search_input
            .as_ref()
            .expect("rendering Apps creates its real search input")
            .clone()
    });
    let search_focus = search.read_with(cx, |search, _cx| search.focus_handle().clone());
    let beta_row = vcx
        .debug_bounds("tm-proc-row-root:2")
        .expect("the second process row must render below the category header");
    vcx.simulate_click(beta_row.center(), Modifiers::none());
    assert_eq!(
        view.read_with(cx, |view, _cx| view.selected_process_row()),
        Some(row_id(302)),
        "the actual row click freezes beta's stable process identity"
    );

    vcx.update(|window, cx| search_focus.focus(window, cx));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        search.read_with(cx, |search, _cx| search.clean_on_escape()),
        "the production Apps SearchInput opts into Escape-clears"
    );

    vcx.simulate_input("alpha");
    assert_eq!(
        search.read_with(cx, |search, _cx| search.display_text().to_owned()),
        "alpha"
    );
    cx.run_until_parked();
    settle_debounced_search(cx).await;

    assert_eq!(
        view.read_with(cx, |view, _cx| view.process_query().to_owned()),
        "alpha",
        "the SearchInput change reaches the shell-owned query"
    );
    assert_eq!(
        visible_process_pids(cx, &view),
        [301],
        "the real Apps projection contains only the matching process"
    );
    assert_eq!(
        view.read_with(cx, |view, _cx| view.selected_process_row()),
        Some(row_id(302)),
        "filtering out the selected row preserves its frozen identity"
    );

    vcx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(
        search.read_with(cx, |search, _cx| search.display_text().is_empty()),
        "Escape clears the field"
    );
    assert_eq!(
        view.read_with(cx, |view, _cx| view.process_query().to_owned()),
        "",
        "clearing the field immediately clears the shell query"
    );
    assert_eq!(
        visible_process_pids(cx, &view),
        [301, 302],
        "clearing the query restores the full filtered projection"
    );
    assert_eq!(
        view.read_with(cx, |view, _cx| view.selected_process_row()),
        Some(row_id(302)),
        "Escape restores rows without changing the selected frozen identity"
    );
}
