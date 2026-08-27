use super::*;

#[test]
fn test_alert_center_defaults() {
    let state = AlertCenterState::default();
    assert!(!state.quiet_hours_active);
    assert!(state.events.is_empty());
}
