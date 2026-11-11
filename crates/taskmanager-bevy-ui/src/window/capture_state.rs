//! Startup capture state through a declared resource access set.
use super::{DemoMode, ThemePreferences};
use crate::app::FrontendTrack;
use crate::capture::{
    capture_perf_device_target, capture_scenario_target, capture_wants_service_logs,
};
use crate::demo_fixture::{
    demo_shell, seed_capture_confirmation_fixture, seed_service_log_fixture,
};
use crate::pages::performance::{PerformanceDeviceFocus, PerformanceFocus};
use crate::pages::processes::card_lane::ProcessViewMode;
use crate::pages::system::dashboard::SystemDashboardState;
use crate::window_surface::WindowSurfaceCommand;
use bevy::ecs::system::{Commands, NonSendMut, Res, ResMut, SystemParam};
use taskmanager_application::system_timeline::{SystemHistoryWindow, SystemPageSection};
use taskmanager_shell::fixture::dashboard_history::seed_shell_system_dashboard_history;
use taskmanager_shell::fixture::process_tree::seed_shell_process_tree_with_partial_aggregate;
use taskmanager_shell::fixture::smbios_memory::seed_shell_memory_inventory;
use taskmanager_shell::presentation::health_review::HealthReviewSection;

pub(super) fn is_production(demo: Option<Res<DemoMode>>) -> bool {
    demo.is_none()
}
pub(super) fn is_demo(demo: Option<Res<DemoMode>>) -> bool {
    demo.is_some()
}
/// The mutable presentation resources one capture startup seeds.
#[derive(SystemParam)]
pub(super) struct CapturePresentationState<'w> {
    dashboard: ResMut<'w, SystemDashboardState>,
    preferences: ResMut<'w, ThemePreferences>,
    device_focus: ResMut<'w, PerformanceDeviceFocus>,
    curve_focus: ResMut<'w, PerformanceFocus>,
    process_view_mode: ResMut<'w, ProcessViewMode>,
}

pub(super) fn initialize_capture_state(
    demo: Option<Res<DemoMode>>,
    mut track: NonSendMut<FrontendTrack>,
    mut state: CapturePresentationState,
    mut commands: Commands,
) {
    if demo.is_none() {
        return;
    }
    let mut shell = demo_shell();
    if capture_wants_service_logs() {
        seed_service_log_fixture(&mut shell);
    }
    seed_capture_confirmation_fixture(&mut shell);
    if let Some(target) = capture_perf_device_target(&shell) {
        if let Some(curve) = target.curve() {
            state.curve_focus.0 = curve;
        }
        state.device_focus.0 = target;
    }
    if matches!(
        capture_scenario_target(),
        Some("system-dashboard" | "history-60m")
    ) {
        let _ = seed_shell_system_dashboard_history(&mut shell, 7_200_000);
    }
    if capture_scenario_target() == Some("system-hardware") {
        seed_shell_memory_inventory(&mut shell);
    }
    if matches!(
        capture_scenario_target(),
        Some("storage-health" | "sensor-center")
    ) {
        state.dashboard.section = SystemPageSection::Health;
        state.dashboard.health_section = if capture_scenario_target() == Some("sensor-center") {
            HealthReviewSection::Sensors
        } else {
            HealthReviewSection::Storage
        };
    }
    if capture_scenario_target() == Some("apps-group-expanded") {
        let _ = seed_shell_process_tree_with_partial_aggregate(&mut shell);
        shell.query = "capture-".into();
    }
    // Scenarios that anchor a real table row or its context menu need the
    // DENSE presentation: the default Cards mode owns a different row root,
    // so a capture that waits for `ProcessRowsRoot` would never paint.
    if matches!(
        capture_scenario_target(),
        Some("process-context-menu" | "process-selection" | "process-force-kill")
    ) {
        *state.process_view_mode = ProcessViewMode::Dense;
    }
    track.shell = shell;
    track.initial_refresh_submitted = true;
    if let Some(target @ ("system-dashboard" | "history-60m")) = capture_scenario_target() {
        state.dashboard.section = SystemPageSection::Dashboard;
        state.dashboard.window = if target == "history-60m" {
            SystemHistoryWindow::SixtyMinutes
        } else {
            SystemHistoryWindow::FifteenMinutes
        };
    }
    if matches!(
        capture_scenario_target(),
        Some("settings-zero-gray" | "apps-zero-gray")
    ) {
        state.preferences.gray_zero_values = true;
    }
    commands.trigger(crate::input::ShellInteractionApplied);
    if capture_scenario_target() == Some("keyboard-help") {
        track.shell.toggle_help();
        commands.trigger(crate::input::ShellInteractionApplied);
    }
    if capture_scenario_target() == Some("apps-group-expanded") {
        commands.trigger(WindowSurfaceCommand::ProcessTree);
        commands.trigger(crate::pages::process_tree::TreeExpansionCommand::ExpandAll);
    }
    if capture_scenario_target() == Some("apps-container-group") {
        commands.trigger(WindowSurfaceCommand::ProcessTree);
        commands.trigger(crate::pages::process_tree::TreeExpansionCommand::ExpandAll);
    }
    if capture_scenario_target() == Some("event-center") {
        commands.trigger(crate::event_center::EventCommand(
            crate::event_center::EventAction::Open,
        ));
    }
    if capture_scenario_target() == Some("vertical-nav") {
        commands.trigger(crate::navigation::ToggleNavigation);
    }
    if capture_scenario_target() == Some("saved-view-presets") {
        commands.trigger(crate::saved_views::SavedViewCommand(
            crate::saved_views::SavedViewAction::Open,
        ));
        commands.trigger(crate::saved_views::SavedViewCommand(
            crate::saved_views::SavedViewAction::Save,
        ));
    }
    if capture_scenario_target() == Some("sidebar-edit") {
        commands.trigger(WindowSurfaceCommand::SidebarDevices);
    }
}
