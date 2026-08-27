use std::time::{Duration, UNIX_EPOCH};

use super::{
    LocalTimeRules, LocalTimeRulesChange, LocalTimeRulesError, LocalTimeRulesObservation,
    unix_micros, unix_millis,
};
use crate::core::FailureKind;

fn daylight_tzif() -> Vec<u8> {
    let mut bytes = vec![0_u8; 44];
    bytes[..4].copy_from_slice(b"TZif");
    bytes[32..36].copy_from_slice(&2_u32.to_be_bytes());
    bytes[36..40].copy_from_slice(&2_u32.to_be_bytes());
    bytes.extend_from_slice(&0_i32.to_be_bytes());
    bytes.extend_from_slice(&10_000_i32.to_be_bytes());
    bytes.extend_from_slice(&[1, 0]);
    bytes.extend_from_slice(&3_600_i32.to_be_bytes());
    bytes.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(&7_200_i32.to_be_bytes());
    bytes.extend_from_slice(&[1, 0]);
    bytes
}

#[test]
fn injected_wall_clock_converts_without_reading_the_host_clock() {
    let instant = UNIX_EPOCH + Duration::from_micros(1_234_567);
    assert_eq!(unix_millis(instant), 1_234);
    assert_eq!(unix_micros(instant), 1_234_567);

    let before_epoch = UNIX_EPOCH - Duration::from_secs(1);
    assert_eq!(unix_millis(before_epoch), 0);
    assert_eq!(unix_micros(before_epoch), 0);
}

#[test]
fn validated_rules_project_standard_and_daylight_offsets() {
    let rules = LocalTimeRules::from_tzif(&daylight_tzif()).expect("valid TZif fixture");
    let before = rules.date_time_at(-1).expect("standard time");
    assert_eq!(before.offset().seconds_east_of_utc(), 3_600);
    assert!(!before.offset().is_daylight_saving());

    let daylight = rules.date_time_at(0).expect("daylight time");
    assert_eq!(daylight.hour(), 2);
    assert_eq!(daylight.offset().seconds_east_of_utc(), 7_200);
    assert!(daylight.offset().is_daylight_saving());

    let standard = rules.date_time_at(10_000).expect("standard time again");
    assert_eq!(standard.offset().seconds_east_of_utc(), 3_600);
    assert!(!standard.offset().is_daylight_saving());
}

#[test]
fn variable_rules_stop_at_the_explicit_transition_horizon() {
    let rules = LocalTimeRules::from_tzif(&daylight_tzif()).expect("valid TZif fixture");
    assert_eq!(rules.valid_through_utc_seconds(), Some(10_000));
    assert!(rules.offset_at(10_000).is_some());
    assert_eq!(rules.offset_at(10_001), None);
    assert_eq!(rules.date_time_at(i64::MAX), None);
    assert_eq!(LocalTimeRules::utc().date_time_at(i64::MAX), None);
    assert_eq!(LocalTimeRules::utc().date_time_at(i64::MIN), None);
}

#[test]
fn hostile_minimum_offset_is_rejected_without_panicking() {
    let mut bytes = daylight_tzif();
    bytes[54..58].copy_from_slice(&i32::MIN.to_be_bytes());
    assert_eq!(
        LocalTimeRules::from_tzif(&bytes),
        Err(LocalTimeRulesError::InvalidOffset)
    );
}

#[test]
fn unavailable_rules_never_project_utc_as_local() {
    let unavailable = LocalTimeRulesObservation::unavailable(FailureKind::PermissionDenied, 7);
    assert_eq!(unavailable.date_time_at(0), None);
    assert_eq!(unavailable.observed_at_ms(), 7);
    assert_eq!(
        LocalTimeRules::from_tzif(&[]),
        Err(LocalTimeRulesError::Empty)
    );
}

#[test]
fn rule_changes_are_typed() {
    let previous = LocalTimeRulesObservation::current(LocalTimeRules::utc(), 1);
    let current =
        LocalTimeRulesObservation::current(LocalTimeRules::from_tzif(&daylight_tzif()).unwrap(), 2);
    assert!(matches!(
        current.change_since(&previous),
        LocalTimeRulesChange::RulesChanged
    ));
    assert_eq!(
        current.change_since(&current),
        LocalTimeRulesChange::Unchanged
    );
    assert!(matches!(
        LocalTimeRulesObservation::unsupported(3).change_since(&current),
        LocalTimeRulesChange::BecameUnavailable {
            failure: FailureKind::Unsupported
        }
    ));
}

#[test]
fn semantic_rule_value_is_the_cache_and_change_authority() {
    let first = LocalTimeRules::from_tzif(&daylight_tzif()).unwrap();
    let second = LocalTimeRules::from_tzif(&daylight_tzif()).unwrap();
    assert_eq!(first, second);
    let first = LocalTimeRulesObservation::current(first.clone(), 1);
    let second = LocalTimeRulesObservation::current(second.clone(), 2);
    assert_eq!(first.cache_key(), second.cache_key());
    assert_eq!(second.change_since(&first), LocalTimeRulesChange::Unchanged);
}
