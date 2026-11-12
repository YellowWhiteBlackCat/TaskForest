//! The dashboard metric grid spends the row it measured.
//!
//! The grid's column geometry belongs to the page budget, so the rendered
//! row must fill the measured page slot exactly: the cards share the slot
//! minus the gaps between them, no dead band survives at the right edge, and
//! a second card never lands on a row of its own.

use crate::gpui_app::root::{RootView, TopPage};
use gpui::{AppContext, Bounds, TestAppContext, VisualTestContext, px, size};
use taskmanager_assets::embedded_fonts;
use taskmanager_test_support::pin_english;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

const GRID: &str = "tm-dashboard-metric-grid";
const REVIEW: &str = "tm-dashboard-review";
const CARD_CPU: &str = "tm-dashboard-history-card:cpu";
const CARD_MEMORY: &str = "tm-dashboard-history-card:memory";
const CARD_DISK: &str = "tm-dashboard-history-card:disk";
const CARD_NETWORK: &str = "tm-dashboard-history-card:network";

/// The shared spacing token the metric grid puts between two cards.
const CARD_GAP: f32 = tokens::SPACE_8.0;

/// Render the real dashboard page at one window size.
fn draw_dashboard(cx: &mut TestAppContext, width: f32, height: f32) -> VisualTestContext {
    let window = cx.add_window(|_, cx| RootView::new_demo(Theme::dark(), cx));
    cx.simulate_window_resize(window.into(), size(px(width), px(height)));
    window
        .update(cx, |view, _, cx| {
            view.page = TopPage::System;
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    VisualTestContext::from_window(window.into(), cx)
}

/// The metric cards this frame actually mounted, in product order.
fn mounted_cards(visual: &mut VisualTestContext) -> Vec<(&'static str, Bounds<gpui::Pixels>)> {
    [CARD_CPU, CARD_MEMORY, CARD_DISK, CARD_NETWORK]
        .into_iter()
        .filter_map(|selector| {
            visual
                .debug_bounds(selector)
                .map(|bounds| (selector, bounds))
        })
        .collect()
}

/// The row's cards fill the measured slot: the last card of a row reaches
/// the row's right edge, the cards are equally wide, and the gap between
/// them is the shared spacing token.
#[gpui::test]
async fn the_metric_row_fills_the_measured_page_slot(cx: &mut TestAppContext) {
    pin_english();
    cx.update(|cx| {
        cx.text_system()
            .add_fonts(embedded_fonts())
            .expect("embedded fonts")
    });

    for (width, height) in [(1180.0, 780.0), (720.0, 480.0)] {
        let mut visual = draw_dashboard(cx, width, height);
        let grid = visual
            .debug_bounds(GRID)
            .expect("the metric grid is mounted");
        let review = visual
            .debug_bounds(REVIEW)
            .expect("the review slot is mounted");
        assert_eq!(
            grid.size.width, review.size.width,
            "the grid spans the review slot it was measured in: {grid:?} in {review:?}"
        );

        let cards = mounted_cards(&mut visual);
        assert!(
            cards.len() >= 2,
            "a two-column frame mounts at least one complete row: {width}x{height}"
        );

        let first = cards[0].1;
        let second = cards[1].1;
        assert_eq!(
            first.origin.y, second.origin.y,
            "the first two cards share one row: {first:?} / {second:?}"
        );
        assert_eq!(
            first.size.width, second.size.width,
            "the cards of one row are equally wide: {first:?} / {second:?}"
        );
        assert_eq!(
            second.origin.x - first.right(),
            px(CARD_GAP),
            "the gap between two cards is the shared spacing token: {first:?} / {second:?}"
        );
        assert_eq!(
            second.right(),
            grid.right(),
            "the last card of the row reaches the row's right edge: {second:?} in {grid:?}"
        );

        // The next row, when the frame admits one, aligns under the first.
        if let Some((_, third)) = cards.get(2) {
            assert_eq!(
                third.origin.x, first.origin.x,
                "the second row starts under the first column: {third:?} / {first:?}"
            );
            assert!(
                third.origin.y >= first.bottom(),
                "the second row starts below the first: {third:?} / {first:?}"
            );
            if let Some((_, fourth)) = cards.get(3) {
                assert_eq!(fourth.origin.x, second.origin.x);
                assert_eq!(
                    fourth.right(),
                    grid.right(),
                    "the last row also reaches the row's right edge: {fourth:?} in {grid:?}"
                );
            }
        }

        // The admitted groups stay inside the review slot: the bottom safety
        // band is a budget fact, never a rendering accident.
        let last = cards.last().expect("a mounted card").1;
        assert!(
            last.bottom() <= review.bottom(),
            "the last metric row stays inside the review slot: {last:?} in {review:?}"
        );
    }
}

/// A fractional window width must not push the second card onto a row of
/// its own: the grid's item basis is rounded down, so the row always holds
/// the budget's column count.
#[gpui::test]
async fn a_fractional_width_keeps_both_cards_on_one_row(cx: &mut TestAppContext) {
    pin_english();
    cx.update(|cx| {
        cx.text_system()
            .add_fonts(embedded_fonts())
            .expect("embedded fonts")
    });

    for width in [1180.5, 1181.5, 1179.5] {
        let mut visual = draw_dashboard(cx, width, 780.0);
        let grid = visual
            .debug_bounds(GRID)
            .expect("the metric grid is mounted");
        let cpu = visual.debug_bounds(CARD_CPU).expect("the cpu card");
        let memory = visual.debug_bounds(CARD_MEMORY).expect("the memory card");
        assert_eq!(
            cpu.origin.y, memory.origin.y,
            "a fractional width must not break the row: {width}"
        );
        assert_eq!(
            memory.right(),
            grid.right(),
            "the row still fills the measured slot: {width}"
        );
    }
}
