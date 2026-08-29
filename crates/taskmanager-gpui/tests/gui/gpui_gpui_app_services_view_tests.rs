use super::{MenuEntry, build_service_menu};
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::TopPage;
use gpui::{AppContext, TestAppContext, VisualTestContext, px};
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_theme::Theme;
use taskmanager_ui::overlays::popup::MenuItem;

/// The row context menu carries the five service actions (Win11 TM
/// parity), labeled through i18n so the menu reads localized copy.
#[gpui::test]
async fn service_row_context_menu_offers_all_five_actions(cx: &mut gpui::TestAppContext) {
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
        ],
        "the context menu must list Start/Stop/Restart/Enable/Disable"
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
