use taskmanager_core::{
    FailureKind, ObservationWireError, OptionalObservation, ProcessMetadataFailure,
    ProcessMetadataObservation, ScalarObservation, ScalarObservationGroup, ScalarObservationSlot,
};

fn rejection<T>(value: serde_json::Value, expected: ObservationWireError)
where
    for<'de> T: serde::Deserialize<'de>,
{
    let error = serde_json::from_value::<T>(value)
        .err()
        .expect("contradictory observation wire payload must be rejected");
    assert!(
        error.to_string().contains(&expected.to_string()),
        "expected {expected}, got {error}"
    );
}

#[test]
fn unknown_schema_compatibility_states_round_trip_without_current_authority() {
    let scalar = ScalarObservation::<u64>::default();
    let optional = OptionalObservation::<u64>::default();
    let group = ScalarObservationGroup::<u64>::default();
    let metadata = ProcessMetadataObservation::<u64>::default();

    let scalar: ScalarObservation<u64> =
        serde_json::from_value(serde_json::to_value(scalar).expect("serialize Unknown scalar"))
            .expect("Unknown scalar must remain compatible");
    let optional: OptionalObservation<u64> =
        serde_json::from_value(serde_json::to_value(optional).expect("serialize Unknown optional"))
            .expect("Unknown optional must remain compatible");
    let group: ScalarObservationGroup<u64> =
        serde_json::from_value(serde_json::to_value(group).expect("serialize Unknown group"))
            .expect("Unknown group must remain compatible");
    let metadata: ProcessMetadataObservation<u64> =
        serde_json::from_value(serde_json::to_value(metadata).expect("serialize Unknown metadata"))
            .expect("Unknown metadata must remain compatible");

    assert_eq!(scalar.current_value(), None);
    assert_eq!(optional.current_value(), None);
    assert_eq!(group.current_observations(), None);
    assert_eq!(metadata.current_value(), None);
}

#[test]
fn scalar_wire_keeps_unknown_and_zero_but_rejects_impossible_freshness() {
    let zero = ScalarObservation::available(0_u64, 10);
    let decoded: ScalarObservation<u64> =
        serde_json::from_value(serde_json::to_value(zero).expect("serialize observed zero"))
            .expect("observed zero must round trip");
    assert_eq!(decoded.current_value(), Some(&0));

    let decoded: ScalarObservation<u64> = serde_json::from_value(serde_json::json!({
        "value": 0,
        "availability": { "status": "unknown" },
        "last_success_ms": null,
    }))
    .expect("Unknown wire payload remains schema-compatible");
    assert_eq!(decoded.current_value(), None);
    assert_eq!(decoded.last_known_value(), Some(&0));

    let failure = FailureKind::ProviderFault;
    rejection::<ScalarObservation<u64>>(
        serde_json::json!({
            "value": null,
            "availability": { "status": "available" },
            "last_success_ms": 10,
        }),
        ObservationWireError::CurrentValueMissing,
    );
    rejection::<ScalarObservation<u64>>(
        serde_json::json!({
            "value": 0,
            "availability": { "status": "partial", "failure": failure },
            "last_success_ms": null,
        }),
        ObservationWireError::CurrentSuccessTimeMissing,
    );
    rejection::<ScalarObservation<u64>>(
        serde_json::json!({
            "value": null,
            "availability": { "status": "stale", "failure": failure },
            "last_success_ms": 10,
        }),
        ObservationWireError::StaleHistoryMissing,
    );
    rejection::<ScalarObservation<u64>>(
        serde_json::json!({
            "value": 0,
            "availability": { "status": "unavailable", "failure": failure },
            "last_success_ms": null,
        }),
        ObservationWireError::UnavailableCarriesValue,
    );
    rejection::<ScalarObservation<u64>>(
        serde_json::json!({
            "value": null,
            "availability": { "status": "unavailable", "failure": failure },
            "last_success_ms": 10,
        }),
        ObservationWireError::UnavailableCarriesSuccessTime,
    );
}

#[test]
fn optional_wire_keeps_presence_orthogonal_and_rejects_contradictions() {
    for observation in [
        OptionalObservation::present(0_u64, 10),
        OptionalObservation::absent(10),
        OptionalObservation::not_applicable(10),
    ] {
        let decoded: OptionalObservation<u64> = serde_json::from_value(
            serde_json::to_value(&observation).expect("serialize valid optional observation"),
        )
        .expect("valid optional observation must round trip");
        assert_eq!(decoded, observation);
    }

    let failure = FailureKind::ProviderFault;
    rejection::<OptionalObservation<u64>>(
        serde_json::json!({
            "state": { "state": "unknown" },
            "availability": { "status": "available" },
            "last_success_ms": 10,
        }),
        ObservationWireError::CurrentStateUnknown,
    );
    rejection::<OptionalObservation<u64>>(
        serde_json::json!({
            "state": { "state": "present", "value": 0 },
            "availability": { "status": "partial", "failure": failure },
            "last_success_ms": null,
        }),
        ObservationWireError::CurrentSuccessTimeMissing,
    );
    rejection::<OptionalObservation<u64>>(
        serde_json::json!({
            "state": { "state": "unknown" },
            "availability": { "status": "stale", "failure": failure },
            "last_success_ms": 10,
        }),
        ObservationWireError::StaleHistoryMissing,
    );
    rejection::<OptionalObservation<u64>>(
        serde_json::json!({
            "state": { "state": "present", "value": 0 },
            "availability": { "status": "unavailable", "failure": failure },
            "last_success_ms": null,
        }),
        ObservationWireError::UnavailableCarriesState,
    );
    rejection::<OptionalObservation<u64>>(
        serde_json::json!({
            "state": { "state": "unknown" },
            "availability": { "status": "unavailable", "failure": failure },
            "last_success_ms": 10,
        }),
        ObservationWireError::UnavailableCarriesSuccessTime,
    );
}

#[test]
fn scalar_group_wire_distinguishes_empty_partial_stale_and_unavailable() {
    let failure = FailureKind::TemporarilyUnavailable;
    let valid = [
        ScalarObservationGroup::available(Vec::<u64>::new(), 10),
        ScalarObservationGroup::partial(
            vec![
                ScalarObservationSlot::Current(0),
                ScalarObservationSlot::Unavailable(failure),
            ],
            10,
            failure,
        ),
        ScalarObservationGroup::unavailable_slots(vec![failure], failure),
    ];
    for group in valid {
        let decoded: ScalarObservationGroup<u64> = serde_json::from_value(
            serde_json::to_value(&group).expect("serialize valid observation group"),
        )
        .expect("valid observation group must round trip");
        assert_eq!(decoded, group);
    }

    rejection::<ScalarObservationGroup<u64>>(
        serde_json::json!({
            "observations": [],
            "availability": { "status": "available" },
            "last_success_ms": null,
        }),
        ObservationWireError::CurrentSuccessTimeMissing,
    );
    rejection::<ScalarObservationGroup<u64>>(
        serde_json::json!({
            "observations": [ScalarObservation::<u64>::unavailable(failure)],
            "availability": { "status": "available" },
            "last_success_ms": 10,
        }),
        ObservationWireError::AvailableGroupContainsNonAvailableItem,
    );
    rejection::<ScalarObservationGroup<u64>>(
        serde_json::json!({
            "observations": [ScalarObservation::partial(0_u64, 10, failure)],
            "availability": { "status": "available" },
            "last_success_ms": 10,
        }),
        ObservationWireError::AvailableGroupContainsNonAvailableItem,
    );
    rejection::<ScalarObservationGroup<u64>>(
        serde_json::json!({
            "observations": [ScalarObservation::<u64>::default()],
            "availability": { "status": "partial", "failure": failure },
            "last_success_ms": 10,
        }),
        ObservationWireError::PartialGroupContainsNonCurrentItem,
    );
    rejection::<ScalarObservationGroup<u64>>(
        serde_json::json!({
            "observations": [ScalarObservation::stale(0_u64, 10, failure)],
            "availability": { "status": "partial", "failure": failure },
            "last_success_ms": 10,
        }),
        ObservationWireError::PartialGroupContainsNonCurrentItem,
    );
    rejection::<ScalarObservationGroup<u64>>(
        serde_json::json!({
            "observations": [ScalarObservation::available(0_u64, 9)],
            "availability": { "status": "partial", "failure": failure },
            "last_success_ms": 10,
        }),
        ObservationWireError::GroupSuccessTimeMismatch,
    );
    rejection::<ScalarObservationGroup<u64>>(
        serde_json::json!({
            "observations": [ScalarObservation::available(0_u64, 10)],
            "availability": { "status": "stale", "failure": failure },
            "last_success_ms": 10,
        }),
        ObservationWireError::StaleGroupContainsNonHistoricalItem,
    );
    rejection::<ScalarObservationGroup<u64>>(
        serde_json::json!({
            "observations": [ScalarObservation::<u64>::default()],
            "availability": { "status": "stale", "failure": failure },
            "last_success_ms": 10,
        }),
        ObservationWireError::StaleGroupContainsNonHistoricalItem,
    );
    rejection::<ScalarObservationGroup<u64>>(
        serde_json::json!({
            "observations": [],
            "availability": { "status": "stale", "failure": failure },
            "last_success_ms": null,
        }),
        ObservationWireError::StaleHistoryMissing,
    );
    rejection::<ScalarObservationGroup<u64>>(
        serde_json::json!({
            "observations": [ScalarObservation::available(0_u64, 10).transition_failure(failure)],
            "availability": { "status": "unavailable", "failure": failure },
            "last_success_ms": null,
        }),
        ObservationWireError::UnavailableGroupContainsHistory,
    );
}

#[test]
fn process_metadata_wire_keeps_absence_history_and_rejects_impossible_states() {
    let zero = ProcessMetadataObservation::available(0_u64, 10);
    let stale_absence = ProcessMetadataObservation::<u64>::absent(10)
        .transition_failure(ProcessMetadataFailure::PidRace);
    for observation in [zero, stale_absence] {
        let decoded: ProcessMetadataObservation<u64> = serde_json::from_value(
            serde_json::to_value(&observation).expect("serialize valid process metadata"),
        )
        .expect("valid process metadata must round trip");
        assert_eq!(decoded, observation);
    }

    let failure = ProcessMetadataFailure::ProviderFault;
    rejection::<ProcessMetadataObservation<u64>>(
        serde_json::json!({
            "value": null,
            "availability": { "status": "available" },
            "last_success_ms": 10,
        }),
        ObservationWireError::CurrentValueMissing,
    );
    rejection::<ProcessMetadataObservation<u64>>(
        serde_json::json!({
            "value": 0,
            "availability": { "status": "partial", "failure": failure },
            "last_success_ms": null,
        }),
        ObservationWireError::CurrentSuccessTimeMissing,
    );
    rejection::<ProcessMetadataObservation<u64>>(
        serde_json::json!({
            "value": 0,
            "availability": { "status": "absent" },
            "last_success_ms": 10,
        }),
        ObservationWireError::AbsentCarriesValue,
    );
    rejection::<ProcessMetadataObservation<u64>>(
        serde_json::json!({
            "value": null,
            "availability": { "status": "stale", "failure": failure },
            "last_success_ms": null,
        }),
        ObservationWireError::StaleHistoryMissing,
    );
    rejection::<ProcessMetadataObservation<u64>>(
        serde_json::json!({
            "value": 0,
            "availability": { "status": "unavailable", "failure": failure },
            "last_success_ms": null,
        }),
        ObservationWireError::UnavailableCarriesValue,
    );
    rejection::<ProcessMetadataObservation<u64>>(
        serde_json::json!({
            "value": null,
            "availability": { "status": "unavailable", "failure": failure },
            "last_success_ms": 10,
        }),
        ObservationWireError::UnavailableCarriesSuccessTime,
    );
}
