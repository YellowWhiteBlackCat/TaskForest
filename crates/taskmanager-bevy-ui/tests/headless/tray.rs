//! Headless test coverage for the Bevy UI system tray seam (ADR-032).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::channel;

use taskmanager_assets::{PRODUCT_TRAY_ICON_SIZE, product};
use taskmanager_core::core::tray::{TrayActionId, TrayEvent, TrayMenuItem};
use taskmanager_platform_contract::{TrayController, TrayFailure};

use super::*;

#[derive(Default)]
struct MockTrayController {
    last_checked: Arc<AtomicBool>,
    last_action_id: Arc<AtomicU32>,
}

impl TrayController for MockTrayController {
    fn set_visible(&self, _visible: bool) -> Result<(), TrayFailure> {
        Ok(())
    }

    fn set_tooltip(&self, _tooltip: Option<String>) -> Result<(), TrayFailure> {
        Ok(())
    }

    fn set_title(&self, _title: Option<String>) -> Result<(), TrayFailure> {
        Ok(())
    }

    fn set_item_checked(&self, id: TrayActionId, checked: bool) -> Result<(), TrayFailure> {
        self.last_action_id.store(id, Ordering::Relaxed);
        self.last_checked.store(checked, Ordering::Relaxed);
        Ok(())
    }
}

#[test]
fn action_mapping_is_complete_and_rejects_unknown_ids() {
    assert_eq!(
        resolve_tray_action(TRAY_ACTION_SHOW),
        Some(TrayIntent::Show)
    );
    assert_eq!(
        resolve_tray_action(TRAY_ACTION_SHOW),
        Some(TrayIntent::ShowWindow)
    );
    assert_eq!(
        resolve_tray_action(TRAY_ACTION_PAUSE),
        Some(TrayIntent::TogglePause)
    );
    assert_eq!(
        resolve_tray_action(TRAY_ACTION_QUIT),
        Some(TrayIntent::Quit)
    );
    assert_eq!(resolve_tray_action(0), None);
    assert_eq!(resolve_tray_action(4), None);
    assert_eq!(resolve_tray_action(99), None);
}

#[test]
fn tray_icon_pixels_has_valid_dimensions_and_opacity() {
    let icon = tray_icon_pixels().expect("product tray icon should be valid RGBA bitmap");
    assert_eq!(icon.width(), PRODUCT_TRAY_ICON_SIZE);
    assert_eq!(icon.height(), PRODUCT_TRAY_ICON_SIZE);
    assert_eq!(
        icon.pixels().len(),
        (PRODUCT_TRAY_ICON_SIZE * PRODUCT_TRAY_ICON_SIZE * 4) as usize
    );

    let pixel = |x: u32, y: u32| {
        let i = ((y * PRODUCT_TRAY_ICON_SIZE + x) * 4) as usize;
        icon.pixels()[i..i + 4].to_vec()
    };

    assert_eq!(pixel(0, 0)[3], 0, "tray icon corner must be transparent");
    assert!(
        icon.pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|rgba| rgba[3] != 0),
        "tray icon must contain visible pixels"
    );
    assert!(
        pixel(PRODUCT_TRAY_ICON_SIZE / 2, PRODUCT_TRAY_ICON_SIZE / 2)[3] != 0,
        "center pixel must be visible"
    );
}

#[test]
fn bevy_tray_spec_uses_bevy_product_identity_and_localized_items() {
    let spec = build_tray_spec(false).expect("tray spec should build successfully");
    assert_eq!(spec.title(), Some(product::BEVY_NAME));
    assert_eq!(
        spec.tooltip(),
        Some(taskmanager_application::i18n::t("tray.tooltip"))
    );
    assert!(!spec.show_menu_on_left_click());

    let items = spec.menu().items();
    assert_eq!(items.len(), 4);

    match &items[0] {
        TrayMenuItem::Action { id, label, enabled } => {
            assert_eq!(*id, TRAY_ACTION_SHOW);
            assert_eq!(label, taskmanager_application::i18n::t("tray.show_window"));
            assert!(enabled);
        }
        other => panic!("expected Show Action item, got {other:?}"),
    }

    match &items[1] {
        TrayMenuItem::Checkmark {
            id,
            label,
            checked,
            enabled,
        } => {
            assert_eq!(*id, TRAY_ACTION_PAUSE);
            assert_eq!(
                label,
                taskmanager_application::i18n::t("tray.pause_refresh")
            );
            assert!(!checked);
            assert!(enabled);
        }
        other => panic!("expected Pause Checkmark item, got {other:?}"),
    }

    assert!(matches!(&items[2], TrayMenuItem::Separator));

    match &items[3] {
        TrayMenuItem::Action { id, label, enabled } => {
            assert_eq!(*id, TRAY_ACTION_QUIT);
            assert_eq!(label, taskmanager_application::i18n::t("tray.quit"));
            assert!(enabled);
        }
        other => panic!("expected Quit Action item, got {other:?}"),
    }
}

#[test]
fn bevy_tray_spec_reflects_paused_state() {
    let paused_spec = build_tray_spec(true).expect("spec should build");
    match &paused_spec.menu().items()[1] {
        TrayMenuItem::Checkmark { checked, .. } => assert!(checked),
        other => panic!("expected Checkmark, got {other:?}"),
    }

    let active_spec = build_tray_spec(false).expect("spec should build");
    match &active_spec.menu().items()[1] {
        TrayMenuItem::Checkmark { checked, .. } => assert!(!checked),
        other => panic!("expected Checkmark, got {other:?}"),
    }
}

#[test]
fn tray_resource_lifecycle_and_defaults() {
    let mut tray = TrayResource::default();
    assert!(!tray.is_active());
    assert!(tray.drain_events().is_empty());
    tray.sync_pause_checkmark(true);
    sync_tray_pause_checkmark(&tray, true);
}

#[test]
fn tray_resource_event_draining() {
    let (tx, rx) = channel();
    let mut tray = TrayResource::new(None, Some(rx));

    tx.send(TrayEvent::IconActivated).expect("send");
    tx.send(TrayEvent::MenuActivated {
        id: TRAY_ACTION_SHOW,
    })
    .expect("send");
    tx.send(TrayEvent::MenuActivated {
        id: TRAY_ACTION_PAUSE,
    })
    .expect("send");
    tx.send(TrayEvent::MenuActivated {
        id: TRAY_ACTION_QUIT,
    })
    .expect("send");

    let events = tray.drain_events();
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0], TrayEvent::IconActivated));
    assert_eq!(
        resolve_tray_action(match events[1] {
            TrayEvent::MenuActivated { id } => id,
            _ => 0,
        }),
        Some(TrayIntent::Show)
    );
    assert_eq!(
        resolve_tray_action(match events[2] {
            TrayEvent::MenuActivated { id } => id,
            _ => 0,
        }),
        Some(TrayIntent::TogglePause)
    );
    assert_eq!(
        resolve_tray_action(match events[3] {
            TrayEvent::MenuActivated { id } => id,
            _ => 0,
        }),
        Some(TrayIntent::Quit)
    );

    assert!(tray.drain_events().is_empty());
}

#[test]
fn sync_tray_pause_checkmark_drives_controller() {
    let last_checked = Arc::new(AtomicBool::new(false));
    let last_action_id = Arc::new(AtomicU32::new(0));
    let mock: Box<dyn TrayController> = Box::new(MockTrayController {
        last_checked: Arc::clone(&last_checked),
        last_action_id: Arc::clone(&last_action_id),
    });

    sync_tray_pause_checkmark(&mock, true);
    assert_eq!(last_action_id.load(Ordering::Relaxed), TRAY_ACTION_PAUSE);
    assert!(last_checked.load(Ordering::Relaxed));

    let tray = TrayResource::new(Some(mock), None);
    assert!(tray.is_active());
    tray.sync_pause_checkmark(false);
    assert_eq!(last_action_id.load(Ordering::Relaxed), TRAY_ACTION_PAUSE);
    assert!(!last_checked.load(Ordering::Relaxed));

    assert_eq!(
        format!("{tray:?}"),
        "TrayResource { has_controller: true, has_events_rx: false }"
    );
}

#[test]
fn tray_resource_empty_and_active_states() {
    let empty = TrayResource::empty();
    assert!(!empty.is_active());
    assert!(empty.controller.is_none());
    assert!(empty.events_rx.is_none());
}

#[test]
fn tray_intent_constants_and_aliases() {
    assert_eq!(TrayIntent::SHOW_WINDOW, TrayIntent::Show);
    assert_eq!(TrayIntent::ShowWindow, TrayIntent::Show);
}

#[test]
fn tray_controller_target_polymorphism() {
    let last_checked = Arc::new(AtomicBool::new(false));
    let last_action_id = Arc::new(AtomicU32::new(0));
    let mock: Box<dyn TrayController> = Box::new(MockTrayController {
        last_checked: Arc::clone(&last_checked),
        last_action_id: Arc::clone(&last_action_id),
    });

    let opt_box: Option<Box<dyn TrayController>> = Some(mock);
    sync_tray_pause_checkmark(&opt_box, true);
    assert_eq!(last_action_id.load(Ordering::Relaxed), TRAY_ACTION_PAUSE);
    assert!(last_checked.load(Ordering::Relaxed));

    let opt_ref: Option<&dyn TrayController> = opt_box.as_deref();
    sync_tray_pause_checkmark(&opt_ref, false);
    assert_eq!(last_action_id.load(Ordering::Relaxed), TRAY_ACTION_PAUSE);
    assert!(!last_checked.load(Ordering::Relaxed));

    if let Some(ctrl) = opt_box.as_deref() {
        sync_tray_pause_checkmark(ctrl, true);
        assert!(last_checked.load(Ordering::Relaxed));
    }
}

#[test]
fn spawn_tray_host_gracefully_handles_headless_runtime() {
    let (controller, rx) = spawn_tray_host(false);
    if let Some(c) = &controller {
        let _ = c.set_item_checked(TRAY_ACTION_PAUSE, false);
    }
    if let Some(r) = &rx {
        let _ = r.try_recv();
    }
}
