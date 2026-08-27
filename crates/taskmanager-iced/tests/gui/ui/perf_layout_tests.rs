use super::{bounded_heading, geometry_contract};
use crate::ui::device_chart::{DEVICE_CHART_HEIGHT, primary_graph_height};
use iced::Length;

#[test]
fn geometry_contract_has_explicit_desktop_and_compact_values() {
    let desktop = geometry_contract(false);
    let compact = geometry_contract(true);
    assert_eq!(desktop.sidebar_width, 216.0);
    assert_eq!(desktop.stats_width, 246.0);
    assert_eq!(desktop.stats_label_width, 96.0);
    assert_eq!(desktop.title_size, 24.0);
    assert!(!desktop.compact);
    assert_eq!(compact.sidebar_width, 0.0);
    assert_eq!(compact.stats_width, 154.0);
    assert_eq!(compact.stats_label_width, 62.0);
    assert_eq!(compact.title_size, 19.0);
    assert!(compact.compact);
    assert!(compact.stats_width < desktop.stats_width);
}

#[test]
fn heading_projection_is_bounded_for_long_device_identity() {
    assert_eq!(bounded_heading("CPU", 12), "CPU");
    assert_eq!(
        bounded_heading("Intel Core Ultra 7 358H with extra suffix", 18),
        "Intel Core Ultra …"
    );
}

#[test]
fn primary_device_graph_fills_wide_cards_but_keeps_compact_cards_readable() {
    assert_eq!(
        primary_graph_height(false),
        Length::Fill,
        "wide device cards must hand the primary graph the left column's remaining height"
    );
    assert_eq!(
        primary_graph_height(true),
        Length::Fixed(DEVICE_CHART_HEIGHT),
        "compact device cards must retain an intrinsic graph height inside the page scroll"
    );
}
