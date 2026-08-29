use std::sync::{Arc, Mutex};

use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityId, CapabilityScheduler, CapabilitySnapshot, EventEnvelope,
    EventPort, EventPortError, RequestEnvelope, RequestPort, SubmissionError, SubmissionErrorKind,
};

use crate::platform::{
    PlatformEvent, PlatformFacets, PlatformHandle, ProcessAffinityControlRequest,
    ProcessAffinityRequest, ProcessControlRequest, ProcessFacets, ProcessInsightFacetState,
    ProcessInsightUnavailable, ProcessNetworkRequest,
};
use taskmanager_core::core::process::FrozenProcessIdentity;

use super::PlatformClient;

#[derive(Default)]
struct EmptyCapabilities;

impl CapabilityCatalog for EmptyCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::default()
    }
}

#[derive(Default)]
struct EmptyEvents;

impl EventPort for EmptyEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

#[derive(Default)]
struct RecordingScheduler {
    planned: Mutex<Vec<CapabilityId>>,
    failed: Mutex<Vec<(CapabilityId, u64)>>,
    cadences: Mutex<Vec<(CapabilityId, Option<u64>)>>,
}

impl CapabilityScheduler for RecordingScheduler {
    fn poll_due(&self, _now_ms: u64) -> Vec<CapabilityId> {
        std::mem::take(&mut *self.planned.lock().expect("planned lock"))
    }

    fn mark_submission_failed(&self, capability: &CapabilityId, failed_at_ms: u64) {
        self.failed
            .lock()
            .expect("failed lock")
            .push((capability.clone(), failed_at_ms));
    }

    fn set_cadence_ms(&self, capability: &CapabilityId, cadence_ms: Option<u64>) {
        self.cadences
            .lock()
            .expect("cadence lock")
            .push((capability.clone(), cadence_ms));
    }

    fn request_recovery(
        &self,
        _capability: &CapabilityId,
        _trigger: taskmanager_platform_contract::CapabilityRecoveryTrigger,
    ) -> taskmanager_platform_contract::CapabilityRecoveryOutcome {
        taskmanager_platform_contract::CapabilityRecoveryOutcome::UnknownCapability
    }

    fn scheduling_snapshot(&self) -> taskmanager_platform_contract::RuntimeSchedulingSnapshot {
        taskmanager_platform_contract::RuntimeSchedulingSnapshot::default()
    }
}

#[derive(Default)]
struct AcceptingNetwork(Mutex<Vec<ProcessNetworkRequest>>);

impl RequestPort for AcceptingNetwork {
    type Request = ProcessNetworkRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        if let Ok(mut requests) = self.0.lock() {
            requests.push(request.payload);
        }
        Ok(())
    }
}

#[test]
fn partial_platform_does_not_implement_unsupported_facets() {
    let facets = PlatformFacets::default();
    let handle = PlatformHandle::new(Arc::new(EmptyCapabilities), Arc::new(EmptyEvents), facets);

    assert!(handle.facets().system().host().is_none());
    assert!(handle.facets().system().cpu().is_none());
    assert!(handle.facets().system().memory().is_none());
    assert!(handle.facets().system().storage().is_none());
    assert!(handle.facets().system().network().is_none());
    assert!(handle.facets().system().gpu().is_none());
    assert!(handle.facets().system().hardware_inventory().is_none());
    assert!(handle.facets().process().list().is_none());
    assert!(handle.facets().process().control().is_none());
    assert!(handle.facets().process().affinity().is_none());
    assert!(handle.facets().service().inventory().is_none());
    assert!(handle.facets().service().dependencies().is_none());
    assert!(handle.facets().service().control().is_none());
    assert!(handle.facets().service().log_snapshot().is_none());
    assert!(handle.facets().service().log_stream().is_none());
    assert!(handle.facets().environment().startup_inventory().is_none());
    assert!(handle.facets().environment().startup_evidence().is_none());
    assert!(handle.facets().environment().startup_control().is_none());
    assert!(handle.facets().environment().session_inventory().is_none());
    assert!(handle.facets().environment().session_control().is_none());
    assert!(handle.facets().integration().command_launch().is_none());
    assert!(handle.facets().integration().resource_reveal().is_none());
    assert!(handle.facets().integration().url_open().is_none());
    assert!(handle.facets().integration().desktop_appearance().is_none());
    assert!(handle.facets().storage().smart_observation().is_none());
    assert!(handle.facets().storage().smart_control().is_none());

    let mut client = PlatformClient::new(handle);
    let error = client
        .submit_hardware_inventory(7)
        .expect_err("missing hardware port");
    assert_eq!(error.capability, CapabilityId::HARDWARE_INVENTORY);
    assert_eq!(error.kind, SubmissionErrorKind::UnsupportedCapability);
}

#[test]
fn scheduled_capability_plan_is_consumed_by_the_typed_application_path() {
    let scheduler = Arc::new(RecordingScheduler {
        planned: Mutex::new(vec![CapabilityId::HARDWARE_INVENTORY]),
        ..Default::default()
    });
    let handle = PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default(),
    )
    .with_scheduler(scheduler.clone());
    let mut client = PlatformClient::new(handle);

    let outcomes = client.run_scheduled_refresh(42);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0]
            .as_ref()
            .map_err(|error| error.capability.clone()),
        Err(CapabilityId::HARDWARE_INVENTORY)
    );
    assert_eq!(
        scheduler.failed.lock().expect("failed lock").as_slice(),
        &[(CapabilityId::HARDWARE_INVENTORY, 42)]
    );
}

#[test]
fn automatic_schedule_lookup_round_trips_registry_policy_and_excludes_manual_work() {
    let schedules = super::automatic_schedules().collect::<Vec<_>>();
    assert!(
        !schedules.is_empty(),
        "automatic scheduling cannot be inert"
    );
    let minimum_ms = u64::try_from(crate::MIN_TELEMETRY_INTERVAL.as_millis())
        .expect("validated minimum fits milliseconds");
    let maximum_ms = u64::try_from(crate::MAX_TELEMETRY_INTERVAL.as_millis())
        .expect("validated maximum fits milliseconds");

    for schedule in schedules {
        assert_eq!(
            super::automatic_cadence_ms(&schedule.capability),
            Some(schedule.cadence_ms),
            "runtime route construction and application dispatch must consume one policy"
        );
        assert!(
            (minimum_ms..=maximum_ms).contains(&schedule.cadence_ms),
            "automatic work must use an accepted interactive cadence"
        );
    }
    assert_eq!(
        super::automatic_cadence_ms(&CapabilityId::PROCESS_CONTROL),
        None,
        "process control remains explicit user intent"
    );
}

#[test]
fn telemetry_interval_updates_every_system_route_through_the_scheduler_seam() {
    let scheduler = Arc::new(RecordingScheduler::default());
    let handle = PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default(),
    )
    .with_scheduler(scheduler.clone());
    let client = PlatformClient::new(handle);
    let interval = crate::TelemetryInterval::new(std::time::Duration::from_millis(250))
        .expect("fixture interval is valid");

    client.set_telemetry_interval(interval);

    let expected = crate::platform::SystemTelemetryDomain::ALL
        .into_iter()
        .map(|domain| (domain.capability(), Some(250)))
        .collect::<Vec<_>>();
    assert_eq!(
        *scheduler.cadences.lock().expect("cadence lock"),
        expected,
        "a user cadence must reach every independently scheduled system domain exactly once"
    );
}

#[test]
fn continuous_history_interval_keeps_system_and_application_samples_in_lockstep() {
    let scheduler = Arc::new(RecordingScheduler::default());
    let handle = PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default(),
    )
    .with_scheduler(scheduler.clone());
    let client = PlatformClient::new(handle);
    let interval = crate::TelemetryInterval::new(std::time::Duration::from_millis(750))
        .expect("fixture interval is valid");

    client.set_history_collection_interval(interval);

    let cadences = scheduler.cadences.lock().expect("cadence lock");
    assert_eq!(
        cadences.last(),
        Some(&(CapabilityId::PROCESS_LIST, Some(750)))
    );
    assert_eq!(
        cadences
            .iter()
            .filter(|(_, cadence)| *cadence == Some(750))
            .count(),
        crate::platform::SystemTelemetryDomain::ALL.len() + 1
    );
}

#[test]
fn continuous_history_profile_disables_unrelated_automatic_work() {
    let scheduler = Arc::new(RecordingScheduler::default());
    let handle = PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default(),
    )
    .with_scheduler(scheduler.clone());
    let client = PlatformClient::new(handle);

    client.apply_automatic_schedule_profile(crate::AutomaticScheduleProfile::ContinuousHistory);

    let cadences = scheduler.cadences.lock().expect("cadence lock");
    assert_eq!(cadences.len(), super::automatic_schedules().count());
    for (capability, cadence) in cadences.iter() {
        let history_fact = crate::platform::SystemTelemetryDomain::from_capability(capability)
            .is_some()
            || capability == &CapabilityId::PROCESS_LIST
            || capability == &CapabilityId::SENSORS
            || capability == &CapabilityId::POWER_SUPPLIES;
        assert_eq!(
            cadence.is_some(),
            history_fact,
            "only persisted-history producers remain scheduled: {capability}"
        );
    }
}

#[test]
fn process_insights_submission_keeps_accepted_siblings_and_projects_failures_immediately() {
    let network = Arc::new(AcceptingNetwork::default());
    let facets = PlatformFacets::default()
        .with_process(ProcessFacets::default().with_network(network.clone()));
    let handle = PlatformHandle::new(Arc::new(EmptyCapabilities), Arc::new(EmptyEvents), facets);
    let target = FrozenProcessIdentity::from_authoritative_parts(42, "worker", 7_500, 9_000)
        .expect("fixture identity");
    let mut client = PlatformClient::new(handle);

    let submission = client.submit_process_insights(target.clone(), 9);
    let submission = submission.expect("first process-insights revision");

    assert!(submission.network.is_ok());
    for failure in [
        &submission.gpu,
        &submission.resources,
        &submission.isolation,
    ] {
        assert_eq!(
            failure.as_ref().map_err(|error| error.kind),
            Err(SubmissionErrorKind::UnsupportedCapability)
        );
    }
    assert_eq!(client.process_insight_requests.len(), 1);
    assert!(matches!(
        submission.projection.network,
        ProcessInsightFacetState::Pending
    ));
    assert!(matches!(
        submission.projection.gpu,
        ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Submission(
            SubmissionErrorKind::UnsupportedCapability
        ))
    ));
    let requests = network.0.lock().expect("recorded network requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target, target);
    assert_eq!(requests[0].revision, submission.revision);
}

#[test]
fn process_insights_revision_state_is_bounded_and_exhaustion_is_typed() {
    let handle = PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default(),
    );
    let mut client = PlatformClient::new(handle);
    client.process_insights_revision = crate::ProcessInsightsRevision::new(u64::MAX);
    let target = FrozenProcessIdentity::from_authoritative_parts(42, "worker", 7_500, 9_000)
        .expect("fixture identity");

    assert_eq!(
        client.submit_process_insights(target, 9),
        Err(crate::ProcessInsightsSubmissionError::RevisionExhausted)
    );
    assert!(client.process_insight_requests.is_empty());
    assert!(client.process_insights_projection.current().is_none());
}

#[test]
fn schema_v1_identity_is_rejected_before_any_process_port_submission() {
    let legacy: FrozenProcessIdentity =
        serde_json::from_str(r#"{"pid":42,"name":"worker","start_time_secs":7500}"#)
            .expect("schema-v1 identity");
    let handle = PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default(),
    );
    let mut client = PlatformClient::new(handle);

    for error in [
        client
            .submit_process_control(ProcessControlRequest::EndTask(legacy.clone()), 1)
            .expect_err("legacy control must fail closed"),
        client
            .submit_process_control(
                ProcessControlRequest::Suspend {
                    target: legacy.clone(),
                },
                1,
            )
            .expect_err("legacy suspend must fail closed"),
        client
            .submit_process_control(
                ProcessControlRequest::Resume {
                    target: legacy.clone(),
                },
                1,
            )
            .expect_err("legacy resume must fail closed"),
        client
            .submit_process_affinity(
                ProcessAffinityRequest {
                    target: legacy.clone(),
                },
                1,
            )
            .expect_err("legacy affinity read must fail closed"),
        client
            .submit_process_affinity_control(
                ProcessAffinityControlRequest {
                    target: legacy.clone(),
                    cpus: vec![0],
                },
                1,
            )
            .expect_err("legacy affinity mutation must fail closed"),
    ] {
        assert_eq!(error.kind, SubmissionErrorKind::InvalidRequest);
    }
    assert_eq!(
        client.submit_process_insights(legacy, 1),
        Err(crate::ProcessInsightsSubmissionError::IdentityUnavailable)
    );
    assert!(client.process_insight_requests.is_empty());
}

#[test]
fn power_refresh_dispatches_exactly_one_power_supply_submission() {
    // A targeted battery refresh must route to a single power-supply submit,
    // not the bundled Health/All fan-out. With empty facets the one
    // submission fails closed as UnsupportedCapability; the *count* is the
    // invariant that distinguishes Power (1) from Health (4) / All (many).
    let handle = PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default(),
    );
    let mut client = PlatformClient::new(handle);

    let outcomes = client.request_refresh(crate::RefreshRequest::Power, 1);
    assert_eq!(
        outcomes.len(),
        1,
        "Power must dispatch exactly one power-supply submission",
    );
    let error = outcomes[0].as_ref().expect_err("empty facets fail closed");
    assert_eq!(error.capability, CapabilityId::POWER_SUPPLIES);
    assert_eq!(error.kind, SubmissionErrorKind::UnsupportedCapability);
}
