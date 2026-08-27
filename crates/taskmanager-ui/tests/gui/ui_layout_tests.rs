use super::{
    AdaptiveGrid, BoundedScrollRailSpec, PageScaffold, bounded_scroll_column_with_fixed_header,
    bounded_scroll_region_with_rail, scroll_region, scroll_region_with_overlay_rail,
    scroll_region_with_rail,
};
use crate::primitives::scrollbar::SCROLLBAR_WIDTH;
use gpui::{
    AppContext, Context, InteractiveElement, IntoElement, Modifiers, ParentElement, Render,
    ScrollDelta, ScrollHandle, ScrollWheelEvent, Styled, TestAppContext, TouchPhase,
    VisualTestContext, Window, div, point, px, size,
};
use taskmanager_theme::{Theme, tokens};

fn card(selector: &'static str) -> gpui::Div {
    div()
        .w_full()
        .h(px(40.0))
        .debug_selector(move || selector.to_string())
}

fn draw_grid(cx: &mut gpui::VisualTestContext, width: f32) {
    cx.draw(
        point(px(0.0), px(0.0)),
        size(px(width), px(220.0)),
        |_, _| {
            div().w(px(width)).h(px(220.0)).child(
                AdaptiveGrid::new(px(220.0))
                    .gap(tokens::SPACE_8)
                    .debug_selector("tm-test-adaptive-grid")
                    .children([
                        card("tm-test-card-0"),
                        card("tm-test-card-1"),
                        card("tm-test-card-2"),
                    ]),
            )
        },
    );
}

#[gpui::test]
async fn adaptive_grid_wraps_cards_and_shrinks_the_single_column(cx: &mut TestAppContext) {
    let visual = cx.add_empty_window();
    draw_grid(visual, 520.0);

    let first = visual
        .debug_bounds("tm-test-card-0")
        .expect("first card should be laid out");
    let second = visual
        .debug_bounds("tm-test-card-1")
        .expect("second card should be laid out");
    let third = visual
        .debug_bounds("tm-test-card-2")
        .expect("third card should be laid out");

    assert_eq!(first.origin.x, px(0.0));
    assert!(
        second.origin.x > first.origin.x,
        "wide view should keep two cards on the first row"
    );
    assert!(
        third.origin.y > first.origin.y,
        "the next card should wrap to a new row"
    );
    assert_eq!(first.size.width, second.size.width);
    assert!(
        third.size.width > px(220.0),
        "the last row should fill its available width"
    );

    draw_grid(visual, 180.0);
    let narrow_first = visual
        .debug_bounds("tm-test-card-0")
        .expect("first narrow card should be laid out");
    let narrow_second = visual
        .debug_bounds("tm-test-card-1")
        .expect("second narrow card should be laid out");

    assert_eq!(narrow_first.origin.x, px(0.0));
    assert_eq!(narrow_second.origin.x, px(0.0));
    assert!(narrow_second.origin.y > narrow_first.origin.y);
    assert!(narrow_first.right() <= px(180.0));
    assert!(narrow_second.right() <= px(180.0));
}

#[gpui::test]
async fn page_scaffold_keeps_footer_pinned_after_body_fills_remaining_space(
    cx: &mut TestAppContext,
) {
    let visual = cx.add_empty_window();
    visual.draw(
        point(px(0.0), px(0.0)),
        size(px(420.0), px(240.0)),
        |_, _| {
            div().flex().flex_col().w(px(420.0)).h(px(240.0)).child(
                PageScaffold::new(
                    div()
                        .flex_1()
                        .w_full()
                        .debug_selector(|| "tm-test-page-body".to_string()),
                    px(tokens::SPACE_16.0),
                )
                .footer(
                    div()
                        .h(px(24.0))
                        .w_full()
                        .debug_selector(|| "tm-test-page-footer".to_string()),
                )
                .render(),
            )
        },
    );

    let body = visual
        .debug_bounds("tm-test-page-body")
        .expect("page body should be laid out");
    let footer = visual
        .debug_bounds("tm-test-page-footer")
        .expect("page footer should be laid out");

    assert_eq!(body.origin.x, px(tokens::SPACE_16.0));
    assert_eq!(body.size.width, px(420.0 - 2.0 * tokens::SPACE_16.0));
    assert!(body.size.height > px(0.0));
    assert_eq!(footer.size.height, px(24.0));
    assert_eq!(footer.bottom(), px(240.0));
    assert!(body.bottom() <= footer.top());
}

struct ScrollHarness {
    long_handle: ScrollHandle,
    short_handle: ScrollHandle,
}

impl Render for ScrollHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col().children([
            scroll_region(
                "tm-test-long-scroll",
                self.long_handle.clone(),
                div().w_full().h(px(600.0)),
            ),
            scroll_region(
                "tm-test-short-scroll",
                self.short_handle.clone(),
                div().w_full().h(px(60.0)),
            ),
        ])
    }
}

#[gpui::test]
async fn scroll_regions_keep_sibling_bounds_and_retain_real_overflow(cx: &mut TestAppContext) {
    let long_handle = ScrollHandle::new();
    let short_handle = ScrollHandle::new();
    let window = cx.add_window(|_window, _cx| ScrollHarness {
        long_handle: long_handle.clone(),
        short_handle: short_handle.clone(),
    });
    cx.simulate_window_resize(window.into(), size(px(320.0), px(180.0)));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    assert_eq!(long_handle.bounds().size.height, px(90.0));
    assert_eq!(short_handle.bounds().size.height, px(90.0));
    assert!(long_handle.max_offset().height > px(0.0));
    assert_eq!(short_handle.max_offset().height, px(0.0));
}

struct ScrollRailHarness {
    handle: ScrollHandle,
}

fn tall_scroll_body(marker: &'static str) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .h(px(600.0))
        .child(div().h(px(100.0)).flex_shrink_0())
        .child(
            div()
                .h(px(40.0))
                .flex_shrink_0()
                .debug_selector(move || marker.to_string()),
        )
        .child(div().h(px(460.0)).flex_shrink_0())
}

fn vertical_wheel(position: gpui::Point<gpui::Pixels>) -> ScrollWheelEvent {
    ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(0.0), px(-80.0))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    }
}

impl Render for ScrollRailHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(scroll_region_with_rail(
                "tm-test-rail-viewport",
                "tm-test-rail-viewport",
                "test-rail-scrollbar",
                "tm-test-rail",
                self.handle.clone(),
                Theme::dark().palette(),
                tall_scroll_body("tm-test-rail-content"),
            ))
    }
}

struct OverlayScrollRailHarness {
    handle: ScrollHandle,
}

impl Render for OverlayScrollRailHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(scroll_region_with_overlay_rail(
                "tm-overlay-rail-viewport",
                "tm-overlay-rail-viewport",
                "test-overlay-rail-scrollbar",
                "tm-overlay-rail",
                self.handle.clone(),
                Theme::dark().palette(),
                tall_scroll_body("tm-overlay-rail-content"),
            ))
    }
}

#[gpui::test]
async fn scroll_region_with_rail_moves_only_content_after_a_real_wheel(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let window = cx.add_window(|_window, _cx| ScrollRailHarness {
        handle: handle.clone(),
    });
    cx.simulate_window_resize(window.into(), size(px(220.0), px(180.0)));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let viewport_before = visual
        .debug_bounds("tm-test-rail-viewport")
        .expect("the rail viewport must remain addressable");
    let rail_before = visual
        .debug_bounds("tm-test-rail")
        .expect("the vertical rail must remain addressable");
    let marker_before = visual
        .debug_bounds("tm-test-rail-content")
        .expect("the scrolling marker must render");

    assert!(handle.max_offset().height > px(0.0));
    assert_eq!(rail_before.size.width, px(SCROLLBAR_WIDTH));
    assert_eq!(rail_before.right(), viewport_before.right());
    assert_eq!(rail_before.origin.y, viewport_before.top());
    assert_eq!(rail_before.bottom(), viewport_before.bottom());
    assert_eq!(
        viewport_before.size.width,
        px(220.0),
        "page rails keep the viewport width while reserving content width"
    );
    assert_eq!(
        marker_before.size.width,
        px(220.0 - tokens::SPACE_16.0),
        "page content must reserve the rail width"
    );
    assert_eq!(
        marker_before.right(),
        viewport_before.right() - px(tokens::SPACE_16.0)
    );

    let offset_before = handle.offset();
    visual.simulate_event(vertical_wheel(viewport_before.center()));
    let offset_after = handle.offset();
    assert!(
        offset_after.y < offset_before.y,
        "a real wheel must move the tracked content owner"
    );
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let viewport_after = visual
        .debug_bounds("tm-test-rail-viewport")
        .expect("the viewport must remain mounted after scrolling");
    let rail_after = visual
        .debug_bounds("tm-test-rail")
        .expect("the rail must remain mounted after scrolling");
    let marker_after = visual
        .debug_bounds("tm-test-rail-content")
        .expect("the scrolling marker must remain measurable");
    assert_eq!(viewport_after, viewport_before);
    assert_eq!(
        rail_after, rail_before,
        "the pinned rail must stay in window space while content scrolls"
    );
    assert_eq!(
        marker_after.origin.y - marker_before.origin.y,
        offset_after.y - offset_before.y,
        "content must consume the exact handle delta"
    );
}

#[gpui::test]
async fn overlay_rail_reserves_no_layout_width_and_still_scrolls(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let window = cx.add_window(|_window, _cx| OverlayScrollRailHarness {
        handle: handle.clone(),
    });
    cx.simulate_window_resize(window.into(), size(px(220.0), px(180.0)));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let viewport = visual
        .debug_bounds("tm-overlay-rail-viewport")
        .expect("the overlay viewport must render");
    let rail = visual
        .debug_bounds("tm-overlay-rail")
        .expect("the overlay rail must render");
    let marker = visual
        .debug_bounds("tm-overlay-rail-content")
        .expect("the overlay content must render");
    assert_eq!(viewport.size.width, px(220.0));
    assert_eq!(marker.size.width, px(220.0));
    assert_eq!(marker.right(), viewport.right());
    assert_eq!(rail.right(), viewport.right());

    let before = handle.offset();
    visual.simulate_event(vertical_wheel(viewport.center()));
    assert!(
        handle.offset().y < before.y,
        "overlay rail must not block the underlying scroll owner"
    );
}

struct BoundedScrollRailHarness {
    handle: ScrollHandle,
}

impl Render for BoundedScrollRailHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(bounded_scroll_region_with_rail(
                BoundedScrollRailSpec {
                    id: "tm-test-bounded-viewport-id",
                    viewport_selector: "tm-test-bounded-viewport",
                    scrollbar_id: "test-bounded-scrollbar",
                    scrollbar_selector: "tm-test-bounded-rail",
                    track_selector: "tm-test-bounded-track",
                    width: Some(px(180.0)),
                    max_height: px(140.0),
                    scroll: self.handle.clone(),
                    palette: Theme::dark().palette(),
                },
                tall_scroll_body("tm-test-bounded-content"),
            ))
    }
}

#[gpui::test]
async fn bounded_scroll_region_keeps_rail_fixed_after_a_real_wheel(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let window = cx.add_window(|_window, _cx| BoundedScrollRailHarness {
        handle: handle.clone(),
    });
    cx.simulate_window_resize(window.into(), size(px(220.0), px(220.0)));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let viewport_before = visual
        .debug_bounds("tm-test-bounded-viewport")
        .expect("the bounded viewport must render");
    let rail_before = visual
        .debug_bounds("tm-test-bounded-rail")
        .expect("the bounded rail must render");
    let marker_before = visual
        .debug_bounds("tm-test-bounded-content")
        .expect("the bounded content marker must render");
    assert_eq!(viewport_before.size.height, px(140.0));
    assert_eq!(viewport_before.size.width, px(180.0));
    assert_eq!(rail_before.origin.y, viewport_before.origin.y);
    assert_eq!(rail_before.bottom(), viewport_before.bottom());
    assert!(handle.max_offset().height > px(0.0));

    let offset_before = handle.offset();
    visual.simulate_event(vertical_wheel(viewport_before.center()));
    let offset_after = handle.offset();
    assert!(
        offset_after.y < offset_before.y,
        "a real wheel must move bounded dialog content"
    );
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let viewport_after = visual
        .debug_bounds("tm-test-bounded-viewport")
        .expect("the bounded viewport must remain mounted");
    let rail_after = visual
        .debug_bounds("tm-test-bounded-rail")
        .expect("the bounded rail must remain mounted");
    let marker_after = visual
        .debug_bounds("tm-test-bounded-content")
        .expect("the bounded content marker must remain measurable");
    assert_eq!(viewport_after, viewport_before);
    assert_eq!(
        rail_after, rail_before,
        "bounded dialog chrome must not enter the scrolling coordinate tree"
    );
    assert_eq!(
        marker_after.origin.y - marker_before.origin.y,
        offset_after.y - offset_before.y,
        "bounded content must consume the exact handle delta"
    );
}

struct FixedHeaderScrollHarness {
    handle: ScrollHandle,
}

impl Render for FixedHeaderScrollHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(bounded_scroll_column_with_fixed_header(
                BoundedScrollRailSpec {
                    id: "tm-test-fixed-scroll-id",
                    viewport_selector: "tm-test-fixed-scroll-viewport",
                    scrollbar_id: "test-fixed-scrollbar",
                    scrollbar_selector: "tm-test-fixed-scroll-rail",
                    track_selector: "tm-test-fixed-scroll-track",
                    width: Some(px(180.0)),
                    max_height: px(140.0),
                    scroll: self.handle.clone(),
                    palette: Theme::dark().palette(),
                },
                tokens::SPACE_12,
                div()
                    .h(px(28.0))
                    .w(px(72.0))
                    .flex_none()
                    .debug_selector(|| "tm-test-fixed-header".to_string()),
                tall_scroll_body("tm-test-fixed-scroll-content"),
            ))
    }
}

#[gpui::test]
async fn fixed_header_column_keeps_header_outside_the_real_scroll_owner(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let window = cx.add_window(|_window, _cx| FixedHeaderScrollHarness {
        handle: handle.clone(),
    });
    cx.simulate_window_resize(window.into(), size(px(220.0), px(220.0)));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let header_before = visual
        .debug_bounds("tm-test-fixed-header")
        .expect("fixed dialog chrome must remain addressable");
    let viewport_before = visual
        .debug_bounds("tm-test-fixed-scroll-viewport")
        .expect("the bounded body must remain addressable");
    let marker_before = visual
        .debug_bounds("tm-test-fixed-scroll-content")
        .expect("the body marker must render");
    assert_eq!(header_before.size.width, px(72.0));
    assert_eq!(viewport_before.size.width, px(180.0));
    assert!(header_before.bottom() < viewport_before.top());
    assert!(handle.max_offset().height > px(0.0));

    let offset_before = handle.offset();
    visual.simulate_event(vertical_wheel(viewport_before.center()));
    let offset_after = handle.offset();
    assert!(offset_after.y < offset_before.y);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let header_after = visual
        .debug_bounds("tm-test-fixed-header")
        .expect("fixed dialog chrome must remain mounted");
    let viewport_after = visual
        .debug_bounds("tm-test-fixed-scroll-viewport")
        .expect("the bounded body must remain mounted");
    let marker_after = visual
        .debug_bounds("tm-test-fixed-scroll-content")
        .expect("the body marker must remain measurable");
    assert_eq!(header_after, header_before);
    assert_eq!(viewport_after, viewport_before);
    assert_eq!(
        marker_after.origin.y - marker_before.origin.y,
        offset_after.y - offset_before.y
    );
}
