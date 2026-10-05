//! Startup capture state through a declared resource access set.
use super::{DemoMode, ThemePreferences};
use crate::app::FrontendTrack;
use crate::capture::{capture_scenario_target, capture_wants_service_logs};
use crate::demo_fixture::{
    demo_shell, seed_capture_confirmation_fixture, seed_service_log_fixture,
};
use crate::pages::system::dashboard::SystemDashboardState;
use bevy::ecs::system::{Commands, NonSendMut, Res, ResMut};
use taskmanager_application::system_timeline::{SystemHistoryWindow, SystemPageSection};
use taskmanager_shell::fixture::dashboard_history::seed_shell_system_dashboard_history;
use taskmanager_shell::fixture::smbios_memory::seed_shell_memory_inventory;
use taskmanager_shell::presentation::health_review::HealthReviewSection;

pub(super) fn is_production(demo: Option<Res<DemoMode>>) -> bool {
    demo.is_none()
}
pub(super) fn is_demo(demo: Option<Res<DemoMode>>) -> bool {
    demo.is_some()
}
pub(super) fn initialize_capture_state(
    demo: Option<Res<DemoMode>>,
    mut track: NonSendMut<FrontendTrack>,
    mut dashboard: ResMut<SystemDashboardState>,
    mut preferences: ResMut<ThemePreferences>,
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
        dashboard.section = SystemPageSection::Health;
        dashboard.health_section = if capture_scenario_target() == Some("sensor-center") {
            HealthReviewSection::Sensors
        } else {
            HealthReviewSection::Storage
        };
    }
    track.shell = shell;
    track.initial_refresh_submitted = true;
    if let Some(target @ ("system-dashboard" | "history-60m")) = capture_scenario_target() {
        dashboard.section = SystemPageSection::Dashboard;
        dashboard.window = if target == "history-60m" {
            SystemHistoryWindow::SixtyMinutes
        } else {
            SystemHistoryWindow::FifteenMinutes
        };
    }
    if matches!(
        capture_scenario_target(),
        Some("settings-zero-gray" | "apps-zero-gray")
    ) {
        preferences.gray_zero_values = true;
    }
    commands.trigger(crate::input::ShellInteractionApplied);
}
