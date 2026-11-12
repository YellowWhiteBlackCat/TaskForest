use super::*;
use taskmanager_theme::tokens;
use taskmanager_theme::tokens::{RowDensity, UiSize};
use taskmanager_ui_contract::presentation::card_flow::{CardFlowBudget, admit_card_groups_capped};
use taskmanager_ui_contract::presentation::card_width::{CardGridWidthSpec, admit_card_grid_width};
use taskmanager_ui_contract::presentation::dashboard::DASHBOARD_METRIC_COUNT;
use taskmanager_ui_contract::presentation::vocabulary::{
    LayoutProfile, NavigationPresentation, VerticalSpace,
};

/// The budget at the product's default theme axis points.
///
/// The chrome rows are measured from the shared control ladder, so a test
/// that states a frame must also state the axis the ladder resolves at.
fn budget(layout: PageLayoutBudget) -> DashboardBudget {
    DashboardBudget::from_page_layout(layout, UiSize::default(), RowDensity::default())
}

/// The chrome the page paints above its metric slot, measured from the
/// shared chrome-row contract.
///
/// The budget reserves exactly this: the contract owns the row heights, the
/// page paints them, and no page literal stands between the two.
fn chrome_of(layout: PageLayoutBudget) -> f32 {
    let rows =
        DashboardChromeRows::from_page_layout(layout, UiSize::default(), RowDensity::default());
    rows.total() + f32::from(tokens::SPACE_6) + ROOT_ROW_GAP * 3.0 + BOTTOM_SAFETY_BAND
}

fn layout(width: f32, height: f32) -> PageLayoutBudget {
    PageLayoutBudget {
        profile: LayoutProfile::Standard,
        vertical_space: VerticalSpace::Standard,
        page_padding: 16.0,
        navigation: NavigationPresentation::Labeled,
        content_width: width,
        content_height: height,
        chrome: None,
    }
}

fn constrained_layout(width: f32, height: f32) -> PageLayoutBudget {
    let mut layout = layout(width, height);
    layout.vertical_space = VerticalSpace::Constrained;
    layout
}

/// The shared verdict the page must spend for the slot it measured.
fn expected_flow(available_height: f32, columns: usize) -> CardFlowBudget {
    admit_card_groups_capped(
        available_height,
        columns,
        card_flow_spec(),
        max_metric_groups(columns),
    )
}

/// The page's own numbers ARE the shared verdict for the slot it measured.
///
/// This is the "one arithmetic" assertion: the GPUI page must not answer a
/// different card count or a different card height than the shared contract.
fn assert_spends_shared_flow(budget: DashboardBudget) {
    let expected = expected_flow(budget.review_height, budget.columns);
    assert_eq!(budget.columns, expected.columns);
    assert_eq!(budget.metric_count, expected.cards());
    assert_eq!(budget.card_height, expected.card_height);
}

/// Every admitted card keeps the shared card footprint's readable floor and
/// never exceeds its ceiling.
fn assert_cards_stay_inside_the_shared_footprint(budget: DashboardBudget) {
    if budget.metric_count == 0 {
        assert_eq!(budget.card_height, 0.0);
        return;
    }
    assert!(
        budget.card_height >= CARD_FLOOR,
        "a card may never shrink below its floor: {budget:?}"
    );
    assert!(
        budget.card_height <= CARD_CEILING,
        "a card may never grow past its ceiling: {budget:?}"
    );
    assert_eq!(budget.metric_count % budget.columns, 0);
    assert!(budget.metric_count <= DASHBOARD_METRIC_COUNT);
}

#[test]
fn the_page_spends_the_shared_card_flow_verdict() {
    let width = 1180.0;
    for height in [
        0.0, 120.0, 260.0, 300.0, 358.0, 359.0, 420.0, 500.0, 620.0, 780.0, 1000.0,
    ] {
        let budget = budget(constrained_layout(width, height));
        assert_spends_shared_flow(budget);
        assert_cards_stay_inside_the_shared_footprint(budget);
    }
}

#[test]
fn a_group_is_admitted_only_when_the_whole_group_fits() {
    let width = 1180.0;
    // Find the height at which the first complete group becomes admissible
    // by probing the page itself, then prove that one pixel less admits no
    // group at all and one pixel more keeps the group complete.
    let mut admitted_at = None;
    for height in 200..=520 {
        let budget = budget(constrained_layout(width, height as f32));
        if budget.metric_count > 0 {
            admitted_at = Some(height as f32);
            break;
        }
    }
    let admitted_at =
        admitted_at.expect("a dashboard wide enough for one group must admit it eventually");

    let below = budget(constrained_layout(width, admitted_at - 1.0));
    assert_eq!(
        below.metric_count, 0,
        "one pixel below the admission must not paint half a group"
    );
    assert_eq!(below.card_height, 0.0);

    let at = budget(constrained_layout(width, admitted_at));
    assert!(at.metric_count > 0);
    assert_cards_stay_inside_the_shared_footprint(at);
    assert!(at.card_height >= CARD_FLOOR);
}

#[test]
fn the_summary_row_is_an_optional_whole_group() {
    let width = 1180.0;
    // The summary row is optional: it never survives at the cost of the
    // mandatory history cards. Probe the ladder and assert that every step
    // keeps the mandatory group complete.
    let mut cards_at = None;
    let mut counts_at = None;
    let mut none_at = None;
    for height in 200..=700 {
        let budget = budget(constrained_layout(width, height as f32));
        match budget.summaries {
            DashboardSummaries::Cards if cards_at.is_none() => cards_at = Some(height as f32),
            DashboardSummaries::Counts if counts_at.is_none() => counts_at = Some(height as f32),
            DashboardSummaries::None if none_at.is_none() => none_at = Some(height as f32),
            _ => {}
        }
        if budget.summaries != DashboardSummaries::None {
            assert!(
                budget.review_height >= CARD_FLOOR,
                "a visible summary must never starve the mandatory cards: {budget:?}"
            );
        }
    }

    // The ladder is monotone in height: the fuller summary only appears
    // once there is room for it AND for a complete mandatory group.
    let cards_at = cards_at.expect("the wide dashboard must reach the card summary");
    let counts_at = counts_at.expect("the wide dashboard must reach the count summary");
    let none_at = none_at.expect("the wide dashboard must reach the bare layout");
    assert!(none_at < counts_at);
    assert!(counts_at < cards_at);

    let at_counts = budget(constrained_layout(width, counts_at));
    assert_eq!(at_counts.summaries, DashboardSummaries::Counts);
    assert!(at_counts.metric_count > 0);

    let at_cards = budget(constrained_layout(width, cards_at));
    assert_eq!(at_cards.summaries, DashboardSummaries::Cards);
    assert!(at_cards.metric_count > 0);
    assert!(at_cards.review_height >= CARD_FLOOR);

    // One pixel below the card summary, the page degrades to the count row
    // rather than dropping to a half summary.
    let just_below = budget(constrained_layout(width, cards_at - 1.0));
    assert_ne!(just_below.summaries, DashboardSummaries::Cards);
    assert!(just_below.metric_count > 0);
}

#[test]
fn the_bottom_safety_band_is_charged_before_the_cards() {
    let width = 1180.0;
    // Two frames that differ only by the safety band must differ by exactly
    // that band in the slot the cards may spend — the band is a budget fact,
    // never a rendering accident.
    let base = 600.0;
    let taller = budget(constrained_layout(width, base));
    let shorter = budget(constrained_layout(width, base - BOTTOM_SAFETY_BAND));
    assert_eq!(
        taller.review_height - shorter.review_height,
        BOTTOM_SAFETY_BAND,
        "the safety band is charged before the cards receive height"
    );
    assert_spends_shared_flow(taller);
    assert_spends_shared_flow(shorter);
}

#[test]
fn a_narrow_page_keeps_one_column_and_still_reaches_every_metric() {
    let width = 520.0;
    for height in [400.0, 600.0, 900.0, 1400.0] {
        let budget = budget(constrained_layout(width, height));
        assert_eq!(budget.columns, 1);
        assert_spends_shared_flow(budget);
        assert_cards_stay_inside_the_shared_footprint(budget);
        // Paging must still reach every metric, whatever the page admitted.
        let window = budget.metric_window(budget.metric_count.max(1));
        assert!(window.first + budget.metric_count.max(1) <= DASHBOARD_METRIC_COUNT);
    }

    let tall = budget(constrained_layout(width, 1400.0));
    assert_eq!(tall.metric_count, 4);
    assert_eq!(tall.metric_window(3).first, 0);
}

/// A tall reference frame must spend the measured review slot; a card
/// footprint ceiling that stops below the slot leaves a visible dead band
/// under the last row. The shared admission arithmetic is NOT the cause: the
/// shape's own card footprint is.
#[test]
fn a_tall_reference_frame_spends_its_measured_review_slot() {
    let budget = budget(constrained_layout(1180.0, 780.0));
    assert_eq!(budget.columns, 2);
    assert_eq!(budget.metric_count, DASHBOARD_METRIC_COUNT);
    assert_cards_stay_inside_the_shared_footprint(budget);
    assert_spends_shared_flow(budget);
    let groups = budget.metric_count / budget.columns;
    let footprint = budget.card_height * groups as f32 + CARD_GAP * (groups - 1) as f32;
    assert!(
        footprint >= budget.review_height - 1.0,
        "a tall frame must not leave a dead band under the last card row: {budget:?}"
    );
}

/// The compact frame's fixed chrome is measured, not guessed: its window
/// controls, coverage readout and paging are one wrapped row, so the budget
/// must not charge three separate full-height rows for them.
#[test]
fn a_compact_frame_charges_the_wrapped_control_row_once() {
    use crate::gpui_app::root::responsive::{FrameBudget, FrameChromeBudget};
    use gpui::{px, size};
    use taskmanager_ui_contract::navigation::NavOrientation;

    let (width, height) = (720.0, 480.0);
    let frame = FrameBudget::for_root(
        size(px(width), px(height)),
        NavOrientation::Vertical,
        FrameChromeBudget::new(32.0, false, false),
    );
    let budget = DashboardBudget::from_frame(frame, UiSize::default(), RowDensity::default());
    assert_eq!(budget.columns, 2);
    assert!(
        budget.metric_count > 0,
        "a complete group still fits: {budget:?}"
    );
    assert_cards_stay_inside_the_shared_footprint(budget);
    assert_spends_shared_flow(budget);
    let groups = budget.metric_count / budget.columns;
    let footprint = budget.card_height * groups as f32 + CARD_GAP * (groups - 1) as f32;
    assert!(
        footprint <= budget.review_height,
        "the cards must fit the measured slot: {budget:?}"
    );
}

/// The metric cards share the measured row: the column count and the gaps
/// between the cards consume the whole content width, so the rendered row
/// leaves no dead band at its right edge.
#[test]
fn the_metric_cards_share_the_measured_row() {
    for width in [
        320.0, 519.0, 520.0, 599.0, 600.0, 900.0, 1148.0, 1180.0, 1600.0,
    ] {
        let budget = budget(constrained_layout(width, 780.0));
        let expected = (width - CARD_GAP * (budget.columns - 1) as f32) / budget.columns as f32;
        assert_eq!(
            budget.card_width, expected,
            "the card width is the row's share: {budget:?}"
        );
        // The share is the SHARED plain-grid admission, not a page-local
        // fraction: the dashboard metric row and the GPU insight row spend
        // one arithmetic.
        let admitted = admit_card_grid_width(
            width,
            budget.columns,
            CardGridWidthSpec::new(CARD_GAP, GRID_COLUMN_CAP),
        );
        assert_eq!(
            budget.card_width, admitted.card_width,
            "the metric row spends the shared plain-grid admission: {budget:?}"
        );
        let consumed =
            budget.card_width * budget.columns as f32 + CARD_GAP * (budget.columns - 1) as f32;
        assert!(
            (consumed - width).abs() < 0.001,
            "the cards and their gaps consume the measured row: {budget:?}"
        );
        // The grid's item basis is rounded down so a fractional width can
        // never push the second card onto a row of its own, and it never
        // exceeds the exact share the budget reserved.
        assert!(budget.card_basis() <= budget.card_width);
        assert!(budget.card_basis() >= budget.card_width - 1.0);
    }
}

/// The chrome rows the budget reserves are the rows the shared chrome-row
/// contract measures: no page literal stands between the reservation and
/// the row the page paints.
#[test]
fn the_reserved_chrome_is_the_shared_chrome_row_contract() {
    for width in [320.0, 600.0, 900.0, 1048.0, 1180.0, 1600.0] {
        let layout = constrained_layout(width, 780.0);
        let budget = budget(layout);
        let rows =
            DashboardChromeRows::from_page_layout(layout, UiSize::default(), RowDensity::default());
        assert_eq!(budget.chrome_rows, rows);
        let summary_height = match budget.summaries {
            DashboardSummaries::Cards => SUMMARY_CARD_HEIGHT,
            DashboardSummaries::Counts => SUMMARY_COUNT_HEIGHT,
            DashboardSummaries::None => 0.0,
        };
        let row_gaps = match budget.summaries {
            DashboardSummaries::None => ROOT_ROW_GAP * 2.0,
            _ => ROOT_ROW_GAP * 3.0,
        };
        assert_eq!(
            budget.review_height,
            780.0
                - rows.total()
                - f32::from(tokens::SPACE_6)
                - row_gaps
                - BOTTOM_SAFETY_BAND
                - summary_height,
            "the reserved chrome is the contract's rows: {budget:?}"
        );
    }
}

/// The theme axis reaches the chrome rows: a larger UI size spends a taller
/// control ladder, so the chrome reservation grows with it.
#[test]
fn the_chrome_rows_follow_the_theme_axis() {
    let layout = constrained_layout(1180.0, 780.0);
    let standard =
        DashboardBudget::from_page_layout(layout, UiSize::Standard, RowDensity::default());
    let large = DashboardBudget::from_page_layout(layout, UiSize::Large, RowDensity::default());
    assert!(
        large.review_height < standard.review_height,
        "a taller control ladder reserves a taller chrome: {standard:?} / {large:?}"
    );
    assert_eq!(
        large.chrome_rows.total() - standard.chrome_rows.total(),
        3.0 * (UiSize::Large.control_height().0 - UiSize::Standard.control_height().0),
        "every chrome row spends the ladder extent: {standard:?} / {large:?}"
    );
}

/// The coverage readout renders INSIDE the window-controls row. A budget
/// that charged it as a row of its own reserved space the page never paints,
/// and the mandatory metric cards stopped short of the slot they may spend.
#[test]
fn the_coverage_readout_is_not_charged_as_its_own_row() {
    let width = 1180.0;
    let height = 780.0;
    let layout = constrained_layout(width, height);
    let budget = budget(layout);
    assert_eq!(budget.summaries, DashboardSummaries::Cards);
    let chrome = chrome_of(layout);
    assert_eq!(
        budget.review_height,
        height - chrome - SUMMARY_CARD_HEIGHT,
        "the review slot is the measured height minus the chrome the page paints: {budget:?}"
    );
}

/// A visible summary row adds one row to the page and therefore one more
/// gap between the page's rows. The summary row must appear exactly when a
/// complete mandatory card group still fits behind it — never earlier.
#[test]
fn the_summary_row_appears_exactly_when_a_whole_group_still_fits() {
    let width = 1180.0;
    let chrome = chrome_of(constrained_layout(width, 780.0));
    let cards_at = chrome + SUMMARY_CARD_HEIGHT + CARD_FLOOR;

    let at = budget(constrained_layout(width, cards_at));
    assert_eq!(at.summaries, DashboardSummaries::Cards);
    assert!(
        at.metric_count > 0,
        "the summary row never starves the mandatory cards: {at:?}"
    );
    assert!(at.review_height >= CARD_FLOOR);

    let just_below = budget(constrained_layout(width, cards_at - 1.0));
    assert_ne!(
        just_below.summaries,
        DashboardSummaries::Cards,
        "one pixel below the whole-group admission must not paint the card summary"
    );
    assert!(just_below.metric_count > 0);
}
