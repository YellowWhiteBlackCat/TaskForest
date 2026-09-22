use super::{MenuEntry, ServiceFilter, build_service_menu};
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::TopPage;
use gpui::{AppContext, TestAppContext, VisualTestContext, px};
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_theme::Theme;
use taskmanager_ui::overlays::popup::MenuItem;

fn service(id: &str, name: &str, status: ServiceStatus) -> ServiceItem {
    ServiceItem::from_inventory(id, name, status, "fixture unit", "", "", "")
}

/// `debug_bounds` takes a `&'static str`; the row/status selectors are indexed.
fn selector(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// The row context menu carries the six service actions (Win11 TM
/// parity), labeled through i18n so the menu reads localized copy.
#[gpui::test]
async fn service_row_context_menu_offers_all_six_actions(cx: &mut gpui::TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    let items = build_service_menu(root);
    let labels: Vec<String> = items
        .iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item(item) => Some(item.label.to_string()),
            MenuEntry::Separator | MenuEntry::Label(_) => None,
        })
        .collect();
    assert_eq!(
        labels,
        vec![
            taskmanager_application::i18n::t("svc.start").to_string(),
            taskmanager_application::i18n::t("svc.stop").to_string(),
            taskmanager_application::i18n::t("svc.restart").to_string(),
            taskmanager_application::i18n::t("svc.enable").to_string(),
            taskmanager_application::i18n::t("svc.disable").to_string(),
            taskmanager_application::i18n::t("svc.reload_daemon").to_string(),
        ],
        "the context menu must list lifecycle actions and daemon reload"
    );
    // Every item is interactive (has an activation closure).
    for entry in &items {
        if let MenuEntry::Item(item) = entry {
            let _: &MenuItem = item;
            assert!(
                item.action.is_some(),
                "every context-menu item must carry its activation action"
            );
        }
    }
}

/// A partial service-name query must only recolor the matching bytes. It must
/// not make the name cell collapse to the match fragment or alter the table's
/// row geometry (the desktop regression used `p-4000` with query `4`).
#[gpui::test]
async fn service_search_highlight_keeps_name_cell_bounded(cx: &mut TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Services;
        v.replace_services_for_test(
            vec![ServiceItem::from_inventory(
                "fixture.p-4000.service",
                "p-4000",
                ServiceStatus::Active,
                "cap P-core frequency",
                "",
                "",
                "",
            )],
            Vec::new(),
        );
        cx.notify();
    });
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    // The persistent table receives its first delegate snapshot during this
    // draw; the following frame paints the now-synchronized virtual rows.
    vcx.update(|window, cx| window.draw(cx).clear());
    let plain_row = vcx
        .debug_bounds("tm-svc-row:0")
        .expect("the unfiltered service row must render");

    view.update(cx, |v, cx| {
        v.services_state.query = "4".to_owned();
        cx.notify();
    });
    vcx.update(|window, cx| window.draw(cx).clear());

    let highlighted_row = vcx
        .debug_bounds("tm-svc-row:0")
        .expect("the matching service row must remain visible");
    assert_eq!(
        plain_row.size.height, highlighted_row.size.height,
        "search highlighting must not change a service table row's geometry: plain={plain_row:?}, highlighted={highlighted_row:?}"
    );
    assert!(
        highlighted_row.size.width > px(0.0),
        "the highlighted service row must retain a usable width: {highlighted_row:?}"
    );
}

/// The inventory's typed active state is the painted list's authority: every
/// unit paints its own row and a status cell carrying its typed state token,
/// a row never paints another unit's state, and the typed status filter is the
/// painted membership authority (a filter with no member paints an empty
/// inventory, never a placeholder row). The filtered phases use fresh windows
/// because `debug_bounds` keeps the last frame that painted a selector, so a
/// dropped row is only observable as "never painted in this window".
#[gpui::test]
async fn inventory_rows_paint_each_units_typed_active_state(cx: &mut TestAppContext) {
    let services = || {
        vec![
            service(
                "fixture.service:active",
                "active-unit",
                ServiceStatus::Active,
            ),
            service(
                "fixture.service:failed",
                "failed-unit",
                ServiceStatus::Failed,
            ),
            service(
                "fixture.service:inactive",
                "inactive-unit",
                ServiceStatus::Inactive,
            ),
        ]
    };
    let wrapped_root = |cx: &mut TestAppContext, filter: ServiceFilter, list: Vec<ServiceItem>| {
        let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
        let view = win.entity(cx).expect("window root RootView entity");
        view.update(cx, |v, cx| {
            v.mark_telemetry_frame_ready();
            v.page = TopPage::Services;
            v.services_state.filter = filter;
            v.replace_services_for_test(list, Vec::new());
            cx.notify();
        });
        (win, view)
    };
    // First draw binds the persistent table delegate; the second paints the
    // synchronized virtual rows.
    let draw = |cx: &mut TestAppContext, win: gpui::WindowHandle<RootView>| {
        for _ in 0..2 {
            cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
                .unwrap();
        }
    };

    // Unfiltered: one painted row per unit, each with its own typed state.
    let (win, _view) = wrapped_root(cx, ServiceFilter::All, services());
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    for (ix, status) in ["Active", "Failed", "Inactive"].into_iter().enumerate() {
        let row = vcx
            .debug_bounds(selector(format!("tm-svc-row:{ix}")))
            .unwrap_or_else(|| panic!("unit {ix} must paint its inventory row"));
        assert!(row.size.height > px(10.0), "row {ix} collapsed: {row:?}");
        let cell = selector(format!("tm-svc-status:{ix}:{status}"));
        assert!(
            vcx.debug_bounds(cell).is_some(),
            "{cell}: the row must paint its own typed active state"
        );
    }
    assert!(
        vcx.debug_bounds("tm-svc-status:0:Failed").is_none(),
        "a row must not paint another unit's typed state"
    );
    assert!(
        vcx.debug_bounds("tm-svc-status:3:Unknown").is_none(),
        "no row beyond the projected inventory may be painted"
    );
    drop(vcx);

    // Typed status filter: only the matching unit reaches the frame.
    let (win, _view) = wrapped_root(cx, ServiceFilter::Failed, services());
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-svc-status:0:Failed").is_some(),
        "the matching unit must stay painted"
    );
    assert!(
        vcx.debug_bounds("tm-svc-row:1").is_none(),
        "the typed filter must keep every non-matching row out of the frame"
    );
    drop(vcx);

    // A typed filter with no member paints an empty inventory, never a
    // placeholder row.
    let (win, _view) = wrapped_root(
        cx,
        ServiceFilter::Inactive,
        vec![service(
            "fixture.service:active",
            "active-unit",
            ServiceStatus::Active,
        )],
    );
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-svc-row:0").is_none(),
        "a typed-status filter with no member must paint no inventory row"
    );
}
