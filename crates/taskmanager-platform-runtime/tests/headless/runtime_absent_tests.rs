use taskmanager_application::{PlatformClient, SubmissionErrorKind};

use super::*;

#[test]
fn absent_handle_has_no_descriptors_ports_or_events() {
    let handle = capability_absent_handle();

    assert_eq!(handle.capabilities().snapshot().iter().count(), 0);
    assert!(handle.host_telemetry().is_none());
    assert!(handle.cpu_telemetry().is_none());
    assert!(handle.hardware_inventory().is_none());
    assert!(handle.process_list().is_none());
    assert!(handle.service_inventory().is_none());
    assert!(handle.storage_health().is_none());
    assert!(matches!(handle.events().try_recv(), Ok(None)));
}

#[test]
fn absent_handle_rejects_submission_as_unsupported() {
    let mut client = PlatformClient::new(capability_absent_handle());
    let submission = client
        .submit_system_telemetry(1)
        .expect("revision allocation succeeds without providers");
    assert!(!submission.has_pending_requests());
    for outcome in submission.into_request_results() {
        assert_eq!(
            outcome.map_err(|error| error.kind),
            Err(SubmissionErrorKind::UnsupportedCapability)
        );
    }
}
