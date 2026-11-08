//! test-intent: behavior
//! The shared graph readout: one statistic list, one canonical order, one
//! catalog word per statistic and the one join every frontend renders.

use crate::presentation::{
    graph_summary, graph_summary_item_label, graph_summary_items, graph_summary_text,
    graph_summary_value,
};
use taskmanager_test_support::pin_english;

fn summary() -> crate::presentation::GraphSummary {
    graph_summary(&[30.0, f32::NAN, 32.0, 40.0]).expect("the window holds finite samples")
}

#[test]
fn the_readout_names_three_statistics_in_the_canonical_order() {
    let items: Vec<crate::presentation::GraphSummaryItem> =
        crate::presentation::GraphSummaryItem::ALL.to_vec();
    assert_eq!(
        items,
        [
            crate::presentation::GraphSummaryItem::Latest,
            crate::presentation::GraphSummaryItem::Average,
            crate::presentation::GraphSummaryItem::Maximum,
        ]
    );
}

#[test]
fn every_statistic_carries_the_catalog_word_the_readout_uses() {
    pin_english();
    assert_eq!(
        graph_summary_item_label(crate::presentation::GraphSummaryItem::Latest),
        "Latest"
    );
    assert_eq!(
        graph_summary_item_label(crate::presentation::GraphSummaryItem::Average),
        "Avg"
    );
    assert_eq!(
        graph_summary_item_label(crate::presentation::GraphSummaryItem::Maximum),
        "Peak"
    );
}

#[test]
fn the_canonical_readout_binds_each_statistic_to_its_own_value() {
    let summary = summary();
    let readout = graph_summary_items(&summary);
    assert_eq!(
        readout[0],
        (crate::presentation::GraphSummaryItem::Latest, 40.0)
    );
    assert_eq!(
        readout[1],
        (crate::presentation::GraphSummaryItem::Average, 34.0)
    );
    assert_eq!(
        readout[2],
        (crate::presentation::GraphSummaryItem::Maximum, 40.0)
    );

    for (item, value) in readout {
        assert_eq!(graph_summary_value(&summary, item), value, "{item:?}");
    }
}

#[test]
fn the_text_fold_renders_the_label_and_the_shared_middot() {
    pin_english();
    let summary = summary();
    assert_eq!(
        graph_summary_text(&summary, Some("CPU"), " · ", |value| format!("{value:.0}")),
        "CPU · Latest 40 · Avg 34 · Peak 40"
    );
    assert_eq!(
        graph_summary_text(&summary, None, " · ", |value| format!("{value:.0}")),
        "Latest 40 · Avg 34 · Peak 40"
    );
    // The terminal repertoire joins with its own separator, without padding.
    assert_eq!(
        graph_summary_text(&summary, Some("CPU"), "·", |value| format!("{value:.0}")),
        "CPU·Latest 40·Avg 34·Peak 40"
    );
}

#[test]
fn an_all_gap_window_still_has_no_readout() {
    assert!(graph_summary(&[f32::NAN, f32::NAN]).is_none());
}
