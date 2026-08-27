use super::memory_stats::optional_memory;
use super::network_stats::network_link_speed_graph_max_mbps;
use super::{finite_graph_summary, gpu_percentage_readout};
use crate::gpui_app::formatting::DisplayUnits;

#[test]
fn gpu_optional_percentages_distinguish_unknown_from_measured_zero() {
    assert_eq!(gpu_percentage_readout(None), "—");
    assert_eq!(gpu_percentage_readout(Some(0.0)), "0%");
    assert_eq!(gpu_percentage_readout(Some(41.6)), "42%");
}

#[test]
fn optional_memory_capacity_distinguishes_unknown_from_measured_zero() {
    assert_eq!(optional_memory(None, DisplayUnits::default()), "—");
    assert_eq!(optional_memory(Some(0), DisplayUnits::default()), "0 KiB");
}

#[test]
fn graph_summary_ignores_provider_gaps_without_reusing_neighbors() {
    let summary = finite_graph_summary(&[20.0, f32::NAN, 0.0, 40.0])
        .expect("finite samples produce a graph summary");
    assert_eq!(summary.latest, 40.0);
    assert_eq!(summary.average, 20.0);
    assert_eq!(summary.minimum, 0.0);
    assert_eq!(summary.maximum, 40.0);
    assert_eq!(summary.sample_count, 3);
    assert_eq!(finite_graph_summary(&[f32::NAN, f32::NAN]), None);
}

#[test]
fn network_fixed_scale_uses_decimal_link_speed_and_fails_closed() {
    assert_eq!(network_link_speed_graph_max_mbps(None), None);
    assert_eq!(network_link_speed_graph_max_mbps(Some(1_000)), Some(125.0));
    assert_eq!(network_link_speed_graph_max_mbps(Some(0)), Some(1.0));
}
