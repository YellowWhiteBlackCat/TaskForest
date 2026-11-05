use super::*;

use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{ProcessCategory, ProcessItem, ProcessScalarObservations};
use taskforest_core::core::process_telemetry::ProcessIdentity;

#[test]
fn row_identity_is_derived_only_from_a_current_process_observation() {
    let process =
        ProcessItem::new(42, "worker").with_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(100, 1),
            ..ProcessScalarObservations::default()
        });
    let row_identity = ProcessLiveKey::from_process(&process).expect("current identity");

    assert_eq!(row_identity.pid(), 42);
    assert_eq!(row_identity.start_token(), 100);
    assert_eq!(
        row_identity.identity(),
        ProcessIdentity {
            pid: 42,
            start_token: 100,
        }
    );
}

#[test]
fn row_id_keeps_structural_and_process_targets_distinct() {
    let key = ProcessLiveKey::from_identity(ProcessIdentity {
        pid: 42,
        start_token: 100,
    })
    .expect("non-zero live key");
    let category = ProcessRowId::Category(ProcessCategory::Application);
    let application = ProcessRowId::Application(key);
    let process = ProcessRowId::Process(key);

    assert_eq!(category.live_key(), None);
    assert_eq!(application.live_key(), Some(key));
    assert!(!application.is_process());
    assert!(process.is_process());
    assert_ne!(application, process);
}

#[test]
fn projection_generation_is_separate_from_provider_identity() {
    let key = ProcessLiveKey::from_identity(ProcessIdentity {
        pid: 42,
        start_token: 100,
    })
    .expect("non-zero live key");
    let first = ProcessProjectionGeneration::INITIAL.next();
    let second = first.next();
    let anchor = ProcessRowAnchor::new(ProcessRowId::Process(key), first);

    assert_eq!(first.get(), 1);
    assert!(anchor.belongs_to(first));
    assert!(!anchor.belongs_to(second));
    assert_eq!(anchor.generation(), first);
    assert_eq!(anchor.id().live_key(), Some(key));
    assert_eq!(
        anchor.id().live_key().map(ProcessLiveKey::start_token),
        Some(100)
    );
}

#[test]
fn projection_generation_saturates_without_wrapping() {
    let max = ProcessProjectionGeneration::new(u64::MAX);
    assert_eq!(max.next(), max);
}

/// Container grouping keeps host processes out of fabricated buckets, keeps
/// `(runtime, id)` buckets separate, and preserves first-seen member order
/// with deterministic, de-duplicated live keys.
#[test]
fn container_group_rows_group_only_current_container_members() {
    use taskforest_core::core::process::{
        ContainerMetadata, ContainerRuntime, ProcessMetadataObservations,
    };

    let owner = || taskforest_core::core::process::ProcessOwner::opaque("tester");
    let docker = |id: &str, name: Option<&str>| {
        let mut metadata = ContainerMetadata::new(ContainerRuntime::Docker, id);
        if let Some(name) = name {
            metadata = metadata.with_name(name);
        }
        metadata
    };
    let attach = |item: &mut ProcessItem, metadata: ContainerMetadata| {
        item.apply_metadata_observations(ProcessMetadataObservations::current_with_container(
            owner(),
            None,
            Some(metadata),
            1,
        ));
    };
    let member = |pid: u32, name: &str| {
        ProcessItem::new(pid, name).with_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(u64::from(pid) + 100, 1),
            ..ProcessScalarObservations::default()
        })
    };

    let mut web1 = member(10, "nginx");
    attach(&mut web1, docker("a1b2c3d4e5f6", Some("web-prod")));
    let mut web2 = member(11, "nginx-worker");
    attach(&mut web2, docker("a1b2c3d4e5f6", Some("web-prod")));
    let mut db = member(12, "postgres");
    attach(&mut db, docker("9f8e7d6c5b4a", None));
    let host = member(13, "host-shell");

    let rows = container_group_rows(&[&web1, &web2, &db, &host], 1);
    assert_eq!(rows.len(), 2, "two container buckets; host is omitted");

    let web = &rows[0];
    assert_eq!(web.metadata().runtime, ContainerRuntime::Docker);
    assert_eq!(web.metadata().container_id, "a1b2c3d4e5f6");
    assert_eq!(web.metadata().name.as_deref(), Some("web-prod"));
    assert_eq!(web.member_count(), 2, "both members counted");
    assert_eq!(web.cpu().member_count(), 2, "CPU covers the bucket members");
    assert_eq!(
        web.cpu().current_member_count(),
        0,
        "the fixture carries no CPU observation, so coverage stays honest"
    );
    assert_eq!(
        web.memory().current_value(),
        None,
        "the fixture carries no memory observation; a container aggregate never invents one"
    );
    assert_eq!(
        web.member_live_keys().len(),
        2,
        "both members carry current live keys"
    );

    let db_row = &rows[1];
    assert_eq!(db_row.metadata().container_id, "9f8e7d6c5b4a");
    assert_eq!(db_row.metadata().name.as_deref(), None);

    // First-seen order is stable across repeated calls (no re-sort churn).
    let again = container_group_rows(&[&web1, &db, &web2, &host], 1);
    assert_eq!(again[0].metadata().container_id, "a1b2c3d4e5f6");
    assert_eq!(again[1].metadata().container_id, "9f8e7d6c5b4a");
}
