use std::time::Duration;

use super::*;
#[test]
fn pause_blocks_first_and_elapsed_scheduler_decisions() {
    let mut policy = TelemetryRefreshPolicy::default();
    policy.apply(TelemetryRefreshPolicyChange::SetPaused(true));

    assert!(!policy.should_submit(None));
    assert!(!policy.should_submit(Some(Duration::from_secs(10))));
}

#[test]
fn held_control_blocks_refresh_until_the_modifier_is_released() {
    let mut policy = TelemetryRefreshPolicy::default();
    policy.apply(TelemetryRefreshPolicyChange::SetControlHeld(true));

    assert!(policy.is_control_held());
    assert!(policy.is_paused());
    assert!(!policy.should_submit(None));
    assert!(!policy.should_submit(Some(Duration::from_secs(10))));

    policy.apply(TelemetryRefreshPolicyChange::SetControlHeld(false));

    assert!(!policy.is_control_held());
    assert!(!policy.is_paused());
    assert!(policy.should_submit(None));
}

#[test]
fn manual_and_held_pauses_are_independent_and_composable() {
    let mut policy = TelemetryRefreshPolicy::default();
    policy.apply(TelemetryRefreshPolicyChange::SetPaused(true));
    policy.apply(TelemetryRefreshPolicyChange::SetControlHeld(true));

    assert!(policy.is_manually_paused());
    assert!(policy.is_control_held());
    assert!(policy.is_paused());

    policy.apply(TelemetryRefreshPolicyChange::SetControlHeld(false));
    assert!(policy.is_paused(), "manual pause must survive Ctrl release");

    policy.apply(TelemetryRefreshPolicyChange::SetInterval(
        TelemetryInterval::default(),
    ));
    assert!(!policy.is_manually_paused());
    assert!(!policy.is_paused());
}

#[test]
fn changing_interval_does_not_clear_a_held_control_pause() {
    let mut policy = TelemetryRefreshPolicy::default();
    policy.apply(TelemetryRefreshPolicyChange::SetControlHeld(true));
    policy.apply(TelemetryRefreshPolicyChange::SetInterval(
        TelemetryInterval::default(),
    ));

    assert!(policy.is_control_held());
    assert!(policy.is_paused());
    assert!(!policy.should_submit(Some(Duration::from_secs(60))));
}

#[test]
fn interval_update_is_synchronous_and_resumes_scheduling() {
    let mut policy = TelemetryRefreshPolicy::default();
    policy.apply(TelemetryRefreshPolicyChange::SetPaused(true));
    let interval =
        TelemetryInterval::new(Duration::from_millis(250)).expect("fixture interval is valid");

    policy.apply(TelemetryRefreshPolicyChange::SetInterval(interval));

    assert_eq!(policy.interval(), interval);
    assert!(!policy.is_paused());
    assert!(!policy.should_submit(Some(Duration::from_millis(249))));
    assert!(policy.should_submit(Some(Duration::from_millis(250))));
}

#[test]
fn zero_elapsed_is_real_and_not_an_uninitialized_marker() {
    let policy = TelemetryRefreshPolicy::default();

    assert!(policy.should_submit(None));
    assert!(!policy.should_submit(Some(Duration::ZERO)));
}

#[test]
fn invalid_zero_duration_cannot_enter_policy() {
    assert_eq!(
        TelemetryInterval::new(Duration::ZERO),
        Err(TelemetryIntervalError::TooFast)
    );
}

#[test]
fn persisted_values_clamp_to_valid_scheduler_bounds() {
    assert_eq!(
        TelemetryInterval::clamped(Duration::ZERO).duration(),
        MIN_TELEMETRY_INTERVAL
    );
    assert_eq!(
        TelemetryInterval::clamped(Duration::from_secs(90)).duration(),
        MAX_TELEMETRY_INTERVAL
    );
}
