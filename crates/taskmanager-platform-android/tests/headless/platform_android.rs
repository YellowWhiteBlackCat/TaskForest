use taskmanager_application::{PlatformClient, SubmissionErrorKind};

use taskmanager_platform_android::{AndroidPlatformRuntime, provider_feature_enabled};

#[test]
fn feature_marker_matches_the_build_configuration() {
    assert_eq!(
        provider_feature_enabled(),
        cfg!(feature = "android-provider")
    );
}

#[test]
fn empty_runtime_has_no_capabilities_or_events() {
    let handle = AndroidPlatformRuntime::spawn().expect("empty runtime is constructible");

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
    let mut client = PlatformClient::new(
        AndroidPlatformRuntime::spawn().expect("empty runtime is constructible"),
    );
    let submission = client
        .submit_system_telemetry(1)
        .expect("revision allocation does not require a provider");

    let outcomes = submission.into_request_results();
    assert!(!outcomes.is_empty());
    assert!(outcomes.into_iter().all(|outcome| {
        outcome.map_err(|error| error.kind) == Err(SubmissionErrorKind::UnsupportedCapability)
    }));
}
