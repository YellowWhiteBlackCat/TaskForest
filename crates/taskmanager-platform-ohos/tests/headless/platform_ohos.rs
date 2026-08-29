use taskmanager_application::PlatformClient;
use taskmanager_platform_contract::SubmissionErrorKind;

use taskmanager_platform_ohos::OhosPlatformRuntime;

#[test]
fn empty_runtime_has_no_capabilities_or_events() {
    let handle = OhosPlatformRuntime::spawn().expect("empty runtime is constructible");

    assert_eq!(handle.capabilities().snapshot().iter().count(), 0);
    assert!(handle.host_telemetry().is_none());
    assert!(handle.process_list().is_none());
    assert!(
        handle
            .events()
            .try_recv()
            .expect("idle event port is readable")
            .is_none()
    );
}

#[test]
fn empty_runtime_rejects_requests_as_unsupported() {
    let mut client =
        PlatformClient::new(OhosPlatformRuntime::spawn().expect("empty runtime is constructible"));
    let submission = client
        .submit_system_telemetry(1)
        .expect("revision allocation does not require a provider");

    let outcomes = submission.into_request_results();
    assert!(!outcomes.is_empty());
    assert!(outcomes.into_iter().all(|outcome| {
        outcome.map_err(|error| error.kind) == Err(SubmissionErrorKind::UnsupportedCapability)
    }));
}
