//! Capture readiness and normal settings input preparation.

use crate::{TuiApp, TuiSurface};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use taskmanager_application::AppPage;
use taskmanager_core::core::metrics::SmartAvailability;
use taskmanager_shell::fixture::process_insights::process_properties_capture_data_ready;
use taskmanager_shell::fixture::process_tree::seed_shell_process_tree;
use taskmanager_ui_contract::navigation::NavOrientation;

/// A scene is ready only when its requested product state exists.
#[must_use]
pub(crate) fn scene_capture_ready(app: &TuiApp) -> bool {
    let Ok(scene) = std::env::var("TM_TUI_CAPTURE_SCENE") else {
        return true;
    };
    if matches!(scene.as_str(), "keyboard-help" | "keyboard-help-end") {
        app.help_open() && (scene != "keyboard-help-end" || app.help_scroll > 0)
    } else if matches!(
        scene.as_str(),
        "process-properties-performance"
            | "process-memory-pss-swap"
            | "process-network-details"
            | "process-gpu-details"
            | "process-resource-limits"
            | "process-isolation"
    ) {
        app.process_properties().is_some()
            && process_properties_capture_data_ready(&app.shell, &scene)
    } else if scene == "active-alert" {
        app.health_open()
            && app.health_review.mode == crate::health_review::HealthReviewMode::ActiveAlerts
            && !app.shell.projection().alert_active.is_empty()
    } else if matches!(
        scene.as_str(),
        "battery-fan-performance" | "battery-live-performance"
    ) {
        app.projection()
            .power_supplies
            .as_ref()
            .is_some_and(|power| {
                power.batteries.iter().any(|battery| {
                    battery.current_capacity_pct() == Some(78)
                        && app
                            .shell
                            .history
                            .battery_capacity_pct_for(&battery.id)
                            .len()
                            >= 8
                        && app.shell.history.battery_power_w_for(&battery.id).len() >= 8
                })
            })
    } else if matches!(scene.as_str(), "smart-missing-tool" | "smart-permission") {
        let expected = if scene == "smart-missing-tool" {
            SmartAvailability::MissingTool
        } else {
            SmartAvailability::PermissionDenied
        };
        app.projection().snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .disks
                .iter()
                .any(|disk| disk.smart_availability == expected)
        })
    } else if matches!(scene.as_str(), "storage-health" | "sensor-center") {
        let expected = if scene == "storage-health" {
            crate::health_review::HealthReviewMode::Storage
        } else {
            crate::health_review::HealthReviewMode::Sensors
        };
        app.health_open()
            && app.health_review.mode == expected
            && app
                .projection()
                .storage_health_projection()
                .is_some_and(|(snapshot, _)| snapshot.filesystems.len() == 3)
            && app
                .projection()
                .sensors
                .as_ref()
                .is_some_and(|snapshot| snapshot.readings.len() == 4)
    } else if scene == "event-center" {
        app.health_open()
            && app.health_review.mode == crate::health_review::HealthReviewMode::Events
            && !app.projection().alert_center.event_history().is_empty()
    } else if scene == "alert-rules-manager" {
        app.health_open()
            && !app
                .shell
                .projection()
                .alert_center
                .managed_rules()
                .is_empty()
    } else if scene == "startup-failure-evidence" {
        app.shell
            .projection()
            .startup_boot_evidence
            .as_ref()
            .is_some_and(|evidence| {
                evidence.failed_units.len() >= 3 && evidence.critical_chain.len() >= 2
            })
    } else if scene == "settings-zero-gray" {
        app.settings_open()
            && app.settings_form.field == 24
            && app.settings_form.gray_zero
            && app.prefs.gray_zero
    } else if scene == "settings-switch-focus" {
        app.settings_open() && app.settings_form.field == 2
    } else if scene == "device-hotplug" {
        app.projection().snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.disks.iter().any(|disk| {
                disk.device_id == "disk:hotplug:usb0"
                    && disk.device_generation.get() == 2
                    && disk.media_removable() == Some(true)
            })
        })
    } else if scene == "apps-group-expanded" {
        let rows = app.process_rows_snapshot();
        rows.iter()
            .filter(|row| matches!(row, crate::process_view::ProcessRow::TreeNode { .. }))
            .count()
            >= 7
            && rows.iter().all(|row| {
                !matches!(
                    row,
                    crate::process_view::ProcessRow::Group {
                        expanded: false,
                        ..
                    } | crate::process_view::ProcessRow::TreeNode {
                        has_children: true,
                        collapsed: true,
                        ..
                    }
                )
            })
    } else if scene == "keyboard-focus" {
        app.focus_panel == crate::FocusPanel::Details
            && app.shell.selected_process_identity().is_some()
    } else if scene == "vertical-nav" {
        app.nav_orientation == NavOrientation::Vertical && app.page() == AppPage::Applications
    } else if scene == "saved-view-presets" {
        app.local_surface_kind() == Some(crate::TuiSurfaceKind::SavedViews)
            && app
                .saved_views
                .rows
                .iter()
                .any(|preset| preset.is_user_saved())
    } else if scene == "sidebar-edit" {
        app.local_surface_kind() == Some(crate::TuiSurfaceKind::SidebarEditor)
            && app
                .sidebar_entries()
                .iter()
                .any(|entry| entry.key == "memory")
    } else {
        true
    }
}

pub(super) fn prepare_capture_settings(app: &mut TuiApp, field: usize, change: bool) {
    app.open_local_surface(TuiSurface::Settings);
    for _ in 0..field {
        let _ = crate::runtime::handle_settings_key(
            app,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        );
    }
    if change {
        let _ = crate::runtime::handle_settings_key(
            app,
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
        );
        let _ = crate::runtime::handle_settings_key(
            app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        prepare_capture_settings(app, field, false);
    }
}

pub(super) fn prepare_expanded_tree(app: &mut TuiApp) {
    app.shell.application.active_page = AppPage::Applications;
    let _ = seed_shell_process_tree(&mut app.shell);
    app.shell.query = "capture-".into();
    let _ = crate::runtime::handle_key(app, KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
    for _ in 0..3 {
        let _ = crate::runtime::handle_key(app, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
}

pub(super) fn prepare_help(app: &mut TuiApp, end: bool) {
    let _ = crate::runtime::handle_key(app, KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    if end {
        for _ in 0..10 {
            let _ = crate::runtime::handle_key(
                app,
                KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            );
        }
    }
}
