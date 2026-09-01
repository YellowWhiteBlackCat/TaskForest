//! Landscape (horizontal-navigation) layout regressions for the Apps page.

use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px, size};

use crate::gpui_app::root::{NavOrientation, TopPage};
use taskmanager_shell::SortCol;

/// The returned rails must drive the real GPUI handles, not merely register a
/// debug rectangle. A compact, full-column Apps table exercises both axes:
/// clicking the lower track moves the virtual list, and clicking the right
/// horizontal track moves the shared header/body offset.
#[gpui::test]
async fn apps_scrollbars_move_their_real_handles(cx: &mut TestAppContext) {
    let (win, view) = super::wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(720.0), px(480.0)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.processes_state.hidden_cols.clear();
        v.replace_processes_for_test(
            (1..=80)
                .map(|pid| {
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(pid)
                        .name(format!("scroll-worker-{pid}"))
                        .build()
                })
                .collect(),
        );
        cx.notify();
    });
    super::draw(cx, win);

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let htrack = vcx
        .debug_bounds("tm-procs-hscroll-track")
        .expect("full compact columns must expose a horizontal track");
    let vrail = vcx
        .debug_bounds("tm-procs-vscrollbar")
        .expect("the virtual process list must expose a vertical rail");

    let hpoint = point(htrack.right() - px(2.0), htrack.center().y);
    vcx.simulate_mouse_move(hpoint, None::<MouseButton>, Modifiers::none());
    vcx.simulate_click(hpoint, Modifiers::none());
    assert!(
        view.read_with(cx, |v, _| v.processes_scroll.horizontal.offset().x
            < px(0.0)),
        "horizontal track click must update the shared header/body offset"
    );

    let vpoint = point(vrail.center().x, vrail.bottom() - px(6.0));
    vcx.simulate_mouse_move(vpoint, None::<MouseButton>, Modifiers::none());
    vcx.simulate_click(vpoint, Modifiers::none());
    assert!(
        view.read_with(cx, |v, _| {
            v.processes_scroll
                .vertical
                .0
                .borrow()
                .base_handle
                .offset()
                .y
                < px(0.0)
        }),
        "vertical rail click must update the uniform-list offset"
    );
}

/// The Apps process table mounts the shared reference resize handle rather
/// than a page-local drag implementation. A real pointer sequence proves the
/// typed payload, first-motion anchor, live width update, and release path are
/// still wired through `RootView`.
#[gpui::test]
async fn apps_column_edge_drag_updates_the_live_width(cx: &mut TestAppContext) {
    let (win, view) = super::wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(1280.0), px(720.0)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.processes_state.hidden_cols.clear();
        v.replace_processes_for_test(
            (1..=8)
                .map(|pid| {
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(pid)
                        .name(format!("resize-worker-{pid}"))
                        .build()
                })
                .collect(),
        );
        cx.notify();
    });
    super::draw(cx, win);

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let handle = vcx
        .debug_bounds("tm-proc-resize-h:6")
        .expect("the visible CPU column must expose the shared resize handle");
    let start = handle.center();
    let before = view.read_with(cx, |v, _| v.proc_col_width(SortCol::Cpu));

    vcx.simulate_mouse_move(start, None::<MouseButton>, Modifiers::none());
    vcx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    // The first movement promotes the drag; the next movement is the first
    // width delta after the shared session anchor is captured.
    vcx.simulate_mouse_move(
        point(start.x + px(2.0), start.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    vcx.simulate_mouse_move(
        point(start.x + px(42.0), start.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    vcx.simulate_mouse_move(
        point(start.x + px(43.0), start.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    vcx.simulate_mouse_move(
        point(start.x + px(83.0), start.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    vcx.simulate_mouse_up(
        point(px(2000.0), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );

    let after = view.read_with(cx, |v, _| v.proc_col_width(SortCol::Cpu));
    assert_eq!(
        after,
        before + px(40.0),
        "the shared column drag must apply the pointer delta to the live width"
    );
    assert!(
        view.read_with(cx, |v, _| v.processes_state.resize_anchor_x.is_none()),
        "releasing outside the window must close the page adapter's drag session"
    );
}

/// Horizontal-navigation (landscape) regression: the Apps page must keep its
/// action bar, complete table (including its axis-specific rails), status bar
/// and right-end nav buttons inside the window. A compact width must expose a
/// real horizontal rail when the enabled columns exceed the viewport; a wide
/// width must not reserve a misleading empty rail.
#[gpui::test]
async fn landscape_apps_page_keeps_table_and_chrome_inside_window(cx: &mut TestAppContext) {
    let (win, view) = super::wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.replace_processes_for_test(
            (1..=8)
                .map(|pid| {
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(pid)
                        .name(format!("landscape-worker-{pid}"))
                        .build()
                })
                .collect(),
        );
        cx.notify();
    });
    for (width, height) in [
        (1280.0f32, 720.0f32),
        (1193.0, 815.0),
        (2386.0, 1631.0),
        (800.0, 480.0),
        (720.0, 480.0),
    ] {
        cx.simulate_window_resize(win.into(), size(px(width), px(height)));
        super::draw(cx, win);

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

        let scroll = vcx
            .debug_bounds("tm-procs-table-scroll")
            .expect("the Apps table must expose its horizontal scroll container");
        inside_window(
            f32::from(scroll.origin.x),
            f32::from(scroll.origin.y),
            f32::from(scroll.size.width),
            f32::from(scroll.size.height),
            "Apps table scroll container",
        );
        let action = vcx
            .debug_bounds("tm-proc-action-bar")
            .expect("the Apps page must expose its action bar");
        inside_window(
            f32::from(action.origin.x),
            f32::from(action.origin.y),
            f32::from(action.size.width),
            f32::from(action.size.height),
            "Apps action bar",
        );
        let strip = vcx
            .debug_bounds("tm-navigation-strip")
            .expect("horizontal navigation must expose its bounded strip");
        let nav_ids = vec![
            "Performance",
            "Apps",
            "Services",
            "Startup",
            "Users",
            "App history",
            "Containers",
            "System",
            "nav-orientation-btn",
            "settings-btn",
        ];
        #[cfg(target_os = "linux")]
        let nav_ids = nav_ids
            .into_iter()
            .chain(std::iter::once("window-capture-btn"));
        #[cfg(not(target_os = "linux"))]
        let nav_ids = nav_ids.into_iter();
        for id in nav_ids {
            let bounds = vcx
                .debug_bounds(id)
                .unwrap_or_else(|| panic!("{id} must render in horizontal navigation"));
            assert!(
                bounds.origin.x >= strip.origin.x - px(0.5)
                    && bounds.origin.x + bounds.size.width
                        <= strip.origin.x + strip.size.width + px(0.5)
                    && bounds.origin.y >= strip.origin.y - px(0.5)
                    && bounds.origin.y + bounds.size.height
                        <= strip.origin.y + strip.size.height + px(0.5),
                "{id} must stay inside the navigation strip: strip={strip:?}, bounds={bounds:?}"
            );
        }
        for (id, label) in [
            ("tm-proc-mode-switcher", "Apps mode switcher"),
            ("tm-proc-status-filter", "Apps status filter row"),
        ] {
            let bounds = vcx
                .debug_bounds(id)
                .unwrap_or_else(|| panic!("{label} must render in the landscape Apps page"));
            inside_window(
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
                label,
            );
        }
        let search = vcx
            .debug_bounds("tm-search-box")
            .expect("the Apps page must expose its search box");
        inside_window(
            f32::from(search.origin.x),
            f32::from(search.origin.y),
            f32::from(search.size.width),
            f32::from(search.size.height),
            "Apps search box",
        );
        let horizontal_track = vcx.debug_bounds("tm-procs-hscroll-track");
        if width <= 800.0 {
            assert!(
                horizontal_track.is_some(),
                "compact Apps content must mount the returned horizontal scrollbar"
            );
        } else {
            assert!(
                horizontal_track.is_none(),
                "a fitting Apps table must not reserve a misleading horizontal scrollbar"
            );
        }
        let status = vcx
            .debug_bounds("tm-status-bar")
            .expect("the Apps page must keep its status bar");
        inside_window(
            f32::from(status.origin.x),
            f32::from(status.origin.y),
            f32::from(status.size.width),
            f32::from(status.size.height),
            "Apps status bar",
        );
        for (id, label) in [
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

/// Vertical navigation is a fixed-width rail: every tab must stay inside the
/// rail and the page body must own the remaining width. This catches the
/// opposite flex mistake from the horizontal case, where intrinsic tab height
/// or a duplicated flex spacer can push controls or page content outside the
/// viewport.
#[gpui::test]
async fn vertical_navigation_keeps_tabs_inside_the_rail(cx: &mut TestAppContext) {
    let (win, view) = super::wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.nav_orientation = NavOrientation::Vertical;
        v.page = TopPage::Apps;
        v.replace_processes_for_test(
            (1..=8)
                .map(|pid| {
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(pid)
                        .name(format!("vertical-worker-{pid}"))
                        .build()
                })
                .collect(),
        );
        cx.notify();
    });

    for (width, height) in [(1280.0f32, 720.0f32), (800.0, 480.0)] {
        cx.simulate_window_resize(win.into(), size(px(width), px(height)));
        super::draw(cx, win);

        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        let rail = vcx
            .debug_bounds("tm-navigation-rail")
            .expect("vertical navigation must expose a bounded rail");
        let body = vcx
            .debug_bounds("tm-procs-table-scroll")
            .expect("the Apps body must render beside the vertical rail");
        assert!(
            body.origin.x >= rail.origin.x + rail.size.width - px(0.5),
            "the body must start after the fixed navigation rail: rail={rail:?}, body={body:?}"
        );
        assert!(
            body.origin.x + body.size.width <= px(width) + px(0.5),
            "the body must stay inside the window: body={body:?}, width={width}"
        );
        let first_row = vcx
            .debug_bounds("tm-proc-row-root:0")
            .expect("vertical Apps navigation must not collapse the process rows");
        assert!(
            first_row.size.height > px(20.0)
                && first_row.origin.y >= body.origin.y - px(0.5)
                && first_row.origin.y + first_row.size.height
                    <= body.origin.y + body.size.height + px(0.5),
            "the first process row must have a bounded visible slot beside the vertical rail: body={body:?}, row={first_row:?}"
        );

        let nav_ids = vec![
            "Performance",
            "Apps",
            "Services",
            "Startup",
            "Users",
            "App history",
            "Containers",
            "System",
            "nav-orientation-btn",
            "settings-btn",
        ];
        #[cfg(target_os = "linux")]
        let nav_ids = nav_ids
            .into_iter()
            .chain(std::iter::once("window-capture-btn"));
        #[cfg(not(target_os = "linux"))]
        let nav_ids = nav_ids.into_iter();
        for id in nav_ids {
            let bounds = vcx
                .debug_bounds(id)
                .unwrap_or_else(|| panic!("{id} must render in vertical navigation"));
            assert!(
                bounds.origin.x >= rail.origin.x - px(0.5)
                    && bounds.origin.x + bounds.size.width
                        <= rail.origin.x + rail.size.width + px(0.5)
                    && bounds.origin.y >= rail.origin.y - px(0.5)
                    && bounds.origin.y + bounds.size.height
                        <= rail.origin.y + rail.size.height + px(0.5),
                "{id} must stay inside the navigation rail: rail={rail:?}, bounds={bounds:?}"
            );
        }
        drop(vcx);
    }
}
