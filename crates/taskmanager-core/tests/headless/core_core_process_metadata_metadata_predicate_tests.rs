use super::{
    PathBuf, ProcessMetadataAvailability, ProcessMetadataFailure, ProcessMetadataObservation,
    ProcessMetadataObservations, ProcessOwner,
};

fn availability(value: ProcessMetadataObservation<u64>) -> ProcessMetadataAvailability {
    value.availability
}

#[test]
fn is_current_and_has_current_value_truth_table() {
    // Every variant, each in both predicates — a constant-true mutation of
    // either predicate is caught by the Absent/Stale rows.
    let cases = [
        (
            ProcessMetadataObservation::<u64>::available(1, 10),
            true,
            true,
        ),
        (
            ProcessMetadataObservation::<u64>::partial(1, 10, ProcessMetadataFailure::NotFound),
            true,
            true,
        ),
        (ProcessMetadataObservation::<u64>::absent(10), true, false),
        (
            ProcessMetadataObservation::<u64>::default()
                .transition_failure(ProcessMetadataFailure::PidRace),
            false,
            false,
        ),
        (
            ProcessMetadataObservation::<u64>::unavailable(
                ProcessMetadataFailure::PermissionDenied,
            ),
            false,
            false,
        ),
    ];
    for (observation, want_current, want_value) in cases {
        let state = availability(observation);
        assert_eq!(
            state.is_current(),
            want_current,
            "{state:?} is_current must be {want_current}"
        );
        assert_eq!(
            state.has_current_value(),
            want_value,
            "{state:?} has_current_value must be {want_value}"
        );
    }
}

#[test]
fn transition_failure_becomes_stale_only_with_a_success_and_clears_the_value_otherwise() {
    let with_success = ProcessMetadataObservation::<u64>::available(7, 10)
        .transition_failure(ProcessMetadataFailure::PidRace);
    assert!(matches!(
        with_success.availability,
        ProcessMetadataAvailability::Stale(ProcessMetadataFailure::PidRace)
    ));
    assert_eq!(
        with_success.current_value(),
        None,
        "a stale value is not presented as current"
    );
    assert_eq!(
        with_success.last_known_value(),
        Some(&7),
        "stale keeps the last-known value"
    );

    let no_success =
        ProcessMetadataObservation::<u64>::unavailable(ProcessMetadataFailure::PermissionDenied)
            .transition_failure(ProcessMetadataFailure::PidRace);
    assert!(matches!(
        no_success.availability,
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PidRace)
    ));
    assert_eq!(
        no_success.current_value(),
        None,
        "no-success failure clears the value"
    );
}

#[test]
fn retain_previous_bridges_only_an_unavailable_gap() {
    let previous = ProcessMetadataObservation::<u64>::available(7, 10);
    // Unavailable + previous → the previous value rides over, marked stale.
    let bridged =
        ProcessMetadataObservation::<u64>::unavailable(ProcessMetadataFailure::PermissionDenied)
            .retain_previous(previous.clone());
    assert!(matches!(
        bridged.availability,
        ProcessMetadataAvailability::Stale(ProcessMetadataFailure::PermissionDenied)
    ));
    assert_eq!(
        bridged.current_value(),
        None,
        "a bridged value is not presented as current"
    );
    assert_eq!(bridged.last_known_value(), Some(&7));

    // Available self → untouched (a `retain_previous → Default` mutation
    // would wipe the value here).
    let fresh = ProcessMetadataObservation::<u64>::available(9, 20);
    let kept = fresh.clone().retain_previous(previous);
    assert_eq!(kept, fresh);
}

#[test]
fn observations_container_transition_failure_maps_every_field() {
    let observations = ProcessMetadataObservations {
        owner: ProcessMetadataObservation::available(
            ProcessOwner {
                identity: super::ProcessOwnerIdentity::Opaque("1000".into()),
                label: Some("alice".into()),
            },
            10,
        ),
        executable_path: ProcessMetadataObservation::available(PathBuf::from("/usr/bin/alice"), 10),
    };
    let failed = observations.transition_failure(ProcessMetadataFailure::PidRace);
    assert!(matches!(
        failed.owner.availability,
        ProcessMetadataAvailability::Stale(ProcessMetadataFailure::PidRace)
    ));
    assert!(matches!(
        failed.executable_path.availability,
        ProcessMetadataAvailability::Stale(ProcessMetadataFailure::PidRace)
    ));
    assert_eq!(
        failed.owner.last_known_value().map(|o| o.display_value()),
        Some("alice".to_string())
    );
}
