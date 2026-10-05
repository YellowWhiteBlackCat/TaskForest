//! Capture readiness and normal settings input preparation.

use crate::{TuiApp, TuiSurface};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use taskmanager_shell::fixture::process_insights::process_properties_capture_data_ready;

/// A scene is ready only when its requested product state exists.
#[must_use]
pub(crate) fn scene_capture_ready(app: &TuiApp) -> bool {
    let Ok(scene) = std::env::var("TM_TUI_CAPTURE_SCENE") else {
        return true;
    };
    if matches!(
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
        !app.shell.projection().alert_active.is_empty()
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
    } else if scene == "sidebar-edit" {
        app.settings_open() && app.settings_form.field == 9
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
