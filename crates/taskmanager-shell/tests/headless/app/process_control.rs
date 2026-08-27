//! Process-control completion correlation coverage (G-01): the shell fold must
//! surface `EndTaskCompleted` / `SignalCompleted` / `AffinityApplied` /
//! `ResourceLimitsApplied` outcomes to TUI/Iced exactly like GPUI's
//! `complete_process_control` — clear the correlated pending submission,
//! record typed feedback, and request a process refresh — while an
//! uncorrelated outcome changes nothing (fail-closed).
use super::super::*;
use std::sync::{Arc, Mutex};
use taskmanager_application::{
    AppPage, CapabilityCatalog, CapabilityId, CapabilityRequest, CapabilitySnapshot,
    CorrelatedEvent, EventEnvelope, EventPort, EventPortError, EventSequence, FailureKind, KeyCode,
    Modifiers, OperationFailure, PlatformClient, PlatformEvent, PlatformEventBatch,
    PlatformEventContext, PlatformFacets, PlatformHandle, ProcessControlRequest, ProcessEvent,
    ProcessFacets, ProcessSignal, ProviderFailure, RequestEnvelope, RequestId, RequestPort,
    SubmissionError,
};

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

/// Records every accepted (envelope id, payload) pair on one typed lane.
struct RecordingRequests<T> {
    submitted: Mutex<Vec<(RequestId, T)>>,
}

impl<T> Default for RecordingRequests<T> {
    fn default() -> Self {
        Self {
            submitted: Mutex::new(Vec::new()),
        }
    }
}

impl<T: CapabilityRequest> RequestPort for RecordingRequests<T> {
    type Request = T;

    fn try_submit(&self, request: RequestEnvelope<T>) -> Result<(), SubmissionError> {
        self.submitted
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((request.id, request.payload));
        Ok(())
    }
}

fn client_with_process_facets(
    control: Arc<RecordingRequests<ProcessControlRequest>>,
    affinity: Arc<RecordingRequests<taskmanager_application::ProcessAffinityRequest>>,
    affinity_control: Arc<
        RecordingRequests<taskmanager_application::ProcessAffinityControlRequest>,
    >,
) -> PlatformClient {
    PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default().with_process(
            ProcessFacets::default()
                .with_control(control)
                .with_affinity(affinity)
                .with_affinity_control(affinity_control),
        ),
    ))
}

/// The capability a control-side `ProcessEvent` completion belongs to.
/// Compared with `==` because `CapabilityId` constants are `Cow<str>` (not
/// structural-match in patterns).
fn completion_capability(event: &ProcessEvent) -> CapabilityId {
    match event {
        ProcessEvent::Snapshot(_) => CapabilityId::PROCESS_LIST,
        ProcessEvent::NetworkCaptureEscalated => CapabilityId::PROCESS_NETWORK_ESCALATION,
        ProcessEvent::EndTaskCompleted(_)
        | ProcessEvent::BatchCompleted(_)
        | ProcessEvent::SignalCompleted { .. } => CapabilityId::PROCESS_CONTROL,
        ProcessEvent::AffinityApplied { .. } => CapabilityId::PROCESS_AFFINITY_CONTROL,
        ProcessEvent::ResourceLimitsApplied { .. } => CapabilityId::PROCESS_RESOURCE_CONTROL,
    }
}

fn process_event_batch(request_id: RequestId, event: ProcessEvent) -> PlatformEventBatch {
    let mut batch = PlatformEventBatch::default();
    batch.process_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id,
            capability: completion_capability(&event),
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 100,
        },
        event,
    ));
    batch
}

fn recorded_process_control(recorded: &RecordingRequests<ProcessControlRequest>) -> RequestId {
    let submitted = recorded.submitted.lock().unwrap_or_else(|p| p.into_inner());
    assert_eq!(submitted.len(), 1, "exactly one control submission");
    submitted[0].0
}

fn selected_demo_target(app: &mut ShellApp) -> taskmanager_application::FrozenProcessIdentity {
    app.application.active_page = AppPage::Applications;
    app.selected = 1;
    app.selected_process_identity()
        .expect("demo process selection has an authoritative identity")
}

#[test]
fn end_task_completion_clears_pending_records_feedback_and_requests_refresh() {
    taskmanager_test_support::pin_english();
    let recorded = Arc::new(RecordingRequests::<ProcessControlRequest>::default());
    let affinity = Arc::new(RecordingRequests::<
        taskmanager_application::ProcessAffinityRequest,
    >::default());
    let mut app = crate::demo_app();
    let target = selected_demo_target(&mut app);
    let mut client = client_with_process_facets(
        recorded.clone(),
        affinity,
        Arc::new(RecordingRequests::<
            taskmanager_application::ProcessAffinityControlRequest,
        >::default()),
    );

    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::EndTask(target.clone()),
    );
    let request_id = recorded_process_control(&recorded);
    assert!(
        app.data.process_control_requests.pending().is_some(),
        "accepted submission must be pending completion correlation"
    );

    app.apply_platform_batch(process_event_batch(
        request_id,
        ProcessEvent::EndTaskCompleted(target.clone()),
    ));

    assert!(
        app.feedback_text().contains("End task succeeded for PID"),
        "completion must surface in the status line: {}",
        app.feedback_text()
    );
    assert!(
        app.data.process_control_requests.pending().is_none(),
        "completion must clear the pending submission"
    );
    assert_eq!(
        app.take_process_refresh_request(),
        Some(PlatformEffect::Refresh(
            taskmanager_application::RefreshRequest::Processes
        )),
        "completion must request a process-list refresh like GPUI"
    );
    assert_eq!(
        app.take_process_refresh_request(),
        None,
        "the refresh flag is one-shot"
    );
}

#[test]
fn signal_completion_records_typed_signal_feedback() {
    taskmanager_test_support::pin_english();
    let recorded = Arc::new(RecordingRequests::<ProcessControlRequest>::default());
    let affinity = Arc::new(RecordingRequests::<
        taskmanager_application::ProcessAffinityRequest,
    >::default());
    let mut app = crate::demo_app();
    let target = selected_demo_target(&mut app);
    let mut client = client_with_process_facets(
        recorded.clone(),
        affinity,
        Arc::new(RecordingRequests::<
            taskmanager_application::ProcessAffinityControlRequest,
        >::default()),
    );

    let effect = app
        .request_process_signal(ProcessSignal::Terminate)
        .expect("selected identity produces a signal effect");
    queue_effect(&mut app, &mut client, effect);
    let request_id = recorded_process_control(&recorded);

    app.apply_platform_batch(process_event_batch(
        request_id,
        ProcessEvent::SignalCompleted {
            target: target.clone(),
            signal: ProcessSignal::Terminate,
        },
    ));

    assert!(
        app.feedback_text()
            .contains("Signal Terminate succeeded for PID")
    );
    assert_eq!(
        app.take_process_refresh_request(),
        Some(PlatformEffect::Refresh(
            taskmanager_application::RefreshRequest::Processes
        ))
    );
}

/// §4.0 语义完备律: the fold records the SUBMISSION vocabulary. A neutral
/// `Suspend` request completes as `SignalCompleted(Stop)` at the adapter edge
/// (Linux/macOS map the concept to SIGSTOP/SIGCONT), but the feedback must
/// keep saying Suspend — a POSIX signal name is an adapter mapping detail,
/// never the user concept.
#[test]
fn suspend_resume_completions_keep_their_own_vocabulary() {
    taskmanager_test_support::pin_english();
    let recorded = Arc::new(RecordingRequests::<ProcessControlRequest>::default());
    let affinity = Arc::new(RecordingRequests::<
        taskmanager_application::ProcessAffinityRequest,
    >::default());
    let mut app = crate::demo_app();
    let target = selected_demo_target(&mut app);
    let mut client = client_with_process_facets(
        recorded.clone(),
        affinity,
        Arc::new(RecordingRequests::<
            taskmanager_application::ProcessAffinityControlRequest,
        >::default()),
    );

    for (request, completion, kind, label) in [
        (
            ProcessControlRequest::Suspend {
                target: target.clone(),
            },
            ProcessEvent::SignalCompleted {
                target: target.clone(),
                signal: ProcessSignal::Stop,
            },
            ProcessControlKind::Suspend,
            "Suspend succeeded for PID",
        ),
        (
            ProcessControlRequest::Resume {
                target: target.clone(),
            },
            ProcessEvent::SignalCompleted {
                target: target.clone(),
                signal: ProcessSignal::Continue,
            },
            ProcessControlKind::Resume,
            "Resume succeeded for PID",
        ),
    ] {
        // The UI path (GPUI menu dispatch) submits the neutral request
        // directly — there is no Suspend/Resume platform effect — and begins
        // the completion correlation itself.
        let request_id = client
            .submit_process_control(request, 1)
            .expect("neutral submission accepted");
        app.begin_process_control(request_id, target.clone(), kind.clone());

        app.apply_platform_batch(process_event_batch(request_id, completion));

        assert!(
            app.feedback_text().contains(label),
            "feedback must keep the neutral vocabulary: {}",
            app.feedback_text()
        );
    }
}

#[test]
fn affinity_control_completion_records_typed_feedback() {
    taskmanager_test_support::pin_english();
    let recorded = Arc::new(RecordingRequests::<ProcessControlRequest>::default());
    let affinity_read = Arc::new(RecordingRequests::<
        taskmanager_application::ProcessAffinityRequest,
    >::default());
    let affinity_control = Arc::new(RecordingRequests::<
        taskmanager_application::ProcessAffinityControlRequest,
    >::default());
    let mut app = crate::demo_app();
    let target = selected_demo_target(&mut app);
    let mut client =
        client_with_process_facets(recorded.clone(), affinity_read, affinity_control.clone());

    // Prime feedback with a completed end-task so the supersession below is
    // observable (a new submission must clear stale feedback).
    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::EndTask(target.clone()),
    );
    let end_task_id = recorded_process_control(&recorded);
    app.apply_platform_batch(process_event_batch(
        end_task_id,
        ProcessEvent::EndTaskCompleted(target.clone()),
    ));
    assert!(app.feedback_text().contains("End task succeeded for PID"));

    let effect = app
        .request_process_affinity_control(vec![0, 1])
        .expect("selected identity produces an affinity control request");
    queue_effect(&mut app, &mut client, effect);
    let submitted = affinity_control
        .submitted
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(
        submitted.len(),
        1,
        "the affinity write reaches its own lane exactly once"
    );
    let request_id = submitted[0].0;
    drop(submitted);
    assert!(
        app.feedback_text().contains("Process affinity set queued"),
        "the queued notice must replace the prior completion"
    );

    app.apply_platform_batch(process_event_batch(
        request_id,
        ProcessEvent::AffinityApplied {
            target: target.clone(),
            cpus: vec![0, 1],
        },
    ));

    assert!(app.feedback_text().contains("Affinity succeeded for PID"));
    assert_eq!(
        app.take_process_refresh_request(),
        Some(PlatformEffect::Refresh(
            taskmanager_application::RefreshRequest::Processes
        ))
    );
}

#[test]
fn affinity_read_snapshot_is_stored_fail_closed() {
    let recorded = Arc::new(RecordingRequests::<ProcessControlRequest>::default());
    let affinity_read = Arc::new(RecordingRequests::<
        taskmanager_application::ProcessAffinityRequest,
    >::default());
    let mut app = crate::demo_app();
    let target = selected_demo_target(&mut app);
    let mut client = client_with_process_facets(
        recorded,
        affinity_read.clone(),
        Arc::new(RecordingRequests::<
            taskmanager_application::ProcessAffinityControlRequest,
        >::default()),
    );

    let effect = app
        .request_process_affinity()
        .expect("selected identity produces an affinity read");
    queue_effect(&mut app, &mut client, effect);
    let submitted = affinity_read
        .submitted
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(submitted.len(), 1);
    let (request_id, read) = submitted[0].clone();
    assert_eq!(read.target, target);

    // A snapshot echoing the request id AND pid lands in ShellData.
    let mut batch = PlatformEventBatch::default();
    batch.process_affinity_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id,
            capability: CapabilityId::PROCESS_AFFINITY,
            provider: None,
            sequence: EventSequence::new(2),
            observed_at_ms: 200,
        },
        taskmanager_application::ProcessAffinityEvent::Snapshot {
            target: read.target.clone(),
            cpus: vec![2, 3],
        },
    ));
    app.apply_platform_batch(batch);
    assert!(matches!(
        app.process_affinity_state(),
        taskmanager_application::ProcessAffinityState::Ready(ready)
            if ready.request_id == request_id
                && ready.target == read.target
                && ready.cpus == vec![2, 3]
    ));

    // An uncorrelated snapshot (stale request id) changes nothing.
    let mut stale = PlatformEventBatch::default();
    stale.process_affinity_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(9_999).expect("unrelated fixture request id"),
            capability: CapabilityId::PROCESS_AFFINITY,
            provider: None,
            sequence: EventSequence::new(3),
            observed_at_ms: 300,
        },
        taskmanager_application::ProcessAffinityEvent::Snapshot {
            target: read.target,
            cpus: vec![7],
        },
    ));
    app.apply_platform_batch(stale);
    assert!(
        matches!(
            app.process_affinity_state(),
            taskmanager_application::ProcessAffinityState::Ready(ready)
                if ready.cpus == vec![2, 3]
        ),
        "an uncorrelated affinity snapshot must not overwrite the stored read"
    );
}

#[test]
fn affinity_read_failure_is_typed_and_never_leaves_a_snapshot() {
    let affinity_read = Arc::new(RecordingRequests::<
        taskmanager_application::ProcessAffinityRequest,
    >::default());
    let mut app = crate::demo_app();
    let target = selected_demo_target(&mut app);
    let mut client = client_with_process_facets(
        Arc::new(RecordingRequests::<ProcessControlRequest>::default()),
        affinity_read.clone(),
        Arc::new(RecordingRequests::<
            taskmanager_application::ProcessAffinityControlRequest,
        >::default()),
    );

    let effect = app
        .request_process_affinity()
        .expect("selected identity produces an affinity read");
    queue_effect(&mut app, &mut client, effect);
    let request_id = affinity_read
        .submitted
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())[0]
        .0;

    let mut batch = PlatformEventBatch::default();
    batch.failures.push(OperationFailure {
        request_id,
        capability: CapabilityId::PROCESS_AFFINITY,
        sequence: EventSequence::new(4),
        kind: FailureKind::PermissionDenied,
        retry: ProviderFailure::from_kind(FailureKind::PermissionDenied).retry(),
        provider: None,
        observed_at_ms: 400,
    });
    app.apply_platform_batch(batch);

    assert!(matches!(
        app.process_affinity_state(),
        taskmanager_application::ProcessAffinityState::Failed {
            failure: FailureKind::PermissionDenied,
            last_good: None,
            ..
        }
    ));
    assert_eq!(app.selected_process_identity(), Some(target));
}

#[test]
fn uncorrelated_outcome_cannot_clear_pending_state_or_confirmations() {
    taskmanager_test_support::pin_english();
    let recorded = Arc::new(RecordingRequests::<ProcessControlRequest>::default());
    let affinity = Arc::new(RecordingRequests::<
        taskmanager_application::ProcessAffinityRequest,
    >::default());
    let mut app = crate::demo_app();
    let target = selected_demo_target(&mut app);
    let mut client = client_with_process_facets(
        recorded.clone(),
        affinity,
        Arc::new(RecordingRequests::<
            taskmanager_application::ProcessAffinityControlRequest,
        >::default()),
    );

    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::EndTask(target.clone()),
    );
    let request_id = recorded_process_control(&recorded);

    // An unrelated pending confirmation gate is open (the user re-requested
    // end-task without confirming yet).
    assert_eq!(
        app.dispatch_key(ShellKeyEvent::new(KeyCode::Delete, Modifiers::NONE)),
        InputDispatch::Consumed
    );
    assert!(app.pending_end().is_some());

    // A completion for an unknown request id must change nothing.
    let feedback_before = app.feedback_text().to_owned();
    app.apply_platform_batch(process_event_batch(
        RequestId::new(9_999).expect("unrelated fixture request id"),
        ProcessEvent::EndTaskCompleted(target),
    ));
    assert!(app.pending_end().is_some(), "unrelated gate must survive");
    assert!(
        app.data.process_control_requests.pending().is_some(),
        "the real pending submission must survive"
    );
    assert_eq!(app.feedback_text(), feedback_before);
    assert_eq!(
        app.take_process_refresh_request(),
        None,
        "no refresh may be requested for an uncorrelated outcome"
    );

    // The genuinely correlated completion still lands afterwards.
    app.apply_platform_batch(process_event_batch(
        request_id,
        ProcessEvent::EndTaskCompleted(
            app.data
                .process_control_requests
                .pending()
                .expect("pending submission survived")
                .target
                .clone(),
        ),
    ));
    assert!(app.feedback_text().contains("End task succeeded for PID"));
}

#[test]
fn correlated_failure_records_typed_error_and_clears_pending_without_refresh() {
    taskmanager_test_support::pin_english();
    let recorded = Arc::new(RecordingRequests::<ProcessControlRequest>::default());
    let affinity = Arc::new(RecordingRequests::<
        taskmanager_application::ProcessAffinityRequest,
    >::default());
    let mut app = crate::demo_app();
    let target = selected_demo_target(&mut app);
    let mut client = client_with_process_facets(
        recorded.clone(),
        affinity,
        Arc::new(RecordingRequests::<
            taskmanager_application::ProcessAffinityControlRequest,
        >::default()),
    );

    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::EndTask(target.clone()),
    );
    let request_id = recorded_process_control(&recorded);

    let mut batch = PlatformEventBatch::default();
    batch.failures.push(OperationFailure {
        request_id,
        capability: CapabilityId::PROCESS_CONTROL,
        sequence: EventSequence::new(4),
        kind: FailureKind::PermissionDenied,
        retry: ProviderFailure::from_kind(FailureKind::PermissionDenied).retry(),
        provider: None,
        observed_at_ms: 400,
    });
    app.apply_platform_batch(batch);

    assert!(
        app.feedback_text().contains("End task failed"),
        "the typed failure must surface: {}",
        app.feedback_text()
    );
    assert!(app.data.process_control_requests.pending().is_none());
    assert_eq!(
        app.take_process_refresh_request(),
        None,
        "a failed control did not land, so no refresh is requested"
    );
}
