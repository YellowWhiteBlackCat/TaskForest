use super::*;

#[test]
fn slow_smart_control_does_not_block_status_observation() {
    let handle = spawn_complete(fake_registry(FakeProvider {
        smart_control_delay: Duration::from_millis(150),
        ..FakeProvider::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let control_id = ids.next_id();
    handle
        .smart_control()
        .expect("SMART control facet")
        .try_submit(RequestEnvelope {
            id: control_id,
            capability: CapabilityId::SMART_CONTROL,
            submitted_at_ms: 1,
            payload: SmartControlRequest::StartSelfTest(SmartSelfTestIntent {
                device_id: DeviceId::new("disk:slow-control"),
                device_generation: DeviceGeneration::INITIAL,
                device_key: "slow-control".into(),
                display_name: "Slow control".into(),
                kind: taskmanager_core::SmartSelfTestKind::Short,
            }),
        })
        .expect("SMART control accepted");

    let observation_id = ids.next_id();
    handle
        .smart_observation()
        .expect("SMART observation facet")
        .try_submit(RequestEnvelope {
            id: observation_id,
            capability: CapabilityId::SMART,
            submitted_at_ms: 2,
            payload: SmartObservationRequest::RefreshAll,
        })
        .expect("SMART observation accepted");

    let observation = wait_event(&handle);
    assert_eq!(observation.request_id, observation_id);
    assert_eq!(observation.capability, CapabilityId::SMART);
    assert_eq!(
        observation.provider,
        Some(ProviderId::borrowed("fixture.storage.smart-observation"))
    );
    assert!(matches!(
        observation.outcome,
        Ok(PlatformEvent::Smart(SmartEvent::Batch(ref batch)))
            if batch.observations.is_empty()
                && batch.issues.is_empty()
                && batch.ended.is_empty()
    ));
    let control = wait_event(&handle);
    assert_eq!(control.request_id, control_id);
    assert_eq!(control.capability, CapabilityId::SMART_CONTROL);
    assert_eq!(
        control.provider,
        Some(ProviderId::borrowed("fixture.storage.smart-control"))
    );
}

#[test]
fn concurrent_smart_poll_keeps_new_job_on_another_target() {
    let refresh_started = Arc::new(AtomicBool::new(false));
    let handle = spawn_complete(fake_registry(FakeProvider {
        smart_refresh_delay: Duration::from_millis(150),
        smart_refresh_started: refresh_started.clone(),
        ..FakeProvider::default()
    }));
    let mut ids = RequestIdGenerator::default();

    let old_id = ids.next_id();
    handle
        .smart_control()
        .expect("SMART control facet")
        .try_submit(RequestEnvelope {
            id: old_id,
            capability: CapabilityId::SMART_CONTROL,
            submitted_at_ms: 1,
            payload: SmartControlRequest::StartSelfTest(SmartSelfTestIntent {
                device_id: DeviceId::new("disk:old"),
                device_generation: DeviceGeneration::INITIAL,
                device_key: "old".into(),
                display_name: "Old".into(),
                kind: taskmanager_core::SmartSelfTestKind::Short,
            }),
        })
        .expect("old SMART control accepted");
    assert_eq!(wait_event(&handle).request_id, old_id);

    let poll_id = ids.next_id();
    handle
        .smart_observation()
        .expect("SMART observation facet")
        .try_submit(RequestEnvelope {
            id: poll_id,
            capability: CapabilityId::SMART,
            submitted_at_ms: 2,
            payload: SmartObservationRequest::RefreshAll,
        })
        .expect("SMART poll accepted");
    for _ in 0..100 {
        if refresh_started.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        refresh_started.load(Ordering::Acquire),
        "SMART observation provider did not start"
    );

    let new_id = ids.next_id();
    handle
        .smart_control()
        .expect("SMART control facet")
        .try_submit(RequestEnvelope {
            id: new_id,
            capability: CapabilityId::SMART_CONTROL,
            submitted_at_ms: 3,
            payload: SmartControlRequest::StartSelfTest(SmartSelfTestIntent {
                device_id: DeviceId::new("disk:new"),
                device_generation: DeviceGeneration::INITIAL,
                device_key: "new".into(),
                display_name: "New".into(),
                kind: taskmanager_core::SmartSelfTestKind::Extended,
            }),
        })
        .expect("new SMART control accepted");

    let control = wait_event(&handle);
    assert_eq!(control.request_id, new_id);
    let stale_poll = wait_event(&handle);
    assert_eq!(stale_poll.request_id, poll_id);
    assert!(matches!(
        stale_poll.outcome,
        Ok(PlatformEvent::Smart(SmartEvent::Batch(ref batch)))
            if batch.observations.len() == 2
                && batch.observations.iter().any(|observation|
                    observation.device_key.as_str() == "old")
                && batch.observations.iter().any(|observation|
                    observation.device_key.as_str() == "new")
    ));
}

#[test]
fn stop_tracking_invalidates_an_inflight_poll_without_claiming_drive_abort() {
    let refresh_started = Arc::new(AtomicBool::new(false));
    let refresh_targets = Arc::new(Mutex::new(Vec::new()));
    let handle = spawn_complete(fake_registry(FakeProvider {
        smart_refresh_delay: Duration::from_millis(150),
        smart_refresh_started: refresh_started.clone(),
        smart_refresh_targets: refresh_targets.clone(),
        ..FakeProvider::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let intent = SmartSelfTestIntent {
        device_id: DeviceId::new("disk:cancel"),
        device_generation: DeviceGeneration::new(4),
        device_key: "cancel".into(),
        display_name: "Cancel tracking".into(),
        kind: taskmanager_core::SmartSelfTestKind::Short,
    };
    let target = intent.target();
    let start_id = ids.next_id();
    handle
        .smart_control()
        .expect("SMART control facet")
        .try_submit(RequestEnvelope {
            id: start_id,
            capability: CapabilityId::SMART_CONTROL,
            submitted_at_ms: 1,
            payload: SmartControlRequest::StartSelfTest(intent),
        })
        .expect("SMART start accepted");
    assert_eq!(wait_event(&handle).request_id, start_id);

    let poll_id = ids.next_id();
    handle
        .smart_observation()
        .expect("SMART observation facet")
        .try_submit(RequestEnvelope {
            id: poll_id,
            capability: CapabilityId::SMART,
            submitted_at_ms: 2,
            payload: SmartObservationRequest::RefreshTarget(target.clone()),
        })
        .expect("targeted SMART poll accepted");
    for _ in 0..100 {
        if refresh_started.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(refresh_started.load(Ordering::Acquire));

    let stop_id = ids.next_id();
    handle
        .smart_control()
        .expect("SMART control facet")
        .try_submit(RequestEnvelope {
            id: stop_id,
            capability: CapabilityId::SMART_CONTROL,
            submitted_at_ms: 3,
            payload: SmartControlRequest::StopTracking(target.clone()),
        })
        .expect("stop tracking accepted");

    let stopped = wait_event(&handle);
    assert_eq!(stopped.request_id, stop_id);
    let stopped_revision =
        match stopped.outcome {
            Ok(PlatformEvent::Smart(SmartEvent::Batch(batch))) => {
                assert!(batch.observations.is_empty());
                assert!(batch.ended.iter().any(|ended| ended.target == target
                    && ended.reason == SmartTrackingEndReason::Requested));
                batch.revision
            }
            outcome => panic!("unexpected stop-tracking outcome: {outcome:?}"),
        };
    let stale_poll = wait_event(&handle);
    assert_eq!(stale_poll.request_id, poll_id);
    let stale_revision = match stale_poll.outcome {
        Ok(PlatformEvent::Smart(SmartEvent::Batch(batch))) => {
            assert!(batch.observations.is_empty());
            assert!(batch.issues.is_empty());
            batch.revision
        }
        outcome => panic!("unexpected stale-poll outcome: {outcome:?}"),
    };
    assert_eq!(
        stale_revision, stopped_revision,
        "late poll must carry the post-cancel projection revision"
    );
    assert_eq!(
        refresh_targets
            .lock()
            .expect("SMART refresh targets")
            .as_slice(),
        &[target]
    );
}

#[test]
fn target_timeout_is_degraded_and_identity_change_removes_only_that_disk() {
    let refresh_errors = Arc::new(Mutex::new(vec![(
        "disk-a".to_string(),
        ProviderFailure::TimedOut,
    )]));
    let handle = spawn_complete(fake_registry(FakeProvider {
        smart_refresh_errors: refresh_errors.clone(),
        ..FakeProvider::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let intents = ["disk-a", "disk-b"].map(|device| SmartSelfTestIntent {
        device_id: DeviceId::new(format!("disk:{device}")),
        device_generation: DeviceGeneration::INITIAL,
        device_key: device.into(),
        display_name: device.into(),
        kind: taskmanager_core::SmartSelfTestKind::Short,
    });
    for intent in intents.clone() {
        let request_id = ids.next_id();
        handle
            .smart_control()
            .expect("SMART control facet")
            .try_submit(RequestEnvelope {
                id: request_id,
                capability: CapabilityId::SMART_CONTROL,
                submitted_at_ms: 1,
                payload: SmartControlRequest::StartSelfTest(intent),
            })
            .expect("SMART start accepted");
        assert_eq!(wait_event(&handle).request_id, request_id);
    }

    let target_a = intents[0].target();
    let timeout_id = ids.next_id();
    handle
        .smart_observation()
        .expect("SMART observation facet")
        .try_submit(RequestEnvelope {
            id: timeout_id,
            capability: CapabilityId::SMART,
            submitted_at_ms: 2,
            payload: SmartObservationRequest::RefreshAll,
        })
        .expect("timed-out target poll accepted");
    let timeout = wait_event(&handle);
    assert_eq!(timeout.request_id, timeout_id);
    assert!(matches!(
        timeout.outcome,
        Ok(PlatformEvent::Smart(SmartEvent::Batch(ref batch)))
            if batch.observations.len() == 2
                && batch.issues.iter().any(|issue|
                    issue.target == target_a && issue.failure == FailureKind::TimedOut)
    ));
    assert_eq!(
        handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::SMART)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::Degraded(FailureKind::TimedOut))
    );

    *refresh_errors.lock().expect("SMART error plan") =
        vec![("disk-a".to_string(), ProviderFailure::IdentityChanged)];
    let identity_id = ids.next_id();
    handle
        .smart_observation()
        .expect("SMART observation facet")
        .try_submit(RequestEnvelope {
            id: identity_id,
            capability: CapabilityId::SMART,
            submitted_at_ms: 3,
            payload: SmartObservationRequest::RefreshTarget(target_a.clone()),
        })
        .expect("identity-change poll accepted");
    let identity = wait_event(&handle);
    assert_eq!(identity.request_id, identity_id);
    assert!(matches!(
        identity.outcome,
        Ok(PlatformEvent::Smart(SmartEvent::Batch(ref batch)))
            if batch.observations.len() == 1
                && batch.observations[0].device_key.as_str() == "disk-b"
                && batch.ended.iter().any(|ended|
                    ended.target == target_a
                        && ended.reason == SmartTrackingEndReason::IdentityChanged)
                && batch.issues.iter().any(|issue|
                    issue.target == target_a
                        && issue.failure == FailureKind::IdentityChanged)
    ));
}

#[test]
fn typed_smart_report_failures_do_not_become_outer_success_health() {
    let missing_tool = SmartSelfTestReport {
        state: DeviceState {
            status: taskmanager_core::DeviceStatus::MissingTool,
            last_success_ms: None,
        },
        failure: Some(taskmanager_core::SmartSelfTestFailure::MissingTool),
        ..SmartSelfTestReport::default()
    };
    let control_handle = spawn_complete(fake_registry(FakeProvider {
        smart_control_report: Some(missing_tool),
        ..FakeProvider::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let control_id = ids.next_id();
    control_handle
        .smart_control()
        .expect("SMART control facet")
        .try_submit(RequestEnvelope {
            id: control_id,
            capability: CapabilityId::SMART_CONTROL,
            submitted_at_ms: 1,
            payload: SmartControlRequest::StartSelfTest(SmartSelfTestIntent {
                device_id: DeviceId::new("disk:missing-tool"),
                device_generation: DeviceGeneration::INITIAL,
                device_key: "missing-tool".into(),
                display_name: "Missing tool".into(),
                kind: taskmanager_core::SmartSelfTestKind::Short,
            }),
        })
        .expect("SMART control accepted");
    let control = wait_event(&control_handle);
    assert_eq!(control.request_id, control_id);
    assert!(matches!(
        control.outcome,
        Ok(PlatformEvent::Smart(SmartEvent::Batch(ref batch)))
            if batch.issues.iter().any(|issue|
                issue.failure == FailureKind::MissingDependency)
                && batch.observations.iter().any(|observation|
                    observation.report.failure
                        == Some(taskmanager_core::SmartSelfTestFailure::MissingTool))
    ));
    let control_capability = control_handle
        .capabilities()
        .snapshot()
        .get(&CapabilityId::SMART_CONTROL)
        .cloned()
        .expect("SMART control capability");
    assert_eq!(
        control_capability.status,
        CapabilityStatus::MissingDependency
    );
    assert_eq!(control_capability.last_success_at_ms, None);

    let permission_report = SmartSelfTestReport {
        state: DeviceState {
            status: taskmanager_core::DeviceStatus::PermissionDenied,
            last_success_ms: None,
        },
        failure: Some(taskmanager_core::SmartSelfTestFailure::PermissionDenied),
        ..SmartSelfTestReport::default()
    };
    let refresh_reports = Arc::new(Mutex::new(vec![(
        "permission".to_string(),
        permission_report,
    )]));
    let observation_handle = spawn_complete(fake_registry(FakeProvider {
        smart_refresh_reports: refresh_reports,
        ..FakeProvider::default()
    }));
    let intent = SmartSelfTestIntent {
        device_id: DeviceId::new("disk:permission"),
        device_generation: DeviceGeneration::INITIAL,
        device_key: "permission".into(),
        display_name: "Permission".into(),
        kind: taskmanager_core::SmartSelfTestKind::Short,
    };
    let target = intent.target();
    let start_id = ids.next_id();
    observation_handle
        .smart_control()
        .expect("SMART control facet")
        .try_submit(RequestEnvelope {
            id: start_id,
            capability: CapabilityId::SMART_CONTROL,
            submitted_at_ms: 2,
            payload: SmartControlRequest::StartSelfTest(intent),
        })
        .expect("SMART start accepted");
    assert_eq!(wait_event(&observation_handle).request_id, start_id);

    let refresh_id = ids.next_id();
    observation_handle
        .smart_observation()
        .expect("SMART observation facet")
        .try_submit(RequestEnvelope {
            id: refresh_id,
            capability: CapabilityId::SMART,
            submitted_at_ms: 3,
            payload: SmartObservationRequest::RefreshTarget(target),
        })
        .expect("SMART observation accepted");
    let refresh = wait_event(&observation_handle);
    assert_eq!(refresh.request_id, refresh_id);
    assert!(matches!(
        refresh.outcome,
        Ok(PlatformEvent::Smart(SmartEvent::Batch(ref batch)))
            if batch.issues.iter().any(|issue|
                issue.failure == FailureKind::PermissionDenied)
                && batch.observations.iter().any(|observation|
                    observation.report.failure
                        == Some(taskmanager_core::SmartSelfTestFailure::PermissionDenied))
    ));
    let observation_capability = observation_handle
        .capabilities()
        .snapshot()
        .get(&CapabilityId::SMART)
        .cloned()
        .expect("SMART observation capability");
    assert_eq!(
        observation_capability.status,
        CapabilityStatus::PermissionRequired
    );
    assert_eq!(observation_capability.last_success_at_ms, None);
}
