//! Measured bottom/right bounds for the compact GPUI inventory tables.

use gpui::{AppContext, Modifiers, MouseButton, TestAppContext, VisualTestContext, px, size};

use crate::gpui_app::list_view::LIST_PAGE_SCAFFOLD_SELECTOR;
use crate::gpui_app::root::{RootView, TopPage};
use taskforest_core::core::services::{ServiceItem, ServiceStatus};
use taskforest_core::core::session::SessionItem;
use taskforest_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupImpact, StartupImpactEvidence, StartupScope,
    StartupSource,
};
use taskmanager_theme::Theme;

const VIEWPORTS: [(f32, f32); 5] = [
    (720.0, 480.0),
    (860.0, 600.0),
    (1180.0, 780.0),
    (1280.0, 720.0),
    (1600.0, 540.0),
];

fn service(ix: usize) -> ServiceItem {
    ServiceItem::from_inventory(
        format!("fixture.service.{ix}"),
        format!("fixture-very-long-systemd-unit-name-{ix}.service"),
        ServiceStatus::Active,
        "fixture service with a long description for elastic clipping",
        "loaded",
        "active",
        "running",
    )
}

fn startup_entry(ix: usize) -> StartupEntry {
    StartupEntry {
        id: format!("desktop:table-bounds-{ix}.desktop").into(),
        name: format!("Startup table bounds fixture {ix:02}"),
        exec: format!(
            "/usr/lib/a-very-long-startup-command-path/table-bounds-{ix} --with-arguments"
        ),
        enabled: ix.is_multiple_of(2),
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: format!("table-bounds-{ix}.desktop").into(),
        impact: StartupImpact::Low,
        impact_evidence: StartupImpactEvidence::Measured { duration_ms: 24 },
    }
}

fn session(ix: usize) -> SessionItem {
    SessionItem {
        id: format!("session-{ix}").into(),
        user: format!("table-bounds-user-{ix}"),
        ..SessionItem::default()
    }
}

fn draw(cx: &mut TestAppContext, window: gpui::WindowHandle<RootView>) {
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .expect("inventory window must draw");
}

fn install(
    cx: &mut TestAppContext,
    page: TopPage,
    width: f32,
    height: f32,
) -> gpui::WindowHandle<RootView> {
    let window = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    cx.simulate_window_resize(window.into(), size(px(width), px(height)));
    let view = window.entity(cx).expect("RootView entity");
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = page;
        match page {
            TopPage::Services => {
                view.replace_services_for_test((0..24).map(service).collect(), Vec::new());
            }
            TopPage::Startup => {
                view.replace_startup_for_test((0..24).map(startup_entry).collect(), Vec::new());
            }
            TopPage::Users => {
                view.replace_sessions_for_test((0..24).map(session).collect(), Vec::new());
            }
            _ => unreachable!("inventory bounds test only installs inventory pages"),
        }
        cx.notify();
    });
    draw(cx, window);
    window
}

fn last_visible_row(
    visual: &mut VisualTestContext,
    selectors: &[&'static str],
    clip: gpui::Bounds<gpui::Pixels>,
) -> gpui::Bounds<gpui::Pixels> {
    selectors
        .iter()
        .filter_map(|selector| visual.debug_bounds(selector))
        .filter(|bounds| bounds.origin.y < clip.bottom() && bounds.bottom() > clip.origin.y)
        .last()
        .unwrap_or_else(|| panic!("at least one inventory row must render"))
}

const SERVICE_ROWS: [&str; 24] = [
    "tm-svc-row:0",
    "tm-svc-row:1",
    "tm-svc-row:2",
    "tm-svc-row:3",
    "tm-svc-row:4",
    "tm-svc-row:5",
    "tm-svc-row:6",
    "tm-svc-row:7",
    "tm-svc-row:8",
    "tm-svc-row:9",
    "tm-svc-row:10",
    "tm-svc-row:11",
    "tm-svc-row:12",
    "tm-svc-row:13",
    "tm-svc-row:14",
    "tm-svc-row:15",
    "tm-svc-row:16",
    "tm-svc-row:17",
    "tm-svc-row:18",
    "tm-svc-row:19",
    "tm-svc-row:20",
    "tm-svc-row:21",
    "tm-svc-row:22",
    "tm-svc-row:23",
];

const STARTUP_ROWS: [&str; 24] = [
    "tm-startup-row:0",
    "tm-startup-row:1",
    "tm-startup-row:2",
    "tm-startup-row:3",
    "tm-startup-row:4",
    "tm-startup-row:5",
    "tm-startup-row:6",
    "tm-startup-row:7",
    "tm-startup-row:8",
    "tm-startup-row:9",
    "tm-startup-row:10",
    "tm-startup-row:11",
    "tm-startup-row:12",
    "tm-startup-row:13",
    "tm-startup-row:14",
    "tm-startup-row:15",
    "tm-startup-row:16",
    "tm-startup-row:17",
    "tm-startup-row:18",
    "tm-startup-row:19",
    "tm-startup-row:20",
    "tm-startup-row:21",
    "tm-startup-row:22",
    "tm-startup-row:23",
];

const USER_ROWS: [&str; 24] = [
    "tm-users-row:0",
    "tm-users-row:1",
    "tm-users-row:2",
    "tm-users-row:3",
    "tm-users-row:4",
    "tm-users-row:5",
    "tm-users-row:6",
    "tm-users-row:7",
    "tm-users-row:8",
    "tm-users-row:9",
    "tm-users-row:10",
    "tm-users-row:11",
    "tm-users-row:12",
    "tm-users-row:13",
    "tm-users-row:14",
    "tm-users-row:15",
    "tm-users-row:16",
    "tm-users-row:17",
    "tm-users-row:18",
    "tm-users-row:19",
    "tm-users-row:20",
    "tm-users-row:21",
    "tm-users-row:22",
    "tm-users-row:23",
];

#[gpui::test]
async fn inventory_tables_protect_the_bottom_and_right_edges(cx: &mut TestAppContext) {
    for (width, height) in VIEWPORTS {
        for (page, row_selectors) in [
            (TopPage::Services, SERVICE_ROWS.as_slice()),
            (TopPage::Startup, STARTUP_ROWS.as_slice()),
            (TopPage::Users, USER_ROWS.as_slice()),
        ] {
            let window = install(cx, page, width, height);
            let mut visual = VisualTestContext::from_window(window.into(), cx);
            let viewport = visual
                .debug_bounds("tm-telemetry-ready-body")
                .expect("page viewport must render");
            let scaffold = visual
                .debug_bounds(LIST_PAGE_SCAFFOLD_SELECTOR)
                .expect("list scaffold must render");
            let table = visual.debug_bounds("tm-table").expect("table must render");
            let scrollbar = visual
                .debug_bounds("tm-list-page-table-scrollbar")
                .expect("horizontal scrollbar must have its own bottom band");
            let last = last_visible_row(&mut visual, row_selectors, table);

            assert!(
                table.right() <= viewport.right() + px(0.5),
                "{width}x{height} {page:?} table leaves the right edge: {table:?}, viewport={viewport:?}"
            );
            assert!(
                last.bottom() <= scrollbar.origin.y + px(0.5)
                    && scrollbar.origin.y >= table.bottom() - px(0.5),
                "{width}x{height} {page:?} row and scrollbar bands overlap: row={last:?}, table={table:?}, scrollbar={scrollbar:?}"
            );
            assert!(
                scaffold.bottom() <= viewport.bottom() + px(0.5),
                "{width}x{height} {page:?} scaffold leaves the bottom: {scaffold:?}, viewport={viewport:?}"
            );
        }
    }
}

#[gpui::test]
async fn startup_command_column_fits_the_measured_compact_content_slot(cx: &mut TestAppContext) {
    let (width, height) = (720.0, 480.0);
    let window = install(cx, TopPage::Startup, width, height);
    let view = window.entity(cx).expect("RootView entity");
    let (name_width, command_width, command_bounds) = view.read_with(cx, |view, cx| {
        let table = view.startup_table.as_ref().expect("startup table entity");
        let table = table.read(cx);
        (
            table.col_groups[1].width,
            table.col_groups[4].width,
            table.col_groups[4].bounds,
        )
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let viewport = visual
        .debug_bounds("tm-telemetry-ready-body")
        .expect("page viewport must render");
    let table = visual.debug_bounds("tm-table").expect("table must render");
    let command_header = visual
        .debug_bounds("tm-startup-command-header-label")
        .expect("startup command header label must render");
    let command_cell = visual
        .debug_bounds("tm-startup-command-cell:0")
        .expect("startup command body cell must render");
    let command_text = visual
        .debug_bounds("tm-startup-command-text:0")
        .expect("startup command body text must render");

    assert!(
        command_width <= px(f32::from(viewport.size.width)),
        "compact command column must fit the measured content slot: width={command_width:?}, viewport={viewport:?}"
    );
    assert_eq!(
        command_width,
        px(88.0),
        "720x480 Startup must reserve the complete Command header label"
    );
    assert!(
        name_width < px(280.0),
        "720x480 Startup must reclaim width from the elastic name column"
    );
    assert!(
        table.right() <= viewport.right() + px(0.5)
            && command_bounds.right() <= viewport.right() + px(0.5),
        "720x480 Startup table and command column must fit the measured viewport: table={table:?}, command={command_bounds:?}, viewport={viewport:?}"
    );
    assert!(
        command_header.origin.x >= command_bounds.origin.x + px(8.0) - px(0.5)
            && command_header.right() <= command_bounds.right() - px(8.0) + px(0.5)
            && command_header.right() <= viewport.right() + px(0.5),
        "720x480 Startup Command heading must fit inside its padded, visible column: label={command_header:?}, column={command_bounds:?}, viewport={viewport:?}"
    );
    assert!(
        command_cell.right() <= command_bounds.right() + px(0.5)
            && command_text.right() <= command_cell.right() + px(0.5),
        "720x480 Startup command text must truncate within its allocated column: text={command_text:?}, cell={command_cell:?}, column={command_bounds:?}"
    );
}

#[gpui::test]
async fn startup_row_tooltip_preserves_adjacent_rows_inside_compact_and_wide_viewports(
    cx: &mut TestAppContext,
) {
    for (width, height) in [(720.0, 480.0), (1600.0, 900.0)] {
        let window = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
        cx.simulate_window_resize(window.into(), size(px(width), px(height)));
        let view = window.entity(cx).expect("RootView entity");
        view.update(cx, |view, cx| {
            view.mark_telemetry_frame_ready();
            view.page = TopPage::Startup;
            view.replace_startup_for_test((0..2).map(startup_entry).collect(), Vec::new());
            cx.notify();
        });
        draw(cx, window);

        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let hovered_row = visual
            .debug_bounds("tm-startup-row:0")
            .expect("first startup row must render");
        let following_row = visual
            .debug_bounds("tm-startup-row:1")
            .expect("following startup row must render");
        visual.simulate_mouse_move(hovered_row.center(), None::<MouseButton>, Modifiers::none());
        draw(cx, window);

        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let viewport = visual
            .debug_bounds("tm-telemetry-ready-body")
            .expect("startup viewport must render");
        let tooltip = visual
            .debug_bounds("tm-tooltip-overlay")
            .expect("hovering a startup row must mount its tooltip");
        assert!(
            tooltip.origin.x >= px(0.0)
                && tooltip.origin.y >= px(0.0)
                && tooltip.right() <= px(width)
                && tooltip.bottom() <= px(height),
            "{width}x{height} tooltip must remain within the window: tooltip={tooltip:?}"
        );
        assert!(
            !tooltip.intersects(&hovered_row),
            "{width}x{height} tooltip must preserve the hovered row: tooltip={tooltip:?}, row={hovered_row:?}"
        );
        assert!(
            !tooltip.intersects(&following_row),
            "{width}x{height} tooltip must preserve the following row: tooltip={tooltip:?}, row={following_row:?}"
        );
        assert!(
            tooltip.origin.x >= viewport.origin.x
                && tooltip.origin.y >= viewport.origin.y
                && tooltip.right() <= viewport.right() + px(0.5)
                && tooltip.bottom() <= viewport.bottom() + px(0.5),
            "{width}x{height} tooltip must stay inside the measured page viewport: tooltip={tooltip:?}, viewport={viewport:?}"
        );
    }
}
