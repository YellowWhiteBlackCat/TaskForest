//! GPUI Run Task submission binds the one command-draft fold.

use super::{RequestId, RootView, i18n, request_submission_failure};
use gpui::AppContext;
use std::sync::{Arc, Mutex};
use taskforest_contract::{
    CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
    RequestEnvelope, RequestPort, SubmissionError, SubmissionErrorKind,
};
use taskmanager_application::{
    CommandLaunchRequest, IntegrationFacets, PlatformClient, PlatformEvent, PlatformFacets,
    PlatformHandle, RequestCorrelation, ShellUiActionIntent, ShellUiActionState,
    TelemetryRefreshPolicy,
};
use taskmanager_telemetry_store::{HistoryRetention, TelemetryStore};
use taskmanager_theme::Theme;
use taskmanager_ui::inputs::text_input::TextInputState;

struct NoCapabilities;
impl CapabilityCatalog for NoCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::default()
    }
}

struct NoEvents;
impl EventPort for NoEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

struct RecordingCommandLaunch(Arc<Mutex<Vec<CommandLaunchRequest>>>);
impl RequestPort for RecordingCommandLaunch {
    type Request = CommandLaunchRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(request.payload);
        Ok(())
    }
}

fn platform_client(submitted: Arc<Mutex<Vec<CommandLaunchRequest>>>) -> PlatformClient {
    let integration = IntegrationFacets::default()
        .with_command_launch(Arc::new(RecordingCommandLaunch(submitted)));
    PlatformClient::new(PlatformHandle::new(
        Arc::new(NoCapabilities),
        Arc::new(NoEvents),
        PlatformFacets::default().with_integration(integration),
    ))
}

fn view_with_draft(cx: &mut gpui::TestAppContext, draft: &str) -> gpui::Entity<RootView> {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, cx| {
        let input = cx.new(|cx| TextInputState::new(cx));
        input.update(cx, |state, cx| state.set_value(draft, cx));
        view.run_input = Some(input);
    });
    root
}

#[gpui::test]
fn an_empty_draft_stays_a_local_rejection_without_a_request_session(cx: &mut gpui::TestAppContext) {
    let root = view_with_draft(cx, " \r\n\t ");
    root.update(cx, |view, cx| {
        assert!(!view.request_run_command(cx));
        assert_eq!(
            view.run_error.as_deref(),
            Some(i18n::t("search.run_command"))
        );
        assert_eq!(
            view.shell.shell_ui_action_state(),
            &ShellUiActionState::Closed
        );
    });
}

#[gpui::test]
fn a_draft_is_canonicalized_by_the_shared_owner_before_submission(cx: &mut gpui::TestAppContext) {
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let client = platform_client(submitted.clone());
    let (telemetry, ingestor) =
        TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::uniform(60));
    let root = cx.new(|cx| {
        RootView::new_with_platform(
            Theme::dark(),
            telemetry,
            ingestor,
            TelemetryRefreshPolicy::default(),
            client,
            cx,
        )
    });
    root.update(cx, |view, cx| {
        let input = cx.new(|cx| TextInputState::new(cx));
        input.update(cx, |state, cx| {
            state.set_value("  echo one\r\n\recho two  ", cx)
        });
        view.run_input = Some(input);
    });
    root.update(cx, |view, cx| {
        assert!(view.request_run_command(cx));
        assert_eq!(view.run_error, None);
        let ShellUiActionState::Loading {
            correlation: RequestCorrelation::Request(request_id),
            intent,
        } = view.shell.shell_ui_action_state()
        else {
            panic!("a submitted command must accept one correlated request session");
        };
        assert_eq!(*request_id, RequestId::MIN);
        let ShellUiActionIntent::Command(request) = intent else {
            panic!("the session must carry one command-launch intent");
        };
        assert_eq!(
            request,
            &CommandLaunchRequest {
                command: "echo one\n\necho two".to_owned(),
            }
        );
    });
    let recorded = submitted
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(
        recorded.len(),
        1,
        "one draft must submit exactly one launch"
    );
    assert_eq!(
        recorded.as_slice(),
        &[
            CommandLaunchRequest::from_draft("  echo one\r\n\recho two  ")
                .expect("non-empty draft")
        ],
        "the platform receives the shared owner's canonical request"
    );
}

#[gpui::test]
fn a_stopped_runtime_rejects_the_same_canonical_request(cx: &mut gpui::TestAppContext) {
    let root = view_with_draft(cx, " /usr/bin/echo hi ");
    root.update(cx, |view, cx| {
        assert!(!view.request_run_command(cx));
        let ShellUiActionState::Failed(failed) = view.shell.shell_ui_action_state() else {
            panic!("a stopped platform must reject the command session");
        };
        assert!(matches!(failed.correlation, RequestCorrelation::Attempt(_)));
        assert_eq!(
            failed.intent,
            ShellUiActionIntent::Command(
                CommandLaunchRequest::from_draft(" /usr/bin/echo hi ").expect("non-empty draft")
            )
        );
        assert_eq!(
            failed.failure,
            request_submission_failure(SubmissionErrorKind::RuntimeStopped)
        );
        assert!(view.run_error.is_some(), "the dialog receives the failure");
    });
}
