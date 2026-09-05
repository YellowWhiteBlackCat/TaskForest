//! test-intent: behavior
//!
//! Service details modal behavior in Bevy.

use taskmanager_core::core::services::{ServiceItem, ServiceStatus};

use super::details_modal::{ServiceDetailsModalState, service_details_modal_scene};

fn dummy_service() -> ServiceItem {
    ServiceItem::from_inventory(
        "test.service",
        "test",
        ServiceStatus::Active,
        "A test service for details modal".to_owned(),
        "loaded",
        "active",
        "running",
    )
}

#[test]
fn service_details_modal_state_open_and_close() {
    let mut state = ServiceDetailsModalState::default();
    assert!(state.target.is_none());

    let service = dummy_service();
    state.target = Some(service.clone());
    assert_eq!(state.target.as_ref().unwrap().name, "test");

    state.target = None;
    assert!(state.target.is_none());
}

#[test]
fn service_details_modal_scene_renders_without_panic() {
    let service = dummy_service();
    let theme = taskmanager_theme::Theme::default();
    let palette = crate::palette::ui_palette(&theme);
    let _scene = service_details_modal_scene(&service, &palette);
}
