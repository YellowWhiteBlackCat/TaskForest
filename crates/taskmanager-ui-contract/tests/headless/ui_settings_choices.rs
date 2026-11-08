//! Headless tests for the shared discrete settings-choice ladders.

use std::time::Duration;

use taskmanager_application::TelemetryInterval;

use super::*;

#[test]
fn refresh_index_matches_offered_steps_and_reports_off_steps_honestly() {
    let offered =
        |ms: u64| refresh_choice_index(TelemetryInterval::clamped(Duration::from_millis(ms)));
    assert_eq!(offered(500), Some(0));
    assert_eq!(offered(2_000), Some(2));
    assert_eq!(offered(5_000), Some(3));
    assert_eq!(offered(1_400), None);
}

#[test]
fn capacity_index_matches_offered_steps_and_reports_off_steps_honestly() {
    assert_eq!(capacity_choice_index(60), Some(0));
    assert_eq!(capacity_choice_index(300), Some(2));
    assert_eq!(capacity_choice_index(600), Some(3));
    assert_eq!(capacity_choice_index(10), None);
    assert_eq!(capacity_choice_index(250), None);
}
