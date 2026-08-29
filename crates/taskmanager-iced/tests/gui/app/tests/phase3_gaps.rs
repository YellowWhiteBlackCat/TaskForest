use super::*;
use crate::app::{FocusTarget, Message, ProcessStatusFilter};
use taskmanager_application::AppPage;

#[test]
fn test_saved_views_presets_lifecycle() {
    let mut app = IcedApp::demo();
    app.shell.application.active_page = AppPage::Applications;
    assert!(!app.saved_views.is_empty());

    // Save the canonical category tree with a running-process filter.
    app.shell
        .set_process_status_filter(ProcessStatusFilter::Running);

    // Save current view
    let initial_count = app.saved_views.len();
    let _ = app.update(Message::SaveCurrentProcessView);
    assert_eq!(app.saved_views.len(), initial_count + 1);

    let custom_preset = app.saved_views.last().unwrap();
    assert_eq!(custom_preset.filter, ProcessStatusFilter::Running);
    let custom_id = custom_preset.id;

    app.shell
        .set_process_status_filter(ProcessStatusFilter::All);

    // Apply the saved view.
    let _ = app.update(Message::ApplySavedView(custom_id));
    assert_eq!(
        app.shell.process_status_filter,
        ProcessStatusFilter::Running
    );

    // Export saved views
    let _ = app.update(Message::ExportSavedViews);
    assert_eq!(
        app.saved_view_feedback,
        Some(crate::saved_views::SavedViewTransferFeedback::ExportCopied)
    );

    // Delete the custom preset
    let _ = app.update(Message::DeleteSavedView(custom_id));
    assert_eq!(app.saved_views.len(), initial_count);
}

#[test]
fn test_alert_center_lifecycle() {
    let mut app = IcedApp::demo();
    assert!(!app.alert_center_open());

    let _ = app.update(Message::OpenAlertCenter);
    assert!(app.alert_center_open());

    // Install a deterministic event through the shared alert authority and
    // then clear it through the same application path the overlay uses.
    let alert = taskmanager_core::core::alerts::Alert {
        instance_id: "cpu-high:system".into(),
        rule_id: "cpu-high".into(),
        target: "system".into(),
        metric: taskmanager_core::core::alerts::AlertMetric::CpuUsagePercent,
        severity: taskmanager_core::core::alerts::AlertSeverity::Critical,
        value: 99.5,
        threshold: 90.0,
        active_since_ms: 1000,
    };
    app.shell
        .replace_alert_event_history(vec![taskmanager_core::core::alerts::AlertEvent {
            id: 1,
            kind: taskmanager_core::core::alerts::AlertEventKind::Activated,
            alert,
            observed_at_ms: 1000,
        }]);
    assert_eq!(app.shell.projection().alert_center.event_history().len(), 1);

    let _ = app.update(Message::ClearAlertEvents);
    assert!(
        app.shell
            .projection()
            .alert_center
            .event_history()
            .is_empty()
    );

    let _ = app.update(Message::CloseAlertCenter);
    assert!(!app.alert_center_open());
}

#[test]
fn test_diagnostics_report_generation() {
    let app = IcedApp::demo();
    let report = crate::export::system_diagnostics_markdown(
        app.shell.projection().hardware.as_ref(),
        app.shell.projection().snapshot.as_ref(),
    );
    assert!(report.contains("TaskForest System Diagnostics Report"));
    assert!(report.contains("OS:"));
    assert!(report.contains("Kernel:"));
}

#[test]
fn test_focus_targets_for_all_phase3_features() {
    assert_eq!(
        crate::focus::focus_id(FocusTarget::SavedViewPreset(42)),
        "iced-saved-view-preset-42"
    );
    assert_eq!(
        crate::focus::focus_id(FocusTarget::SavedViewSaveCurrent),
        "iced-saved-view-save-current"
    );
    assert_eq!(
        crate::focus::focus_id(FocusTarget::SavedViewExport),
        "iced-saved-view-export"
    );
    assert_eq!(
        crate::focus::focus_id(FocusTarget::HistoryReplayToggle),
        "iced-history-replay-toggle"
    );
    assert_eq!(
        crate::focus::focus_id(FocusTarget::AlertCenterClear),
        "iced-alert-center-clear"
    );
    assert_eq!(
        crate::focus::focus_id(FocusTarget::ProcessMenuCopyTsv),
        "iced-process-menu-copy-tsv"
    );
    assert_eq!(
        crate::focus::focus_id(FocusTarget::ProcessMenuCopyJson),
        "iced-process-menu-copy-json"
    );
}
