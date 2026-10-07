//! Execution and scope guards for the canonical local command registry.

use super::*;
use taskmanager_application::system_timeline::SystemPageSection;

pub(super) fn run_direct_arms(
    app: &mut TuiApp,
    arms: &[TuiDirectArm],
    digit: Option<char>,
    modifiers: Modifiers,
) -> InputDispatch {
    for arm in arms {
        if direct_scope_armed(app, arm.scope, modifiers) {
            return execute_tui_local_direct(app, digit, arm.action);
        }
    }
    InputDispatch::Unhandled
}

/// The single guard implementation for every declared [`TuiDirectScope`].
/// Modifier policy per scope mirrors the historical hand-written systems
/// exactly (the overlay toggles ignore chords; page commands refuse
/// Ctrl/Alt; the resource digits also refuse the platform modifier).
fn direct_scope_armed(app: &TuiApp, scope: TuiDirectScope, modifiers: Modifiers) -> bool {
    match scope {
        TuiDirectScope::SystemPage => app.page() == AppPage::System && modifiers == Modifiers::NONE,
        TuiDirectScope::SystemDashboard => {
            app.page() == AppPage::System
                && app.system_section == SystemPageSection::Dashboard
                && modifiers == Modifiers::NONE
        }
        TuiDirectScope::PerformanceHistoryAvailable => {
            app.page() == AppPage::Performance
                && app.history_replay_available()
                && modifiers == Modifiers::NONE
        }
        TuiDirectScope::HistoryReviewAvailable => {
            ((app.page() == AppPage::Performance && app.history_replay_open())
                || app.page() == AppPage::AppHistory)
                && app.history_replay_available()
                && modifiers == Modifiers::NONE
        }
        TuiDirectScope::Anywhere => true,
        TuiDirectScope::ApplicationsPage => {
            app.page() == AppPage::Applications && !modifiers.control && !modifiers.alt
        }
        TuiDirectScope::ApplicationsEscalationReady => {
            direct_scope_armed(app, TuiDirectScope::ApplicationsPage, modifiers)
                && inline_network_escalation_ready(app)
        }
        TuiDirectScope::RowTarget(page) => app.page() == page,
        TuiDirectScope::PerformanceResourceDigit => {
            app.page() == AppPage::Performance
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.platform
        }
        TuiDirectScope::ServicesPageLogClosed => {
            app.page() == AppPage::Services && app.shell.service_log.is_none()
        }
        TuiDirectScope::ServicesPageLogOpen => {
            app.page() == AppPage::Services
                && app.shell.service_log.is_some()
                && !modifiers.control
                && !modifiers.alt
        }
        TuiDirectScope::PerformanceGpuPage => {
            app.page() == AppPage::Performance
                && !modifiers.control
                && !modifiers.alt
                && app.perf_device == PerfDevice::Gpu
        }
        TuiDirectScope::PerformanceDiskPage => {
            app.page() == AppPage::Performance
                && !modifiers.control
                && !modifiers.alt
                && app.perf_device == PerfDevice::Disk
        }
        TuiDirectScope::PerformanceDiskSmartReady => {
            direct_scope_armed(app, TuiDirectScope::PerformanceDiskPage, modifiers)
                && crate::menus::smart_self_test_target(app).is_some()
        }
        TuiDirectScope::ServicesPage => {
            app.page() == AppPage::Services && !modifiers.control && !modifiers.alt
        }
    }
}

/// The single execution site for every declared [`TuiDirectAction`]. Only
/// [`TuiDirectAction::SelectPerfResource`] consumes the pressed digit.
fn execute_tui_local_direct(
    app: &mut TuiApp,
    digit: Option<char>,
    action: TuiDirectAction,
) -> InputDispatch {
    match action {
        TuiDirectAction::SystemDashboard => {
            app.select_system_section(SystemPageSection::Dashboard);
            InputDispatch::Consumed
        }
        TuiDirectAction::SystemHardware => {
            app.select_system_section(SystemPageSection::Hardware);
            InputDispatch::Consumed
        }
        TuiDirectAction::SelectSystemHistoryWindow => {
            if digit.is_some_and(|digit| app.select_system_history_window_digit(digit)) {
                InputDispatch::Consumed
            } else {
                InputDispatch::Unhandled
            }
        }
        TuiDirectAction::ToggleHistoryReplay => {
            app.toggle_history_replay();
            InputDispatch::Consumed
        }
        TuiDirectAction::RefreshHistoryReplay => {
            app.refresh_history_replay();
            InputDispatch::Consumed
        }
        TuiDirectAction::ToggleSettings => {
            app.toggle_settings();
            InputDispatch::Consumed
        }
        TuiDirectAction::ToggleAbout => {
            app.toggle_about();
            InputDispatch::Consumed
        }
        TuiDirectAction::ToggleHealth => {
            app.toggle_health();
            InputDispatch::Consumed
        }
        TuiDirectAction::ToggleContainers => {
            app.toggle_containers();
            InputDispatch::Consumed
        }
        TuiDirectAction::ExportSnapshot => {
            app.export_snapshot();
            InputDispatch::Consumed
        }
        TuiDirectAction::ExportDiagnosticReport => {
            app.open_diagnostic_bundle();
            InputDispatch::Consumed
        }
        TuiDirectAction::CaptureWindow => {
            app.request_current_window_capture();
            InputDispatch::Consumed
        }
        TuiDirectAction::SelectPerfResource => {
            let Some(digit) = digit else {
                return InputDispatch::Unhandled;
            };
            let Some(device) = app.select_perf_device_digit(digit) else {
                return InputDispatch::Unhandled;
            };
            app.select_perf_device(device);
            InputDispatch::Consumed
        }
        TuiDirectAction::OpenServiceMenu => {
            let _ = app.open_service_menu();
            InputDispatch::Consumed
        }
        TuiDirectAction::OpenSessionMenu => {
            let _ = app.open_session_menu();
            InputDispatch::Consumed
        }
        TuiDirectAction::OpenStartupMenu => {
            let _ = app.open_startup_menu();
            InputDispatch::Consumed
        }
        TuiDirectAction::OpenProcessProperties => {
            let _ = app.open_process_properties();
            InputDispatch::Consumed
        }
        TuiDirectAction::ToggleColumnMenu => {
            app.toggle_column_menu();
            InputDispatch::Consumed
        }
        TuiDirectAction::ToggleMarkedProcess => {
            toggle_marked_process(app);
            InputDispatch::Consumed
        }
        TuiDirectAction::ToggleBatchMenu => {
            let _ = app.open_batch_menu();
            InputDispatch::Consumed
        }
        TuiDirectAction::CopyClipboard => {
            // Defensive restatement of the historical inline guard: while a
            // search owns the input scope its characters never reach this
            // system, so this only fails closed.
            if app.search_active() {
                return InputDispatch::Unhandled;
            }
            app.copy_selected_process(&mut std::io::stdout());
            InputDispatch::Consumed
        }
        TuiDirectAction::OpenProcessMenu => {
            let _ = app.open_process_menu();
            InputDispatch::Consumed
        }
        TuiDirectAction::OpenServiceLog => InputDispatch::consumed(app.shell.open_service_log()),
        TuiDirectAction::ExportServiceLog => {
            app.export_service_log();
            InputDispatch::Consumed
        }
        TuiDirectAction::RequestNetworkEscalation => {
            InputDispatch::Effect(Box::new(ShellApp::request_process_network_escalation()))
        }
        TuiDirectAction::ToggleGpuEngineRows => {
            InputDispatch::consumed(app.toggle_gpu_engine_rows())
        }
        TuiDirectAction::CycleGpuChartMetric => {
            app.cycle_gpu_chart_metric();
            InputDispatch::Consumed
        }
        TuiDirectAction::ToggleDirectoryScan => {
            InputDispatch::consumed(app.toggle_directory_scan())
        }
        TuiDirectAction::RequestSmartSelfTest => {
            // The scope guard already proved a SMART-capable target exists;
            // the arm re-resolves and freezes it into the shared gate. No
            // effect returns here — the platform request is emitted only by
            // the gate's `y`, like every shared confirmation.
            let _ = app.arm_smart_self_test();
            InputDispatch::Consumed
        }
        TuiDirectAction::BrowseServiceDependencies => {
            let _ = app.open_service_dependencies();
            InputDispatch::Consumed
        }
    }
}
