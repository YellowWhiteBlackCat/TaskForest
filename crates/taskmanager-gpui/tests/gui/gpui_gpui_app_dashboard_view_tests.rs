use super::timeline_graph_options;
use super::{SummaryDestination, TopPage};
use crate::gpui_app::dashboard::DashboardPanel;
use crate::gpui_app::graph::sample_x;
use crate::gpui_app::sidebar::SelectedDevice;
use gpui::px;

#[test]
fn dashboard_timeline_uses_the_complete_window_for_paint_and_hover() {
    for sample_count in [2, 61, 241] {
        let opts = timeline_graph_options(sample_count, 100.0);
        let left = px(17.0);
        let width = px(580.0);
        assert_eq!(
            sample_x(left, width, 0, sample_count, opts.data_points),
            left
        );
        assert_eq!(
            sample_x(
                left,
                width,
                sample_count - 1,
                sample_count,
                opts.data_points
            ),
            left + width,
        );
    }
}

#[test]
fn summary_destinations_map_to_expected_pages_and_targets() {
    let cpu = SummaryDestination::Cpu.navigation();
    assert_eq!(cpu.page, TopPage::Performance);
    assert_eq!(cpu.device, Some(SelectedDevice::Cpu));
    let memory = SummaryDestination::Memory.navigation();
    assert_eq!(memory.page, TopPage::Performance);
    assert_eq!(memory.device, Some(SelectedDevice::Memory));
    assert_eq!(
        SummaryDestination::Processes.navigation().page,
        TopPage::Apps
    );
    let events = SummaryDestination::Events.navigation();
    assert_eq!(events.page, TopPage::System);
    assert_eq!(events.panel, Some(DashboardPanel::Events));
}
