use super::*;
use crate::core::{ProcessApplicationIdentity, ProcessItem};

#[test]
fn current_absence_is_distinct_from_failure_and_can_become_stale() {
    let absent = ProcessMetadataObservation::<PathBuf>::absent(42);
    assert!(absent.availability().is_current());
    assert_eq!(absent.current_value(), None);
    assert_eq!(absent.last_success_ms(), Some(42));

    let stale = absent.transition_failure(ProcessMetadataFailure::PidRace);
    assert_eq!(
        stale.availability(),
        ProcessMetadataAvailability::Stale(ProcessMetadataFailure::PidRace)
    );
    assert_eq!(stale.last_success_ms(), Some(42));

    let decoded: ProcessMetadataObservation<PathBuf> = serde_json::from_value(
        serde_json::to_value(&stale).expect("serialize stale confirmed absence"),
    )
    .expect("stale confirmed absence must round trip");
    assert_eq!(decoded, stale);
}

#[test]
fn failure_without_success_cannot_fabricate_stale_metadata() {
    let unavailable = ProcessMetadataObservation::<PathBuf>::default()
        .transition_failure(ProcessMetadataFailure::PermissionDenied);

    assert_eq!(
        unavailable.availability(),
        ProcessMetadataAvailability::Unavailable(ProcessMetadataFailure::PermissionDenied)
    );
    assert_eq!(unavailable.last_success_ms(), None);
}

#[test]
fn metadata_wire_preserves_real_zero() {
    let available = ProcessMetadataObservation::available(0_u64, 10);
    let decoded: ProcessMetadataObservation<u64> =
        serde_json::from_value(serde_json::to_value(&available).expect("serialize metadata zero"))
            .expect("metadata zero must round trip");
    assert_eq!(decoded.current_value(), Some(&0));
}

#[test]
fn legacy_wire_payload_falls_back_only_while_metadata_is_unknown() {
    let mut json =
        serde_json::to_value(ProcessItem::new(42, "worker")).expect("serialize process row shape");
    json["user"] = serde_json::json!("alice");
    json["exe_path"] = serde_json::json!("/usr/bin/worker");
    json.as_object_mut()
        .expect("process row JSON object")
        .remove("metadata_observations");

    let decoded: ProcessItem =
        serde_json::from_value(json).expect("deserialize legacy process row");

    assert_eq!(
        decoded.metadata_observations().owner.availability(),
        ProcessMetadataAvailability::Available
    );
    assert_eq!(decoded.current_user().as_deref(), Some("alice"));
    assert_eq!(
        decoded.current_exe_path(),
        Some(std::path::Path::new("/usr/bin/worker"))
    );
}

#[test]
fn typed_unavailability_never_falls_back_to_legacy_metadata() {
    let mut item = ProcessItem::new(42, "worker");
    item.apply_metadata_observations(ProcessMetadataObservations {
        owner: ProcessMetadataObservation::unavailable(ProcessMetadataFailure::PermissionDenied),
        executable_path: ProcessMetadataObservation::unavailable(ProcessMetadataFailure::PidRace),
    });
    let mut wire = serde_json::to_value(item).expect("serialize typed metadata");
    wire["user"] = serde_json::json!("stale-user");
    wire["exe_path"] = serde_json::json!("/stale/executable");
    let item: ProcessItem = serde_json::from_value(wire).expect("conflicting metadata payload");

    assert_eq!(item.current_user(), None);
    assert_eq!(item.current_exe_path(), None);
}

#[test]
fn typed_only_metadata_survives_without_legacy_projection_fields() {
    let mut item = ProcessItem::new(42, "worker");
    item.apply_metadata_observations(ProcessMetadataObservations::current(
        ProcessOwner::opaque("typed-owner"),
        Some(PathBuf::from("/typed/executable")),
        10,
    ));
    let mut wire = serde_json::to_value(item).expect("serialize typed metadata");
    let object = wire.as_object_mut().expect("process row object");
    object.remove("user");
    object.remove("exe_path");

    let decoded: ProcessItem = serde_json::from_value(wire).expect("typed-only process row");
    assert_eq!(decoded.current_user().as_deref(), Some("typed-owner"));
    assert_eq!(
        decoded.current_exe_path(),
        Some(std::path::Path::new("/typed/executable"))
    );
}

#[test]
fn confirmed_absent_and_stale_metadata_win_over_legacy_strings() {
    let mut absent = ProcessItem::new(42, "worker");
    absent.apply_metadata_observations(ProcessMetadataObservations {
        owner: ProcessMetadataObservation::absent(10),
        executable_path: ProcessMetadataObservation::absent(10),
    });
    let mut stale = ProcessItem::new(43, "worker");
    stale.apply_metadata_observations(ProcessMetadataObservations {
        owner: ProcessMetadataObservation::available(ProcessOwner::opaque("old-owner"), 10)
            .transition_failure(ProcessMetadataFailure::PidRace),
        executable_path: ProcessMetadataObservation::available(
            PathBuf::from("/old/executable"),
            10,
        )
        .transition_failure(ProcessMetadataFailure::PidRace),
    });

    for (mut wire, expected_availability) in [
        (
            serde_json::to_value(absent).expect("serialize absent metadata"),
            ProcessMetadataAvailability::Absent,
        ),
        (
            serde_json::to_value(stale).expect("serialize stale metadata"),
            ProcessMetadataAvailability::Stale(ProcessMetadataFailure::PidRace),
        ),
    ] {
        wire["user"] = serde_json::json!("legacy-owner");
        wire["exe_path"] = serde_json::json!("/legacy/executable");
        let decoded: ProcessItem =
            serde_json::from_value(wire).expect("deserialize conflicting metadata");
        assert_eq!(
            decoded.metadata_observations().owner.availability(),
            expected_availability
        );
        assert_eq!(decoded.current_user(), None);
        assert_eq!(decoded.current_exe_path(), None);
    }
}

#[test]
fn legacy_projections_are_derived_from_current_typed_metadata_only() {
    let mut item = ProcessItem::new(42, "worker");
    item.apply_metadata_observations(ProcessMetadataObservations {
        owner: ProcessMetadataObservation::partial(
            ProcessOwner {
                identity: ProcessOwnerIdentity::Numeric(4242),
                label: None,
            },
            50,
            ProcessMetadataFailure::NotFound,
        ),
        executable_path: ProcessMetadataObservation::absent(50),
    });

    assert_eq!(item.current_user().as_deref(), Some("4242"));
    assert_eq!(item.current_exe_path(), None);
    let wire = serde_json::to_value(item).expect("serialize canonical metadata");
    assert_eq!(wire["user"], "4242");
    assert_eq!(wire["exe_path"], serde_json::Value::Null);
}

#[test]
fn legacy_metadata_requires_a_trusted_nonzero_process_identity() {
    let mut wire =
        serde_json::to_value(ProcessItem::new(0, "idle")).expect("serialize process row shape");
    wire["user"] = serde_json::json!("legacy-owner");
    wire["exe_path"] = serde_json::json!("/legacy/executable");
    wire.as_object_mut()
        .expect("process row object")
        .remove("metadata_observations");

    let decoded: ProcessItem = serde_json::from_value(wire).expect("PID zero legacy payload");
    assert_eq!(decoded.current_user(), None);
    assert_eq!(decoded.current_exe_path(), None);
}

#[test]
fn typed_only_application_identity_survives_without_legacy_metadata_fields() {
    let identity = ProcessApplicationIdentity::new("org.example.Worker", "Worker", None)
        .expect("valid application identity");
    let mut item = ProcessItem::new(42, "worker");
    item.apply_application_identity(ProcessMetadataObservation::available(identity.clone(), 10));
    let mut wire = serde_json::to_value(item).expect("serialize application identity");
    wire.as_object_mut()
        .expect("process row object")
        .remove("user");
    wire.as_object_mut()
        .expect("process row object")
        .remove("exe_path");

    let decoded: ProcessItem = serde_json::from_value(wire).expect("typed-only process row");
    assert_eq!(decoded.current_application_identity(), Some(&identity));
}
