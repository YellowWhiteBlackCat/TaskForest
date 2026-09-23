//! Startup-entry and session control lane isolation contracts.

use super::*;
use taskmanager_application::SessionEvent;
use taskmanager_application::StartupEvent;
use taskmanager_application::StartupEvidenceEvent;
use taskmanager_core::StartupControlPolicy;
use taskmanager_core::StartupImpact;
use taskmanager_core::StartupImpactEvidence;
use taskmanager_core::StartupImpactUnknownReason;
use taskmanager_core::StartupScope;
use taskmanager_core::StartupSource;

#[test]
fn environment_catalog_keeps_five_distinct_registration_identities() {
    let handle = spawn_complete(fake_registry(FakeProvider::default()));
    let capabilities = handle.capabilities().snapshot();

    for capability in [
        CapabilityId::STARTUP,
        CapabilityId::STARTUP_EVIDENCE,
        CapabilityId::STARTUP_CONTROL,
        CapabilityId::SESSIONS,
        CapabilityId::SESSION_CONTROL,
    ] {
        assert_eq!(
            capabilities
                .get(&capability)
                .map(|descriptor| descriptor.providers.clone()),
            Some(vec![fixture_environment_provider(&capability)])
        );
    }
}

#[test]
fn startup_evidence_has_an_independent_clocked_provider_chain() {
    let observed_times = Arc::new(Mutex::new(Vec::new()));
    let handle = spawn_complete(fake_registry(FakeProvider {
        startup_evidence_times: observed_times.clone(),
        ..Default::default()
    }));
    let mut request_ids = RequestIdGenerator::default();
    let request_id = request_ids.next_id();

    handle
        .startup_evidence()
        .expect("startup evidence facet")
        .try_submit(RequestEnvelope {
            id: request_id,
            capability: CapabilityId::STARTUP_EVIDENCE,
            submitted_at_ms: 1,
            payload: StartupEvidenceRequest::Refresh,
        })
        .expect("startup evidence accepted");

    let event = wait_event(&handle);
    assert_eq!(event.request_id, request_id);
    assert_eq!(event.capability, CapabilityId::STARTUP_EVIDENCE);
    assert_eq!(
        event.provider,
        Some(fixture_environment_provider(&event.capability))
    );
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::StartupEvidence(
            StartupEvidenceEvent::Snapshot(_)
        ))
    ));
    let times = observed_times.lock().expect("startup evidence times");
    assert_eq!(times.len(), 1);
    assert!(times[0] > 0);
}

#[test]
fn startup_and_session_controls_are_non_blocking_and_correlated() {
    let startup_controls = Arc::new(Mutex::new(Vec::new()));
    let session_controls = Arc::new(Mutex::new(Vec::new()));
    let handle = spawn_complete(fake_registry(FakeProvider {
        delay: Duration::from_millis(80),
        startup_controls: startup_controls.clone(),
        session_controls: session_controls.clone(),
        ..Default::default()
    }));
    let mut control_ids = LatestControlRequest::default();
    let startup_id = control_ids.begin();
    let session_id = control_ids.begin();
    let mut request_ids = RequestIdGenerator::default();

    let started = Instant::now();
    handle
        .startup_control()
        .expect("startup control facet")
        .try_submit(RequestEnvelope {
            id: request_ids.next_id(),
            capability: CapabilityId::STARTUP_CONTROL,
            submitted_at_ms: 1,
            payload: StartupControlRequest {
                request_id: startup_id,
                entry: StartupEntry {
                    id: "desktop:demo.desktop".into(),
                    name: "demo".into(),
                    exec: "demo".into(),
                    enabled: false,
                    source: StartupSource::DesktopEntry,
                    scope: StartupScope::User,
                    control_policy: StartupControlPolicy::Direct,
                    locator: "/tmp/demo.desktop".into(),
                    impact: StartupImpact::None,
                    impact_evidence: StartupImpactEvidence::Unknown {
                        reason: StartupImpactUnknownReason::NotInstrumented,
                    },
                },
                enabled: true,
            },
        })
        .expect("startup control accepted");
    handle
        .session_control()
        .expect("session control facet")
        .try_submit(RequestEnvelope {
            id: request_ids.next_id(),
            capability: CapabilityId::SESSION_CONTROL,
            submitted_at_ms: 1,
            payload: SessionControlRequest {
                request_id: session_id,
                session_id: "7".into(),
                action: SessionControlAction::Lock,
            },
        })
        .expect("session control accepted");
    assert!(
        started.elapsed() < Duration::from_millis(50),
        "control facets blocked for {:?}",
        started.elapsed()
    );

    let events = [wait_event(&handle), wait_event(&handle)];
    for event in &events {
        assert_eq!(
            event.provider,
            Some(fixture_environment_provider(&event.capability))
        );
    }
    assert!(events.iter().any(|event| matches!(
        event.outcome,
        Ok(PlatformEvent::Startup(
            StartupEvent::Control(ref outcome)
        )) if outcome.request_id == startup_id && outcome.result.is_ok()
    )));
    assert!(events.iter().any(|event| matches!(
        event.outcome,
        Ok(PlatformEvent::Sessions(
            SessionEvent::Control(ref outcome)
        )) if outcome.request_id == session_id
            && outcome.result == Err(FailureKind::PermissionDenied)
    )));
    assert_eq!(
        startup_controls
            .lock()
            .expect("startup controls")
            .as_slice(),
        &[("demo".into(), true)]
    );
    assert_eq!(
        session_controls
            .lock()
            .expect("session controls")
            .as_slice(),
        &[("7".into(), SessionControlAction::Lock)]
    );
}
