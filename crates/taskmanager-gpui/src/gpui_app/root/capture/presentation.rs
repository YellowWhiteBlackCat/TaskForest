//! A strict capture waits for its actual controlled data and two rendered frames.
use super::CaptureScenario;
use super::state::SurfacePresentation;
use crate::gpui_app::root::{RootView, TopPage, WindowSurfaceKind};
use crate::gpui_app::sidebar::SelectedDevice;
use gpui::{Context, Window};
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_shell::presentation::effective_smart_status;

fn ready(view: &RootView) -> bool {
    match view.capture_evidence.scenario {
        Some(CaptureScenario::ActiveAlert) => !view.active_alerts().is_empty(),
        Some(CaptureScenario::SmartMissingTool | CaptureScenario::SmartPermission) => {
            let expected =
                if view.capture_evidence.scenario == Some(CaptureScenario::SmartMissingTool) {
                    DeviceStatus::MissingTool
                } else {
                    DeviceStatus::PermissionDenied
                };
            view.page == TopPage::Performance
                && view.selected == SelectedDevice::Disk(0)
                && view
                    .system_snapshot()
                    .disks
                    .first()
                    .is_some_and(|disk| effective_smart_status(disk) == expected)
        }
        Some(CaptureScenario::ServiceDetailsLogs) => view
            .service_details_target()
            .is_some_and(|target| view.services().iter().any(|service| &service.id == target)),
        Some(CaptureScenario::BatteryFanPerformance | CaptureScenario::BatteryLivePerformance) => {
            view.selected == SelectedDevice::Battery(0)
                && !view.selected_device_missing
                && view.capture_evidence.dynamic_history_seeded
        }
        Some(CaptureScenario::SettingsSwitchFocus | CaptureScenario::SettingsZeroGray) => {
            view.window_surface_kind() == Some(WindowSurfaceKind::Settings)
        }
        Some(CaptureScenario::SidebarEdit) => view.sidebar_visible && view.sidebar_edit_mode,
        _ => true,
    }
}
pub(crate) fn schedule_controlled_presentation(
    view: &mut RootView,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    if view.capture_evidence.scenario.is_none()
        || !view.capture_evidence.scenario_ready()
        || !view.capture_evidence.ui_data_ready()
        || !ready(view)
        || view.capture_evidence.controlled_presentation != SurfacePresentation::Waiting
    {
        return;
    }
    view.capture_evidence.controlled_presentation = SurfacePresentation::Scheduled;
    cx.on_next_frame(window, |_view, window, cx| {
        cx.notify();
        cx.on_next_frame(window, |view, _window, cx| {
            if ready(view) {
                view.capture_evidence.controlled_presentation = SurfacePresentation::Presented;
                super::marker::emit_marker("surface_presented", view.capture_evidence.scenario);
            } else {
                view.capture_evidence.controlled_presentation = SurfacePresentation::Waiting;
                cx.notify();
            }
        });
    });
}
