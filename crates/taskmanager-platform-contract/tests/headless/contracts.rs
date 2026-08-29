use taskmanager_core::{FailureKind, ProviderId};
use taskmanager_platform_contract::{
    CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    MAX_REQUEST_SCOPE_BYTES, ProviderFailure, RequestIdGenerator, RequestScope,
    RequestTrackingError, RetryDisposition,
};

#[test]
fn request_ids_are_non_zero_across_wrap() {
    let mut ids = RequestIdGenerator::default();
    assert_eq!(ids.next_id().get(), 1);
}

#[test]
fn request_scope_enforces_its_utf8_byte_boundary_without_truncation() {
    let boundary = "a".repeat(MAX_REQUEST_SCOPE_BYTES);
    let accepted = RequestScope::try_owned(boundary.clone()).expect("exact byte boundary");
    assert_eq!(accepted.as_str(), boundary);

    let oversized = "a".repeat(MAX_REQUEST_SCOPE_BYTES + 1);
    assert_eq!(
        RequestScope::try_owned(oversized),
        Err(RequestTrackingError::TargetScopeTooLong {
            actual_bytes: MAX_REQUEST_SCOPE_BYTES + 1,
            max_bytes: MAX_REQUEST_SCOPE_BYTES,
        })
    );
    assert_eq!(
        RequestScope::try_from_str(""),
        Err(RequestTrackingError::EmptyTargetScope)
    );
}

#[test]
fn request_scope_counts_unicode_as_utf8_bytes() {
    let accepted_text = "界".repeat(MAX_REQUEST_SCOPE_BYTES / "界".len());
    assert!(accepted_text.len() <= MAX_REQUEST_SCOPE_BYTES);
    assert!(RequestScope::try_from_str(&accepted_text).is_ok());

    let oversized_text = format!("{accepted_text}界");
    assert!(oversized_text.chars().count() < MAX_REQUEST_SCOPE_BYTES);
    assert_eq!(
        RequestScope::try_from_str(&oversized_text),
        Err(RequestTrackingError::TargetScopeTooLong {
            actual_bytes: oversized_text.len(),
            max_bytes: MAX_REQUEST_SCOPE_BYTES,
        })
    );
}

#[test]
fn provider_failures_have_stable_operation_failure_mapping() {
    assert_eq!(
        ProviderFailure::MissingDependency.kind(),
        FailureKind::MissingDependency
    );
    assert_eq!(
        ProviderFailure::MissingDependency.retry(),
        RetryDisposition::AfterCapabilityChange
    );
    assert_eq!(
        ProviderFailure::IdentityChanged.retry(),
        RetryDisposition::Never
    );
    assert_eq!(
        ProviderFailure::TimedOut.retry(),
        RetryDisposition::RetryLater
    );
    for failure in [
        FailureKind::Unsupported,
        FailureKind::RequiresEscalation,
        FailureKind::PermissionDenied,
        FailureKind::MissingDependency,
        FailureKind::TimedOut,
        FailureKind::IdentityChanged,
        FailureKind::TemporarilyUnavailable,
        FailureKind::Rejected,
        FailureKind::ProviderFault,
    ] {
        assert_eq!(ProviderFailure::from_kind(failure).kind(), failure);
    }
}

#[test]
fn capability_snapshot_is_deterministic_and_extensible() {
    let snapshot = CapabilitySnapshot::from_descriptors([
        CapabilityDescriptor {
            id: CapabilityId::PROCESS_LIST,
            status: CapabilityStatus::Available,
            providers: vec![ProviderId::borrowed("test.process")],
            observed_at_ms: 50,
            last_success_at_ms: Some(50),
        },
        CapabilityDescriptor {
            id: CapabilityId::owned("future.capability"),
            status: CapabilityStatus::Unsupported,
            providers: Vec::new(),
            observed_at_ms: 50,
            last_success_at_ms: None,
        },
    ]);

    assert_eq!(
        snapshot
            .iter()
            .map(|descriptor| descriptor.id.as_str())
            .collect::<Vec<_>>(),
        ["future.capability", "process.list"]
    );
}

#[test]
fn system_telemetry_domains_have_stable_independent_capability_ids() {
    let domains = [
        CapabilityId::TELEMETRY_HOST,
        CapabilityId::TELEMETRY_CPU,
        CapabilityId::TELEMETRY_MEMORY,
        CapabilityId::TELEMETRY_STORAGE,
        CapabilityId::TELEMETRY_NETWORK,
        CapabilityId::TELEMETRY_GPU,
    ];

    assert_eq!(
        domains.iter().map(CapabilityId::as_str).collect::<Vec<_>>(),
        [
            "telemetry.host",
            "telemetry.cpu",
            "telemetry.memory",
            "telemetry.storage",
            "telemetry.network",
            "telemetry.gpu",
        ]
    );
}
