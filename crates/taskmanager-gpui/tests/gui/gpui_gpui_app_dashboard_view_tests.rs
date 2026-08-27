use super::{SummaryDestination, TopPage};
use crate::gpui_app::dashboard::DashboardPanel;
use crate::gpui_app::sidebar::SelectedDevice;

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
