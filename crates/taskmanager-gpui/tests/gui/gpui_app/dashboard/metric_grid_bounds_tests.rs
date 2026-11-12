//! The dashboard metric grid spends the row it measured.
//!
//! The grid's column geometry belongs to the page budget, so the rendered
//! row must fill the measured page slot exactly: the cards share the slot
//! minus the gaps between them, no dead band survives at the right edge, and
//! a second card never lands on a row of its own.

use crate::gpui_app::chrome::titlebar_height;
use crate::gpui_app::root::responsive::{DashboardBudget, FrameBudget, FrameChromeBudget};
use crate::gpui_app::root::{RootView, TopPage};
use gpui::{AppContext, Bounds, TestAppContext, VisualTestContext, px, size};
use taskmanager_assets::embedded_fonts;
use taskmanager_test_support::pin_english;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui_contract::presentation::card_width::CardGridWidthBudget;
use taskmanager_ui_contract::presentation::card_width::{CardGridWidthSpec, admit_card_grid_width};
use taskmanager_ui_contract::presentation::vocabulary::BOTTOM_SAFETY_BAND;

const GRID: &str = "tm-dashboard-metric-grid";
const REVIEW: &str = "tm-dashboard-review";
const SECTION_TABS: &str = "tm-dashboard-section-tabs";
const WINDOW_CONTROLS: &str = "tm-dashboard-window-controls";
const PAGING: &str = "tm-dashboard-paging";
const CARD_CPU: &str = "tm-dashboard-history-card:cpu";
const CARD_MEMORY: &str = "tm-dashboard-history-card:memory";
const CARD_DISK: &str = "tm-dashboard-history-card:disk";
const CARD_NETWORK: &str = "tm-dashboard-history-card:network";

/// The shared spacing token the metric grid puts between two cards.
const CARD_GAP: f32 = tokens::SPACE_8.0;

/// The theme's tab-control extent: the shared control ladder's height for
/// the chrome rows at the default UI size.
fn chrome_line_height() -> f32 {
    use taskmanager_ui::presentation_binding::control_measurement;
    use taskmanager_ui_contract::presentation::sizes::{ControlRole, ControlState};
    control_measurement(
        ControlRole::Tab,
        ControlState::Normal,
        tokens::UiSize::default(),
        tokens::RowDensity::default(),
    )
    .extent
}

/// Render the real dashboard page at one window size.
fn draw_dashboard(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
) -> (VisualTestContext, DashboardBudget) {
    let window = cx.add_window(|_, cx| RootView::new_demo(Theme::dark(), cx));
    cx.simulate_window_resize(window.into(), size(px(width), px(height)));
    let budget = window
        .read_with(cx, |view, _| {
            let appearance = view.presentation_snapshot().appearance;
            DashboardBudget::from_frame(
                FrameBudget::for_root(
                    size(px(width), px(height)),
                    view.nav_orientation,
                    FrameChromeBudget::new(titlebar_height(&view.theme), false, false),
                ),
                appearance.ui_size,
                appearance.density,
            )
        })
        .expect("the demo resolve its frame budget");
    window
        .update(cx, |view, _, cx| {
            view.page = TopPage::System;
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    (VisualTestContext::from_window(window.into(), cx), budget)
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
        let (mut visual, _) = draw_dashboard(cx, width, height);
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
        let (mut visual, _) = draw_dashboard(cx, width, 780.0);
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

/// The budget's chrome rows are the rows the page PAINTS.
///
/// A guessed row height reserves space the page never paints, so the
/// admitted cards stop short of the bottom of their own slot. Every chrome
/// row's painted height must equal the shared contract's reserved height,
/// on both the one-line and the wrapped rung.
#[gpui::test]
async fn the_painted_chrome_rows_equal_the_reserved_rows(cx: &mut TestAppContext) {
    pin_english();
    cx.update(|cx| {
        cx.text_system()
            .add_fonts(embedded_fonts())
            .expect("embedded fonts")
    });
    let line = chrome_line_height();
    let wrapped = line * 2.0 + tokens::SPACE_8.0;
    for (width, height, section, window) in [
        (1180.0, 780.0, line, line),
        (1000.0, 780.0, line, line),
        (900.0, 780.0, wrapped, line),
        (720.0, 480.0, wrapped, line),
        (480.0, 360.0, wrapped, wrapped),
    ] {
        let (mut visual, _) = draw_dashboard(cx, width, height);
        for (selector, reserved) in [
            (SECTION_TABS, section),
            (WINDOW_CONTROLS, window),
            (PAGING, line),
        ] {
            let bounds = visual
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} is mounted at {width}x{height}"));
            assert_eq!(
                f32::from(bounds.size.height),
                reserved,
                "{selector} paints the height the budget reserved at {width}x{height}: {bounds:?}"
            );
        }
    }
}

/// An admitted card GROWS into its flex line.
///
/// The card-flow budget's `card_height` is a FLOOR, not an exact height:
/// `align_content: stretch` distributes the leftover review height into the
/// grid's lines, and a card pinned to its budgeted height leaves dead space
/// below it. The last card must reach the bottom of the review slot minus
/// its own bottom safety band, and every card must be at least as tall as
/// the budgeted floor.
#[gpui::test]
async fn an_admitted_card_fills_its_whole_flex_line(cx: &mut TestAppContext) {
    pin_english();
    cx.update(|cx| {
        cx.text_system()
            .add_fonts(embedded_fonts())
            .expect("embedded fonts")
    });
    for (width, height) in [(1180.0, 780.0), (1280.0, 720.0), (720.0, 480.0)] {
        let (mut visual, budget) = draw_dashboard(cx, width, height);
        let cards = mounted_cards(&mut visual);
        let review = visual.debug_bounds(REVIEW).expect("the review slot");
        let grid = visual.debug_bounds(GRID).expect("the metric grid");
        assert!(
            budget.metric_count > 0,
            "the frame admits cards: {budget:?}"
        );
        assert_eq!(cards.len(), budget.metric_count);
        let floor = budget.card_height;
        let last = cards.last().expect("a mounted card").1;
        assert!(
            f32::from(last.size.height) >= floor - 0.001,
            "a card never shrinks below its budgeted floor: {last:?} floor {floor}"
        );
        // The grid spans the whole review slot minus the safety band; the
        // last line reaches its bottom, so no dead band survives below the
        // last card row.
        assert_eq!(
            f32::from(grid.bottom()),
            f32::from(review.bottom()) - BOTTOM_SAFETY_BAND,
            "the grid reaches its stretched lines' bottom: {grid:?} in {review:?}"
        );
        assert!(
            f32::from(last.bottom()) <= f32::from(grid.bottom()) + 0.001,
            "the last line stays inside the grid: {last:?} in {grid:?}"
        );
        // The card FILLS its line: a card pinned to the budgeted height
        // leaves the line's stretched remainder as a dead band below it.
        let line_count = budget.metric_count.div_ceil(budget.columns);
        let line_height = (f32::from(grid.size.height)
            - CARD_GAP * (line_count.saturating_sub(1)) as f32)
            / line_count as f32;
        assert!(
            f32::from(last.size.height) >= line_height - 1.0,
            "a card grows into its stretched line: {last:?} line {line_height}"
        );
        assert!(
            (f32::from(last.bottom()) - f32::from(grid.bottom())).abs() <= 1.0,
            "the last row reaches the bottom of its line: {last:?} in {grid:?}"
        );
        // Every card is the exact share the shared plain-grid admission
        // computes — no page-local fraction survives.
        let admitted: CardGridWidthBudget = admit_card_grid_width(
            f32::from(review.size.width),
            budget.columns,
            CardGridWidthSpec::new(CARD_GAP, 2),
        );
        assert_eq!(f32::from(last.size.width), admitted.card_width);
    }
}
