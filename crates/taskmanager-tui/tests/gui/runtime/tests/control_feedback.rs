//! Control-outcome feedback tests (G-01 payoff): a confirmed control action
//! must become a VISIBLE typed outcome — the footer feedback line renders the
//! per-frozen-identity result with the localized typed reason on failure, and
//! the completion's process-list refresh request is drained through the shared
//! effect seam. The chains below drive the real key path (`Delete` + `y`, the
//! gated batch confirm), the real `queue_effect` submission (a recording
//! process-control port), and the real platform-batch fold.

use super::super::*;

use std::sync::{Arc, Mutex};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyModifiers;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::{
    AppPage, CapabilityCatalog, CapabilitySnapshot, CorrelatedEvent, EventEnvelope, EventPort,
    EventPortError, EventSequence, FailureKind, FrozenProcessIdentity, OperationFailure,
    PlatformEvent, PlatformEventBatch, PlatformFacets, PlatformHandle, ProcessBatchAction,
    ProcessBatchIntent, ProcessBatchResult, ProcessBatchTargetResult, ProcessControlRequest,
    ProcessFacets, ProcessListRequest, ProviderFailure, RequestEnvelope, RequestId, RequestPort,
    SubmissionError,
};

use crate::render;
use crate::{TuiApp, TuiTheme};

/// Render the live frame through the same TestBackend path the render tests
/// use, pinning English + serializing against the language-flipping i18n test.
fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, app, TuiTheme::default()))
        .expect("draw");
    terminal.backend().to_string()
}

fn app_on_processes() -> TuiApp {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    app.shell.selected = 0;
    app
}

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

/// Records every accepted payload (with its envelope request id) on one typed
/// lane, so a correlated completion event can echo the exact id back.
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

impl<T: taskmanager_application::CapabilityRequest> RequestPort for RecordingRequests<T> {
    type Request = T;

    fn try_submit(&self, request: RequestEnvelope<T>) -> Result<(), SubmissionError> {
        self.submitted
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((request.id, request.payload));
        Ok(())
    }
}

fn recorded<T: Clone>(recorder: &RecordingRequests<T>) -> Vec<(RequestId, T)> {
    recorder
        .submitted
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// A client with recording process-control + process-list lanes — enough for
/// the End-task chain (submit EndTask, drain the completion's refresh).
fn control_client(
    control: Arc<RecordingRequests<ProcessControlRequest>>,
    list: Arc<RecordingRequests<ProcessListRequest>>,
) -> taskmanager_application::PlatformClient {
    taskmanager_application::PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default().with_process(
            ProcessFacets::default()
                .with_control(control)
                .with_list(list),
        ),
    ))
}

fn press(app: &mut TuiApp, code: ratatui::crossterm::event::KeyCode) -> Option<PlatformEffect> {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE))
}

/// The full single-target chain: `Delete` opens the gated confirmation, `y`
/// emits the EndTask effect, `queue_effect` submits it (the recording port
/// captures the envelope id), the platform batch's correlated completion folds
/// through the shell, and the frame renders the typed success feedback line.
#[test]
fn end_task_completion_renders_typed_success_feedback_and_refresh_is_drained() {
    // Pin before the chain starts: the feedback notice text is frozen by
    // `apply_platform_batch`, earlier than `frame_text`'s draw-time pin.
    taskmanager_test_support::pin_english();
    let control = Arc::new(RecordingRequests::<ProcessControlRequest>::default());
    let list = Arc::new(RecordingRequests::<ProcessListRequest>::default());
    let mut client = control_client(control.clone(), list.clone());

    let mut app = app_on_processes();
    let target = app
        .shell
        .selected_process_identity()
        .expect("selected row has a frozen identity");

    // Delete opens the confirmation; no effect is emitted yet.
    assert!(press(&mut app, ratatui::crossterm::event::KeyCode::Delete).is_none());
    assert!(app.pending_end().is_some(), "Delete must gate the end task");

    // y confirms: the EndTask effect carries the frozen identity.
    let effect = press(&mut app, ratatui::crossterm::event::KeyCode::Char('y'));
    let Some(PlatformEffect::EndTask(submitted)) = effect else {
        panic!("confirm must emit EndTask, got {effect:?}");
    };
    assert_eq!(submitted, target);

    // The runtime queues it; the provider lane receives the typed request and
    // the shell records the submission for completion correlation.
    taskmanager_shell::queue_effect(
        &mut app.shell,
        &mut client,
        PlatformEffect::EndTask(target.clone()),
    );
    let submissions = recorded(&control);
    assert_eq!(submissions.len(), 1, "exactly one control submission");
    let (request_id, payload) = &submissions[0];
    assert!(matches!(payload, ProcessControlRequest::EndTask(_)));

    // The correlated completion arrives in the next platform batch — the same
    // envelope shape the live worker publishes.
    let mut batch = PlatformEventBatch::default();
    batch.process_events.push(CorrelatedEvent {
        request_id: *request_id,
        capability: taskmanager_application::CapabilityId::PROCESS_CONTROL,
        provider: None,
        sequence: EventSequence::new(1),
        observed_at_ms: 1_000,
        event: taskmanager_application::ProcessEvent::EndTaskCompleted(target.clone()),
    });
    app.apply_platform_batch(batch);

    // The shell's single typed feedback authority accepted the outcome; the
    // footer renders the same immutable notice.
    let feedback = app
        .shell
        .feedback_notice()
        .expect("typed feedback recorded");
    assert_eq!(
        feedback.source(),
        taskmanager_shell::FeedbackSource::Control
    );
    assert_eq!(
        feedback.severity(),
        taskmanager_shell::FeedbackSeverity::Success
    );
    assert!(feedback.text().contains(&target.pid.to_string()));
    let text = frame_text(&app, 140, 36);
    assert!(
        text.contains(&format!("End task succeeded for PID {}", target.pid)),
        "the typed success line must render in the footer, got:\n{text}"
    );
    assert!(
        text.contains('\u{2713}'),
        "the success marker must render with the line"
    );

    // The completion requested a process-list refresh; the drain helper
    // submits it through the shared seam exactly once (one-shot).
    assert!(drain_process_refresh(&mut app, &mut client));
    let lists = recorded(&list);
    assert_eq!(
        lists.len(),
        1,
        "the drained refresh must reach the process-list lane"
    );
    assert_eq!(lists[0].1, ProcessListRequest::Refresh);
    assert!(!drain_process_refresh(&mut app, &mut client), "one-shot");
}

/// The end-task confirmation's dismiss paths: `n` and Esc both clear the gate
/// without emitting any effect (the destructive request must never fire from a
/// dismissal).
#[test]
fn end_task_confirmation_n_and_esc_dismiss_without_an_effect() {
    let mut app = app_on_processes();
    assert!(press(&mut app, ratatui::crossterm::event::KeyCode::Delete).is_none());
    assert!(app.pending_end().is_some(), "Delete must gate the end task");

    // n dismisses: no effect, the gate closes.
    assert!(press(&mut app, ratatui::crossterm::event::KeyCode::Char('n')).is_none());
    assert!(
        app.pending_end().is_none(),
        "n must dismiss the confirmation"
    );

    // Re-gate and dismiss with Esc (the shared router's Dismiss path).
    assert!(press(&mut app, ratatui::crossterm::event::KeyCode::Delete).is_none());
    assert!(app.pending_end().is_some());
    assert!(press(&mut app, ratatui::crossterm::event::KeyCode::Esc).is_none());
    assert!(
        app.pending_end().is_none(),
        "Esc must dismiss the confirmation"
    );
}

/// A failed single-target control renders the typed failure reason (the
/// shared `control_error_detail` single source), never a Debug dump of the
/// `FailureKind`.
#[test]
fn failed_single_control_renders_the_typed_reason_not_debug() {
    // Pin before the failure folds: the notice text freezes at batch-apply
    // time, earlier than `frame_text`'s draw-time pin.
    taskmanager_test_support::pin_english();
    let control = Arc::new(RecordingRequests::<ProcessControlRequest>::default());
    let list = Arc::new(RecordingRequests::<ProcessListRequest>::default());
    let mut client = control_client(control.clone(), list);
    let mut app = app_on_processes();
    let target = app
        .shell
        .selected_process_identity()
        .expect("selected row has a frozen identity");
    taskmanager_shell::queue_effect(
        &mut app.shell,
        &mut client,
        PlatformEffect::EndTask(target.clone()),
    );
    let request_id = recorded(&control)[0].0;
    let mut batch = PlatformEventBatch::default();
    batch.failures.push(OperationFailure {
        request_id,
        capability: taskmanager_application::CapabilityId::PROCESS_CONTROL,
        sequence: EventSequence::new(1),
        kind: FailureKind::PermissionDenied,
        retry: ProviderFailure::from_kind(FailureKind::PermissionDenied).retry(),
        provider: None,
        observed_at_ms: 1_000,
    });
    app.apply_platform_batch(batch);

    let text = frame_text(&app, 140, 36);
    assert!(
        text.contains(&format!("End task failed for PID {}", target.pid)),
        "the typed failure line must render, got:\n{text}"
    );
    // The typed reason is the localized single-source string, not Debug.
    assert!(
        text.contains("Permission denied; administrator privileges may be required"),
        "the localized typed reason must render, got:\n{text}"
    );
    assert!(
        !text.contains("PermissionDenied"),
        "the failure must never render as Debug formatting, got:\n{text}"
    );
}

/// The batch lane (Kill / Suspend / Resume / Priority) renders per-item
/// outcomes: the applied/total count plus the first failing frozen identity
/// with its typed reason. Driven through the real gated confirm ('y').
#[test]
fn batch_completion_renders_per_item_outcomes_with_typed_failure_reason() {
    // Pin before the batch folds: the notice text freezes at batch-apply
    // time, earlier than `frame_text`'s draw-time pin.
    taskmanager_test_support::pin_english();
    let mut app = app_on_processes();
    // Two trustworthy rows so the batch freezes two targets.
    let processes = app
        .projection()
        .processes
        .as_ref()
        .expect("demo processes")
        .clone();
    let targets: Vec<FrozenProcessIdentity> = processes
        .iter()
        .filter_map(FrozenProcessIdentity::from_process)
        .take(2)
        .collect();
    assert_eq!(targets.len(), 2, "the demo list has trustworthy rows");

    // Request the gated Kill over the anchor; y confirms (ExecuteBatch).
    assert!(
        app.shell
            .request_process_batch(ProcessBatchAction::Kill)
            .is_none(),
        "Kill gates behind the confirmation"
    );
    assert!(app.shell.pending_batch().is_some());
    let effect = app.shell.confirm_process_batch().expect("confirm emits");
    let PlatformEffect::ExecuteBatch(intent) = &effect else {
        panic!("confirm must emit ExecuteBatch, got {effect:?}");
    };
    assert_eq!(intent.targets.len(), 1, "the anchor row is frozen");

    // The provider answers with a per-target result: the frozen row applied,
    // plus a second failed identity the intent did not carry is impossible —
    // so exercise both outcomes by folding a result over the SAME intent with
    // one applied and one failed target rebuilt from the demo rows.
    let mut mixed = intent.clone();
    mixed.targets = targets.clone();
    let result = ProcessBatchResult {
        intent: mixed,
        targets: vec![
            (targets[0].clone(), ProcessBatchTargetResult::Applied),
            (
                targets[1].clone(),
                ProcessBatchTargetResult::Failed(FailureKind::PermissionDenied),
            ),
        ],
    };
    let request_id = RequestId::new(7).expect("non-zero fixture id");
    taskmanager_shell::fixture::seed_process_batch_loading(
        &mut app.shell,
        result.intent.clone(),
        request_id,
    );
    let mut batch = PlatformEventBatch::default();
    batch.process_events.push(CorrelatedEvent {
        request_id,
        capability: taskmanager_application::CapabilityId::PROCESS_CONTROL,
        provider: None,
        sequence: EventSequence::new(1),
        observed_at_ms: 1_000,
        event: taskmanager_application::ProcessEvent::BatchCompleted(result),
    });
    app.apply_platform_batch(batch);

    // The footer renders the count plus the failing item with its typed reason.
    let text = frame_text(&app, 140, 36);
    assert!(
        text.contains("Force kill: 1/2 targets applied"),
        "the batch count line must render, got:\n{text}"
    );
    assert!(
        text.contains(&format!("{} ({}): ", targets[1].name, targets[1].pid)),
        "the failing frozen identity must render, got:\n{text}"
    );
    assert!(
        text.contains("Permission denied; administrator privileges may be required"),
        "the typed failure reason must render, got:\n{text}"
    );
    assert!(
        !text.contains("PermissionDenied"),
        "the failure must never render as Debug formatting, got:\n{text}"
    );
}

/// An all-applied batch outcome renders the success marker and no failure
/// item — the per-item line is failure-only.
#[test]
fn fully_applied_batch_renders_success_marker_without_failure_item() {
    // Pin before the batch folds: the notice text freezes at batch-apply
    // time, earlier than `frame_text`'s draw-time pin.
    taskmanager_test_support::pin_english();
    let mut app = app_on_processes();
    let identity = app
        .shell
        .selected_process_identity()
        .expect("selected row has a frozen identity");
    let intent = ProcessBatchIntent {
        action: ProcessBatchAction::Suspend,
        scope: Default::default(),
        targets: vec![identity.clone()],
    };
    let result = ProcessBatchResult {
        intent,
        targets: vec![(identity.clone(), ProcessBatchTargetResult::Applied)],
    };
    let request_id = RequestId::new(9).expect("non-zero fixture id");
    taskmanager_shell::fixture::seed_process_batch_loading(
        &mut app.shell,
        result.intent.clone(),
        request_id,
    );
    let mut batch = PlatformEventBatch::default();
    batch.process_events.push(CorrelatedEvent {
        request_id,
        capability: taskmanager_application::CapabilityId::PROCESS_CONTROL,
        provider: None,
        sequence: EventSequence::new(1),
        observed_at_ms: 1_000,
        event: taskmanager_application::ProcessEvent::BatchCompleted(result),
    });
    app.apply_platform_batch(batch);

    let text = frame_text(&app, 140, 36);
    assert!(
        text.contains("Suspend: 1/1 targets applied"),
        "the applied count must render, got:\n{text}"
    );
    assert!(
        text.contains('\u{2713}'),
        "an all-applied batch renders the success marker"
    );
    assert!(
        !text.contains('\u{26a0}'),
        "no failure marker for an all-applied batch"
    );
}
