//! Services row-memo and landscape-layout regressions.

use super::*;
use crate::gpui_app::root::{RootView, TopPage};
use gpui::{TestAppContext, VisualTestContext, px, size};
use std::rc::Rc;
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_theme::Theme;

use taskmanager_shell::{InfoSortCol, InfoTable, SortDir};

fn service(id: &str, name: &str, status: ServiceStatus) -> ServiceItem {
    ServiceItem::from_inventory(id, name, status, "", "", "", "")
}

#[gpui::test]
fn services_rows_reuse_the_projection_until_an_input_changes(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, _cx| {
        view.replace_services_for_test(
            vec![
                service("b.service", "Beta", ServiceStatus::Active),
                service("a.service", "Alpha", ServiceStatus::Failed),
            ],
            Vec::new(),
        );
        let first = view.services_rows();
        // No column picked yet: the shell sort slot is `None`, so the memoized
        // order is the provider order (single source: shell `InventorySorts`).
        let names: Vec<&str> = first.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Beta", "Alpha"]);
        let second = view.services_rows();
        assert!(
            Rc::ptr_eq(&first, &second),
            "unchanged inputs must reuse the cached projection"
        );

        // A new list generation invalidates the memo.
        view.advance_services_generation_for_test();
        let rebuilt = view.services_rows();
        assert!(!Rc::ptr_eq(&first, &rebuilt));

        // A query edit invalidates it too (without any generation bump).
        view.services_state.query = "Alpha".to_owned();
        let filtered = view.services_rows();
        assert_eq!(filtered.len(), 1, "the memo must apply the new query");
        assert_eq!(filtered[0].name, "Alpha");
    });
}

/// The inventory-sort chain, end to end: a table-header sort click drives the
/// widget's `perform_sort` (exactly what the header icon invokes), the emitted
/// `SortChanged` applies the post-cycle state VERBATIM onto the shell-owned
/// `InventorySorts` slot, and the memo — keyed on that slot — rebuilds with
/// the new row order.
#[gpui::test]
fn header_sort_click_flows_through_the_shell_slot_and_reorders_the_memo(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    // Build the persistent table entity exactly as the first Services render
    // does (the SortChanged subscription is wired inside).
    let table = root.update(cx, |view, cx| {
        view.replace_services_for_test(
            vec![
                service("b.service", "Beta", ServiceStatus::Active),
                service("a.service", "Alpha", ServiceStatus::Failed),
                service("c.service", "Gamma", ServiceStatus::Inactive),
            ],
            Vec::new(),
        );
        init_table_entity(Theme::dark(), cx)
    });
    root.update(cx, |view, _| {
        let provider_rows = view.services_rows();
        let provider_order: Vec<&str> = provider_rows.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(provider_order, vec!["Beta", "Alpha", "Gamma"]);
    });
    // First header click on the Status column: the widget cycles to
    // Descending, the subscriber applies it verbatim to the shell slot.
    table.update(cx, |table, cx| table.perform_sort(0, cx));
    root.update(cx, |view, _| {
        assert_eq!(
            view.inventory_sort(InfoTable::Services),
            Some((InfoSortCol::Status, SortDir::Desc)),
            "the header click must land on the shell-owned sort slot"
        );
        let rows = view.services_rows();
        let names: Vec<&str> = rows.iter().map(|s| s.name.as_str()).collect();
        // Descending reverses the shell status rank (Active < Inactive <
        // Failed < Unknown ascending), so Failed ranks first here.
        assert_eq!(names, vec!["Alpha", "Gamma", "Beta"]);
    });
    // A same-column second click cycles the widget to Ascending; the slot
    // follows and the memo rebuilds again.
    table.update(cx, |table, cx| table.perform_sort(0, cx));
    root.update(cx, |view, _| {
        assert_eq!(
            view.inventory_sort(InfoTable::Services),
            Some((InfoSortCol::Status, SortDir::Asc))
        );
        let rows = view.services_rows();
        let names: Vec<&str> = rows.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Beta", "Gamma", "Alpha"]);
    });
    // The third click cycles back to Unsorted: the slot returns to provider
    // order (`None`) — the unsort affordance is real.
    table.update(cx, |table, cx| table.perform_sort(0, cx));
    root.update(cx, |view, _| {
        assert_eq!(view.inventory_sort(InfoTable::Services), None);
        let rows = view.services_rows();
        let names: Vec<&str> = rows.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Beta", "Alpha", "Gamma"]);
    });
}

/// Horizontal-navigation (landscape) regression: the Services page must
/// keep its action bar, table surface and status bar inside the window.
/// This catches flex items growing to the table's intrinsic column width
/// instead of clipping inside the page's own scroll container.
#[gpui::test]
async fn landscape_services_page_keeps_table_and_chrome_inside_window(cx: &mut TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Services;
        let services = (1..=8)
            .map(|i| {
                service(
                    &format!("svc-{i}.service"),
                    &format!("Landscape Service {i}"),
                    ServiceStatus::Active,
                )
            })
            .collect();
        view.replace_services_for_test(services, Vec::new());
        cx.notify();
    })
    .unwrap();
    for (width, height) in [
        (1280.0f32, 720.0f32),
        (1193.0, 815.0),
        (2386.0, 1631.0),
        (800.0, 480.0),
        (720.0, 480.0),
    ] {
        cx.simulate_window_resize(win.into(), size(px(width), px(height)));
        cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();

        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        let inside_window = |origin_x: f32,
                             origin_y: f32,
                             size_w: f32,
                             size_h: f32,
                             label: &str| {
            assert!(
                origin_x >= -0.5
                    && origin_x + size_w <= width + 0.5
                    && origin_y >= -0.5
                    && origin_y + size_h <= height + 0.5,
                "{label} must stay inside the {width}x{height} landscape window: x={origin_x}, y={origin_y}, w={size_w}, h={size_h}"
            );
        };

        for (id, label) in [
            ("tm-table", "Services table surface"),
            ("tm-svc-action-bar", "Services action bar"),
            ("tm-search-box", "Services search box"),
            ("tm-status-bar", "Services status bar"),
            ("nav-orientation-btn", "navigation orientation button"),
            ("settings-btn", "settings gear"),
        ] {
            let bounds = vcx.debug_bounds(id).unwrap_or_else(|| {
                panic!("{label} must render in horizontal-navigation landscape layout")
            });
            inside_window(
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
                label,
            );
        }
        drop(vcx);
    }
}
