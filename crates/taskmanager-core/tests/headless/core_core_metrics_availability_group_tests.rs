use super::*;

#[test]
fn confirmed_empty_is_distinct_from_unknown_and_unavailable() {
    let unknown = ScalarObservationGroup::<u64>::default();
    let empty = ScalarObservationGroup::<u64>::available(Vec::new(), 10);
    let unsupported = ScalarObservationGroup::<u64>::unavailable(FailureKind::Unsupported);

    assert_eq!(unknown.current_observations(), None);
    assert_eq!(empty.current_observations(), Some([].as_slice()));
    assert_eq!(empty.last_success_ms(), Some(10));
    assert_eq!(unsupported.current_observations(), None);
    assert_eq!(
        unsupported.availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );

    let decoded: ScalarObservationGroup<u64> = serde_json::from_value(
        serde_json::to_value(&empty).expect("serialize confirmed-empty group"),
    )
    .expect("confirmed-empty group must round trip");
    assert_eq!(decoded.current_observations(), Some([].as_slice()));
}

#[test]
fn failed_refresh_retains_group_only_as_stale() {
    let previous = ScalarObservationGroup::available(vec![0_u64], 10);
    let stale = ScalarObservationGroup::unavailable(FailureKind::TemporarilyUnavailable)
        .retain_previous(previous);

    assert_eq!(stale.current_observations(), None);
    assert_eq!(stale.last_known_observations().len(), 1);
    assert_eq!(
        stale.availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(stale.last_success_ms(), Some(10));
}

#[test]
fn partial_and_unavailable_groups_keep_only_their_legal_slot_states() {
    let failure = FailureKind::TemporarilyUnavailable;
    let partial = ScalarObservationGroup::partial(
        vec![
            ScalarObservationSlot::Current(0_u64),
            ScalarObservationSlot::Partial(1, failure),
            ScalarObservationSlot::Unavailable(failure),
        ],
        10,
        failure,
    );
    assert_eq!(partial.last_success_ms(), Some(10));
    assert!(
        partial.last_known_observations()[..2]
            .iter()
            .all(|slot| slot.last_success_ms() == Some(10))
    );
    assert!(
        partial
            .last_known_observations()
            .iter()
            .all(|slot| !matches!(
                slot.availability(),
                ScalarAvailability::Unknown | ScalarAvailability::Stale(_)
            ))
    );
    let unavailable = ScalarObservationGroup::<u64>::unavailable_slots(vec![failure], failure);

    for group in [partial, unavailable] {
        let decoded: ScalarObservationGroup<u64> = serde_json::from_value(
            serde_json::to_value(&group).expect("serialize valid mixed group"),
        )
        .expect("valid mixed group must round trip");
        assert_eq!(decoded, group);
    }
}
