use super::*;

#[test]
fn battery_status_labels_cover_every_state() {
    for (state, expected) in [
        (battery::State::Charging, "Charging"),
        (battery::State::Discharging, "Discharging"),
        (battery::State::Full, "Full"),
        (battery::State::Empty, "Empty"),
        (battery::State::Unknown, "Unknown"),
    ] {
        assert_eq!(status_label(state), expected);
    }
}

#[test]
fn invalid_battery_scalars_never_turn_into_zero() {
    assert_eq!(
        percent_observation(f32::NAN, 1),
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(
        percent_observation(101.0, 1),
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(
        voltage_observation(-1.0, 1),
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(
        finite_nonnegative_observation(f32::INFINITY, 1),
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    );
}

/// Wh quantities widen to the shared µWh fact axis; a non-finite or negative
/// quantity (the crate's non-`Option` "not reported" signal) is typed
/// unsupported, never a zero.
#[test]
fn energy_facts_convert_wh_to_uwh_and_hide_not_reported_values() {
    assert_eq!(
        watt_hours_observation(49.0, 1),
        ScalarObservation::available(49_000_000.0, 1)
    );
    assert_eq!(
        watt_hours_observation(f64::NAN, 1),
        ScalarObservation::unavailable(FailureKind::Unsupported)
    );
    assert_eq!(
        watt_hours_observation(-1.0, 1),
        ScalarObservation::unavailable(FailureKind::Unsupported)
    );
}

/// A native estimate survives as seconds; `None` (no estimate / status-gated
/// by the crate itself) is typed unsupported and a non-finite value is a
/// provider fault — neither becomes a believable zero.
#[test]
fn estimate_observations_map_none_to_unsupported_and_never_zero() {
    assert_eq!(
        estimate_observation(Some(3_780.5), 1),
        ScalarObservation::available(3_780.5, 1)
    );
    assert_eq!(
        estimate_observation(None, 1),
        ScalarObservation::unavailable(FailureKind::Unsupported)
    );
    assert_eq!(
        estimate_observation(Some(f64::INFINITY), 1),
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(
        estimate_observation(Some(-60.0), 1),
        ScalarObservation::unavailable(FailureKind::ProviderFault)
    );
}

#[test]
fn battery_snapshot_has_coherent_discovery_authority() {
    let snapshot =
        collect_battery_snapshot("fixture", ProviderId::borrowed("fixture.power.battery"), 1)
            .expect("battery crate returns a typed snapshot");
    taskmanager_platform_conformance::assert_device_discovery_consistent(&snapshot)
        .expect("portable battery discovery must be coherent");
}
