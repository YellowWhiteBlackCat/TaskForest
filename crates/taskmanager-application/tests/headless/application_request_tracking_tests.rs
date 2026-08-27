use taskmanager_platform_contract::{
    CapabilityRequest, RequestScope, RequestTracking, RequestTrackingError,
};

use super::*;
use crate::{
    FrozenProcessIdentity, LatestControlRequest, ProcessBatchAction, ProcessBatchIntent,
    ProcessGroupScope, ProcessSignal, ResourceGroupLimitRequest, ServiceAction, ServiceId,
    ServiceLogLevelFilter, ServiceLogQuery, ServiceLogTimeFilter, SessionControlAction,
    SessionControlRequest, SessionId, StartupControlPolicy, StartupControlRequest, StartupEntry,
    StartupEntryId, StartupEntryLocator, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource,
};

fn target_scope(request: &impl CapabilityRequest) -> RequestScope {
    match request
        .runtime_tracking()
        .expect("authoritative target fixture")
    {
        RequestTracking::Target(scope) => scope,
        tracking => panic!("expected target lifecycle, got {tracking:?}"),
    }
}

fn process(pid: u32, name: &str, wall_clock_start: u64, start_token: u64) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, name, wall_clock_start, start_token)
        .expect("authoritative process fixture")
}

#[test]
fn single_process_work_tracks_pid_and_authoritative_start_token() {
    let first = process(41, "first display name", 100, 700);
    let renamed = process(41, "renamed display value", 999, 700);
    let reused_pid = process(41, "new process", 100, 701);
    let other_pid = process(42, "first display name", 100, 700);

    let expected = target_scope(&ProcessControlRequest::EndTask(first.clone()));
    assert_eq!(
        expected,
        target_scope(&ProcessControlRequest::EndTask(renamed))
    );
    assert_ne!(
        expected,
        target_scope(&ProcessControlRequest::EndTask(reused_pid))
    );
    assert_ne!(
        expected,
        target_scope(&ProcessControlRequest::EndTask(other_pid))
    );

    let single_target_controls = [
        ProcessControlRequest::SendSignal {
            target: first.clone(),
            signal: ProcessSignal::Terminate,
        },
        ProcessControlRequest::Suspend {
            target: first.clone(),
        },
        ProcessControlRequest::Resume {
            target: first.clone(),
        },
    ];
    for request in single_target_controls {
        assert_eq!(target_scope(&request), expected);
    }

    assert_eq!(
        target_scope(&ProcessAffinityRequest {
            target: first.clone(),
        }),
        expected
    );
    assert_eq!(
        target_scope(&ProcessAffinityControlRequest {
            target: first.clone(),
            cpus: vec![0, 2],
        }),
        expected
    );
    assert_eq!(
        target_scope(&ProcessResourceControlRequest {
            target: first.clone(),
            limits: ResourceGroupLimitRequest::default(),
        }),
        expected
    );
    assert_eq!(
        target_scope(&ResourceRevealRequest {
            target: first,
            cached_executable: Some("/provider/cached/path".into()),
        }),
        expected,
        "desktop reveal is still target-scoped process work"
    );
}

#[test]
fn process_batch_keeps_one_transaction_lifecycle_even_for_one_target() {
    let request = ProcessControlRequest::ExecuteBatch(ProcessBatchIntent {
        action: ProcessBatchAction::End,
        scope: ProcessGroupScope::PidAdjacency,
        targets: vec![process(41, "display", 100, 700)],
    });

    assert_eq!(request.runtime_tracking(), Ok(RequestTracking::Capability));
}

fn process_insight_tracking(
    target: &FrozenProcessIdentity,
) -> [Result<RequestTracking, RequestTrackingError>; 7] {
    let revision = ProcessInsightsRevision::new(9);
    [
        ProcessNetworkRequest {
            target: target.clone(),
            revision,
        }
        .runtime_tracking(),
        ProcessGpuRequest {
            target: target.clone(),
            revision,
        }
        .runtime_tracking(),
        ProcessResourcesRequest {
            target: target.clone(),
            revision,
        }
        .runtime_tracking(),
        ProcessIsolationRequest {
            target: target.clone(),
            revision,
        }
        .runtime_tracking(),
        ProcessThreadsRequest {
            target: target.clone(),
            revision,
        }
        .runtime_tracking(),
        ProcessOpenFilesRequest {
            target: target.clone(),
            revision,
        }
        .runtime_tracking(),
        ProcessEnvironmentRequest {
            target: target.clone(),
            revision,
        }
        .runtime_tracking(),
    ]
}

#[test]
fn every_process_insight_facet_tracks_the_same_exact_process_generation() {
    let selected = process(70, "selected", 100, 8_000);
    let replacement = process(71, "replacement", 100, 8_000);
    let expected = target_scope(&ProcessControlRequest::EndTask(selected.clone()));

    for tracking in process_insight_tracking(&selected) {
        assert_eq!(tracking, Ok(RequestTracking::Target(expected.clone())));
    }
    assert_ne!(
        process_insight_tracking(&replacement)[0],
        Ok(RequestTracking::Target(expected))
    );

    let legacy: FrozenProcessIdentity = serde_json::from_value(serde_json::json!({
        "pid": 70,
        "name": "legacy",
        "start_time_secs": 100
    }))
    .expect("schema-v1 process identity fixture");
    for tracking in process_insight_tracking(&legacy) {
        assert_eq!(tracking, Err(RequestTrackingError::MissingTargetIdentity));
    }
}

fn service_log_query(service_id: ServiceId) -> ServiceLogQuery {
    ServiceLogQuery {
        service_id,
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    }
}

#[test]
fn service_facets_share_the_provider_issued_service_identity() {
    let service_id = ServiceId::new("provider:demo.service");
    let other_id = ServiceId::new("provider:other.service");
    let mut request_ids = LatestControlRequest::default();
    let expected = target_scope(&ServiceDependenciesRequest {
        service_id: service_id.clone(),
    });

    assert_eq!(
        target_scope(&ServiceControlRequest {
            request_id: request_ids.begin(),
            service_id: service_id.clone(),
            action: ServiceAction::Restart,
        }),
        expected
    );
    assert_eq!(
        target_scope(&ServiceLogSnapshotRequest {
            service_id: service_id.clone(),
        }),
        expected
    );
    assert_eq!(
        target_scope(&ServiceLogStreamRequest {
            query: service_log_query(service_id),
        }),
        expected
    );
    assert_ne!(
        target_scope(&ServiceDependenciesRequest {
            service_id: other_id,
        }),
        expected
    );
}

fn startup_entry(id: &str, name: &str, locator: &str) -> StartupEntry {
    StartupEntry {
        id: StartupEntryId::new(id),
        name: name.into(),
        exec: "fixture".into(),
        enabled: true,
        source: StartupSource::UserService,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: StartupEntryLocator::new(locator),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    }
}

#[test]
fn environment_controls_ignore_display_and_native_locator_when_scoping_targets() {
    let mut request_ids = LatestControlRequest::default();
    let startup = target_scope(&StartupControlRequest {
        request_id: request_ids.begin(),
        entry: startup_entry("startup:stable", "display one", "native/one"),
        enabled: false,
    });
    assert_eq!(
        target_scope(&StartupControlRequest {
            request_id: request_ids.begin(),
            entry: startup_entry("startup:stable", "display two", "native/two"),
            enabled: true,
        }),
        startup
    );
    assert_ne!(
        target_scope(&StartupControlRequest {
            request_id: request_ids.begin(),
            entry: startup_entry("startup:replacement", "display one", "native/one"),
            enabled: false,
        }),
        startup
    );

    let session = target_scope(&SessionControlRequest {
        request_id: request_ids.begin(),
        session_id: SessionId::new("session:stable"),
        action: SessionControlAction::Disconnect,
    });
    assert_eq!(
        target_scope(&SessionControlRequest {
            request_id: request_ids.begin(),
            session_id: SessionId::new("session:stable"),
            action: SessionControlAction::Lock,
        }),
        session
    );
    assert_ne!(
        target_scope(&SessionControlRequest {
            request_id: request_ids.begin(),
            session_id: SessionId::new("session:replacement"),
            action: SessionControlAction::Disconnect,
        }),
        session
    );
}

#[test]
fn missing_opaque_identity_is_rejected_instead_of_minting_an_empty_target() {
    let mut request_ids = LatestControlRequest::default();
    assert_eq!(
        ServiceDependenciesRequest {
            service_id: ServiceId::default(),
        }
        .runtime_tracking(),
        Err(taskmanager_platform_contract::RequestTrackingError::EmptyTargetScope)
    );
    assert_eq!(
        StartupControlRequest {
            request_id: request_ids.begin(),
            entry: startup_entry("", "legacy display", "native/locator"),
            enabled: false,
        }
        .runtime_tracking(),
        Err(taskmanager_platform_contract::RequestTrackingError::EmptyTargetScope)
    );
    assert_eq!(
        SessionControlRequest {
            request_id: request_ids.begin(),
            session_id: SessionId::default(),
            action: SessionControlAction::Disconnect,
        }
        .runtime_tracking(),
        Err(taskmanager_platform_contract::RequestTrackingError::EmptyTargetScope)
    );
}
