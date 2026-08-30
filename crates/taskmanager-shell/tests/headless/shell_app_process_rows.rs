use super::*;

use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{ProcessCategory, ProcessItem, ProcessScalarObservations};
use taskmanager_core::core::process_telemetry::ProcessIdentity;

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
