//! test-intent: behavior
//! Native keys, identity-safe edits, persisted choices and actual terminal bounds.

use super::*;
use crate::ui::test_support::{install_config_store, repo_temp_dir};
use ratatui::crossterm::event::KeyModifiers;
use ratatui::{Terminal, backend::TestBackend};
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::{AppAction, AppPage, ConfigDrain};
use taskmanager_core::core::metrics::SmartAvailability;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

fn key(app: &mut TuiApp, code: KeyCode) {
    assert!(crate::runtime::handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).is_none());
}

fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("language guard");
    set_language(Language::En);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| crate::ui::render(frame, app, crate::TuiTheme::default()))
        .expect("render");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn native_sidebar_keys_edit_specific_rows_and_fixed_actions_fit_every_viewport() {
    let mut app = crate::demo_app();
    app.toggle_settings();
    key(&mut app, KeyCode::F(3));
    assert_eq!(
        app.local_surface_kind(),
        Some(TuiSurfaceKind::SidebarEditor)
    );
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char(' '));
    assert!(
        !app.sidebar_entries()
            .iter()
            .find(|entry| entry.key == "memory")
            .expect("memory")
            .visible
    );
    assert!(!app.visible_perf_devices().contains(&PerfDevice::Memory));
    key(&mut app, KeyCode::Char(' '));
    key(&mut app, KeyCode::Left);
    assert_eq!(app.sidebar_entries()[0].key, "memory");
    for (width, height) in [
        (54, 16),
        (80, 24),
        (120, 36),
        (180, 20),
        (54, 50),
        (200, 60),
    ] {
        let painted = frame_text(&app, width, height);
        for text in [
            "Edit devices",
            "> [x] Memory",
            "Space Show/Hide",
            "Move up/Move down",
            "Esc Close",
        ] {
            assert!(
                painted.contains(text),
                "{width}x{height}: missing {text}: {painted}"
            );
        }
    }
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.page(), AppPage::Performance);
    assert_eq!(app.performance_device_key.as_deref(), Some("memory"));
    assert_eq!(app.perf_device, PerfDevice::Memory);
}

#[test]
fn selected_disk_is_identity_bound_and_removed_editor_controls_do_not_redirect() {
    let mut app = crate::demo_app();
    crate::demo::apply_capture_scene_override(&mut app, "device-hotplug");
    let mut snapshot = app
        .projection()
        .snapshot
        .as_ref()
        .expect("snapshot")
        .clone();
    for disk in &mut snapshot.disks {
        disk.smart_availability = SmartAvailability::Available;
    }
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    app.open_sidebar_editor();
    let usb = app
        .sidebar_entries()
        .into_iter()
        .find(|entry| entry.key == "disk:disk:hotplug:usb0")
        .expect("USB key");
    if let Some(TuiSurface::SidebarEditor { selected }) = app.local_surface_mut() {
        *selected = Some(usb.key.clone());
    }
    key(&mut app, KeyCode::Enter);
    let smart = crate::menus::smart_self_test_target(&app).expect("selected SMART target");
    assert_eq!(smart.device_id.as_str(), "disk:hotplug:usb0");
    assert_eq!(smart.device_generation.get(), 2);
    let painted = frame_text(&app, 120, 36);
    assert!(painted.contains("TaskForest Flash"));
    assert!(
        !painted.contains("TiPro9000 2TB"),
        "another disk cannot replace the selected identity"
    );
    let mut snapshot = app
        .projection()
        .snapshot
        .as_ref()
        .expect("snapshot")
        .clone();
    snapshot
        .disks
        .retain(|disk| disk.device_id != "disk:hotplug:usb0");
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    assert!(frame_text(&app, 120, 36).contains("Device disconnected"));
    assert!(
        crate::menus::smart_self_test_target(&app).is_none(),
        "another eligible disk cannot take over its control"
    );
    app.open_sidebar_editor();
    if let Some(TuiSurface::SidebarEditor { selected }) = app.local_surface_mut() {
        *selected = Some(usb.key);
    }
    let before = app.config_draft.clone();
    key(&mut app, KeyCode::Char(' '));
    key(&mut app, KeyCode::Left);
    assert_eq!(app.config_draft, before);
}

#[test]
fn selected_gpu_control_and_history_gate_follow_the_same_concrete_identity() {
    let mut app = crate::demo_app();
    let mut snapshot = app
        .projection()
        .snapshot
        .as_ref()
        .expect("snapshot")
        .clone();
    let mut gpu = snapshot.gpu.first().expect("demo GPU").clone();
    gpu.device_id = "gpu:second".into();
    snapshot.gpu.push(gpu);
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    app.open_sidebar_editor();
    if let Some(TuiSurface::SidebarEditor { selected }) = app.local_surface_mut() {
        *selected = Some("gpu:gpu:second".into());
    }
    key(&mut app, KeyCode::Enter);
    assert_eq!(
        app.gpu_engine_rows_device_id()
            .expect("control target")
            .as_str(),
        "gpu:second"
    );
    assert_eq!(
        app.viewed_gpu().expect("history gate target").device_id,
        "gpu:second"
    );
}

#[test]
fn sidebar_keys_persist_the_shared_fields_and_restart_rehydrates_them() {
    let root = repo_temp_dir().join("tui-sidebar-config");
    std::fs::create_dir_all(&root).expect("fixture directory");
    let path = root.join("config.json");
    let mut app = crate::demo_app();
    install_config_store(&mut app, &path);
    app.open_sidebar_editor();
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char(' '));
    key(&mut app, KeyCode::Left);
    for _ in 0..64 {
        let drain = app
            .config_client
            .as_mut()
            .expect("client")
            .wait_for_drain(std::time::Duration::from_secs(2));
        assert!(
            !matches!(drain, ConfigDrain::Empty),
            "pending save must complete"
        );
        app.drain_config_publications();
        if app
            .config_client
            .as_ref()
            .and_then(|client| client.snapshot())
            .is_some_and(|config| {
                config
                    .sidebar_order
                    .first()
                    .is_some_and(|key| key == "memory")
                    && config
                        .sidebar_device_overrides
                        .iter()
                        .any(|entry| entry.device == "memory" && !entry.visible)
            })
        {
            break;
        }
    }
    drop(app);
    let mut restarted = crate::demo_app();
    install_config_store(&mut restarted, &path);
    assert_eq!(restarted.sidebar_entries()[0].key, "memory");
    assert!(!restarted.sidebar_entries()[0].visible);
    let _ = restarted.apply_action(AppAction::SelectPage(AppPage::Performance));
    drop(restarted);
    std::fs::remove_dir_all(root).expect("cleanup");
}
