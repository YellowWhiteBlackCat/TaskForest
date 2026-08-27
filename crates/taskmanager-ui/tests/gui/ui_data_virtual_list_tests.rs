use super::{
    DeferredScrollRequest, SizeLayout, VirtualList, VirtualListScrollHandle, build_size_layout,
    resolve_deferred_scroll, v_virtual_list, visible_range_for,
};
use gpui::{
    AppContext, Axis, Bounds, Context, Div, Entity, IntoElement, Pixels, Render, ScrollStrategy,
    Size, Styled, TestAppContext, Window, div, point, px, size,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

fn pxv(v: f32) -> gpui::Pixels {
    px(v)
}

#[test]
fn prefix_sums_include_gap_and_skip_last() {
    let layout = build_size_layout(&[pxv(10.0), pxv(20.0), pxv(30.0)], pxv(2.0));
    assert_eq!(
        layout,
        SizeLayout {
            sizes: vec![pxv(12.0), pxv(22.0), pxv(30.0)],
            origins: vec![pxv(0.0), pxv(12.0), pxv(34.0)],
            content_size: pxv(64.0),
        }
    );
    // Empty input is a valid zero layout.
    let empty = build_size_layout(&[], pxv(2.0));
    assert!(empty.sizes.is_empty() && empty.content_size == pxv(0.0));
}

#[test]
fn visible_scan_starts_at_first_item_that_crosses_the_start_edge() {
    // offset 0: item 0 always crosses the start edge.
    let sizes = [pxv(100.0), pxv(100.0), pxv(100.0)];
    assert_eq!(visible_range_for(&sizes, 0.0, 250.0, 0.0), 0..3);
    assert_eq!(visible_range_for(&sizes, 0.0, 100.0, 0.0), 0..2);
    // Scrolled 100px: item 1 starts the viewport; item 2 ends exactly
    // at the far edge and is the scan's first crossing item, so it is
    // included as the exclusive end of the range.
    assert_eq!(visible_range_for(&sizes, -100.0, 100.0, 0.0), 1..3);
    // Leading padding shifts the start edge inward; item 0 still
    // crosses it (partially visible), so it stays in range.
    assert_eq!(visible_range_for(&sizes, -50.0, 100.0, 10.0), 0..2);
}

#[test]
fn visible_scan_single_item_boundaries() {
    // Exactly one item, exactly filled: fully visible (A-11 lock).
    let one = [pxv(100.0)];
    assert_eq!(visible_range_for(&one, 0.0, 100.0, 0.0), 0..1);
    // Smaller viewport still shows the single item.
    assert_eq!(visible_range_for(&one, 0.0, 50.0, 0.0), 0..1);
    // Oversized item: still exactly one.
    let huge = [pxv(500.0)];
    assert_eq!(visible_range_for(&huge, 0.0, 100.0, 0.0), 0..1);
    // Scrolled most of the way through a huge item: still visible.
    assert_eq!(visible_range_for(&huge, -400.0, 100.0, 0.0), 0..1);
}

#[test]
fn visible_scan_all_items_fit_means_all_visible() {
    // gc's `last == 0` special case would collapse this to 0..0; our
    // uniform rule returns the full range.
    let sizes = [pxv(50.0), pxv(50.0)];
    assert_eq!(visible_range_for(&sizes, 0.0, 200.0, 0.0), 0..2);
    assert_eq!(visible_range_for(&sizes, 0.0, 120.0, 0.0), 0..2);
}

#[test]
fn visible_scan_scrolled_past_end_is_empty() {
    let sizes = [pxv(100.0), pxv(100.0)];
    assert_eq!(visible_range_for(&sizes, -300.0, 100.0, 0.0), 2..2);
    // Empty sizes.
    assert_eq!(visible_range_for(&[], 0.0, 100.0, 0.0), 0..0);
    // Zero viewport.
    assert_eq!(visible_range_for(&sizes, 0.0, 0.0, 0.0), 0..0);
}

#[test]
fn deferred_scroll_top_bottom_aligns_only_out_of_view_edges() {
    let content = Bounds {
        origin: point(px(0.0), px(0.0)),
        size: size(px(300.0), px(300.0)),
    };
    let item = Bounds {
        origin: point(px(0.0), px(400.0)),
        size: size(px(300.0), px(20.0)),
    };
    // Item below the viewport bottom: Top aligns its top edge.
    let resolved = resolve_deferred_scroll(
        Axis::Vertical,
        point(px(0.0), px(0.0)),
        item,
        &content,
        DeferredScrollRequest {
            item_index: 4,
            strategy: ScrollStrategy::Top,
            offset_items: 0,
        },
    );
    assert_eq!(resolved.y, -px(400.0));
    // Item already inside: no movement.
    let inside = Bounds {
        origin: point(px(0.0), px(50.0)),
        size: size(px(300.0), px(20.0)),
    };
    let resolved = resolve_deferred_scroll(
        Axis::Vertical,
        point(px(0.0), px(0.0)),
        inside,
        &content,
        DeferredScrollRequest {
            item_index: 1,
            strategy: ScrollStrategy::Top,
            offset_items: 0,
        },
    );
    assert_eq!(resolved, point(px(0.0), px(0.0)));
    // Bottom strategy on an item above the viewport aligns its bottom
    // edge (item spans -100..-80, viewport 0..300).
    let above = Bounds {
        origin: point(px(0.0), px(-100.0)),
        size: size(px(300.0), px(20.0)),
    };
    let resolved = resolve_deferred_scroll(
        Axis::Vertical,
        point(px(0.0), px(0.0)),
        above,
        &content,
        DeferredScrollRequest {
            item_index: 1,
            strategy: ScrollStrategy::Bottom,
            offset_items: 0,
        },
    );
    assert_eq!(resolved.y, px(300.0) - px(-80.0));
    // Bottom on an item already inside the viewport: no movement
    // (non-strict).
    let resolved = resolve_deferred_scroll(
        Axis::Vertical,
        point(px(0.0), px(0.0)),
        inside,
        &content,
        DeferredScrollRequest {
            item_index: 1,
            strategy: ScrollStrategy::Bottom,
            offset_items: 0,
        },
    );
    assert_eq!(resolved, point(px(0.0), px(0.0)));
    // Center centers the item.
    let resolved = resolve_deferred_scroll(
        Axis::Vertical,
        point(px(0.0), px(0.0)),
        inside,
        &content,
        DeferredScrollRequest {
            item_index: 1,
            strategy: ScrollStrategy::Center,
            offset_items: 0,
        },
    );
    assert_eq!(resolved.y, px(150.0) - px(60.0));
}

#[test]
fn visible_scan_perf_smoke_100k_items() {
    let sizes: Vec<gpui::Pixels> = (0..100_000).map(|i| px(10.0 + (i % 7) as f32)).collect();
    let start = Instant::now();
    for _ in 0..20 {
        let range = visible_range_for(&sizes, -500_000.0, 800.0, 0.0);
        assert!(range.end >= range.start && range.end <= sizes.len());
    }
    // Two linear scans over 100k items, 20 times: must be far under a
    // second even on slow CI machines.
    assert!(start.elapsed().as_millis() < 2000, "scan too slow");
}

/// 附录 A-12 lock: `scroll_to_item(0, …)` goes through the same deferred
/// protocol as every other index — gc had a `(0,0)` shortcut that
/// bypassed the deferred request.
#[test]
fn scroll_to_item_zero_uses_the_same_deferred_protocol_as_any_index() {
    let handle = VirtualListScrollHandle::new();
    assert_eq!(handle.deferred_scroll(), None);

    handle.scroll_to_item(0, ScrollStrategy::Top);
    assert_eq!(
        handle.deferred_scroll(),
        Some(DeferredScrollRequest {
            item_index: 0,
            strategy: ScrollStrategy::Top,
            offset_items: 0,
        }),
        "index 0 must produce a deferred request, not a shortcut"
    );

    // Non-zero indices behave identically (no special case at 0).
    handle.scroll_to_item(3, ScrollStrategy::Center);
    assert_eq!(
        handle.deferred_scroll(),
        Some(DeferredScrollRequest {
            item_index: 3,
            strategy: ScrollStrategy::Center,
            offset_items: 0,
        })
    );
}

/// 附录 A-13 lock: when the render closure produces no item, the
/// cross-axis measurement falls back to `Size::default()` — gc measured
/// a phantom element and miscomputed the whole list height. A closure
/// that does render an item measures its real size.
#[gpui::test]
async fn empty_item_render_measures_to_default_size(cx: &mut TestAppContext) {
    struct Root;
    impl Render for Root {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    // Measurement must run inside the window's layout phase (gpui
    // rejects `layout_as_root` outside request_layout/prepaint/paint),
    // so the harness performs the measurement during its own render.
    struct MeasureHarness {
        view: Entity<Root>,
        results: Rc<RefCell<Vec<Size<Pixels>>>>,
    }
    impl Render for MeasureHarness {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let empty: VirtualList = v_virtual_list(
                self.view.clone(),
                "empty",
                Rc::new(vec![px(50.0), px(50.0)]),
                |_, _, _, _| -> Vec<Div> { Vec::new() },
            );
            let empty_measured = empty.measure_item(None, window, cx);
            let no_items: VirtualList = v_virtual_list(
                self.view.clone(),
                "no-items",
                Rc::new(Vec::new()),
                |_, _, _, _| -> Vec<Div> { Vec::new() },
            );
            let no_items_measured = no_items.measure_item(None, window, cx);
            let sized: VirtualList = v_virtual_list(
                self.view.clone(),
                "sized",
                Rc::new(vec![px(50.0)]),
                |_, _, _, _| vec![div().w(px(40.0)).h(px(30.0))],
            );
            let sized_measured = sized.measure_item(None, window, cx);
            *self.results.borrow_mut() = vec![empty_measured, no_items_measured, sized_measured];
            div()
        }
    }

    let view = cx.new(|_| Root);
    let results = Rc::new(RefCell::new(Vec::new()));
    let window = cx.add_window(|_window, _cx| MeasureHarness {
        view,
        results: results.clone(),
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let results = results.borrow();
    assert_eq!(
        results[0],
        Size::default(),
        "empty render must fall back to a zero placeholder size"
    );
    assert_eq!(
        results[1],
        Size::default(),
        "a zero-item list must also measure as zero"
    );
    assert_eq!(results[2], size(px(40.0), px(30.0)));
}
