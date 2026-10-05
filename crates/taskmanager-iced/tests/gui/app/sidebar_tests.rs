//! test-intent: behavior
//! Native messages preserve concrete identity, category precedence and persisted order.

use super::*;
use crate::app::{InputScope, Message, SettingsChange};
use taskmanager_application::ConfigStore;
use taskmanager_core::core::config::Config;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

#[test]
fn sidebar_editor_edits_devices_and_rejects_controls_after_removal_or_close() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::OpenSidebarEditor);
    assert_eq!(
        app.input_scope(),
        InputScope::LocalSurface(LocalSurfaceKind::SidebarEditor)
    );
    let disk = app
        .sidebar_entries()
        .into_iter()
        .find(|entry| matches!(entry.device, PerfDevice::Disk(_)))
        .expect("demo disk");
    let _ = app.update(Message::SetSidebarDeviceVisibility {
        key: disk.key.clone(),
        visible: false,
    });
    assert!(
        !app.sidebar_entries()
            .iter()
            .find(|entry| entry.key == disk.key)
            .expect("hidden device still editable")
            .visible
    );
    app.apply_settings_change(SettingsChange::ShowDevice(
        crate::app::DeviceKind::Disks,
        false,
    ));
    let _ = app.update(Message::SetSidebarDeviceVisibility {
        key: disk.key.clone(),
        visible: true,
    });
    assert!(
        app.sidebar_entries()
            .iter()
            .find(|entry| entry.key == disk.key)
            .expect("concrete override wins category")
            .visible
    );
    let _ = app.update(Message::MoveSidebarDevice {
        key: disk.key.clone(),
        delta: -20,
    });
    assert_eq!(app.sidebar_entries()[0].key, disk.key);
    let mut snapshot = app
        .shell
        .projection()
        .snapshot
        .as_ref()
        .expect("snapshot")
        .clone();
    snapshot.disks.clear();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    let before = app.config_draft();
    let _ = app.update(Message::SetSidebarDeviceVisibility {
        key: disk.key.clone(),
        visible: false,
    });
    assert_eq!(
        app.config_draft(),
        before,
        "a removed key cannot redirect to the replacement index"
    );
    let _ = app.update(Message::DismissOverlay);
    let _ = app.update(Message::SetSidebarDeviceVisibility {
        key: "memory".into(),
        visible: false,
    });
    assert_eq!(
        app.config_draft(),
        before,
        "closed controls have no write authority"
    );
}

#[test]
fn concrete_sidebar_choices_survive_coordinator_save_and_restart() {
    let root = crate::test_support::temp_dir("sidebar-config");
    let path = root.join("config.json");
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));
    let _ = app.update(Message::OpenSidebarEditor);
    let _ = app.update(Message::SetSidebarDeviceVisibility {
        key: "memory".into(),
        visible: false,
    });
    let _ = app.update(Message::MoveSidebarDevice {
        key: "memory".into(),
        delta: -1,
    });
    app.wait_for_config_where(|config| {
        config
            .sidebar_order
            .first()
            .is_some_and(|key| key == "memory")
            && config
                .sidebar_device_overrides
                .iter()
                .any(|entry| entry.device == "memory" && !entry.visible)
    });
    drop(app);
    let restarted = IcedApp::with_config_store(None, ConfigStore::new(&path));
    assert_eq!(restarted.sidebar_entries()[0].key, "memory");
    assert!(!restarted.sidebar_entries()[0].visible);
    assert_eq!(
        restarted.config_draft().show_memory,
        Config::default().show_memory,
        "concrete edits retain category defaults"
    );
    drop(restarted);
    std::fs::remove_dir_all(root).expect("cleanup");
}
