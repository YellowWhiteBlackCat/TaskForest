// Narrow imports (mirroring root/termination.rs): a `use super::*` glob in a
// test module pulls gpui's deeply generic prelude into the test scope and
// pushes the `#[test]` attribute macro past its recursion limit.
use super::{ServiceControlConfirmation, requires_service_confirmation};
use crate::gpui_app::root::{RootView, TopPage};
use gpui::AppContext;
use std::sync::{Arc, Mutex};
use taskmanager_application::i18n;
use taskmanager_application::{
    EnvironmentFacets, PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle,
    SessionControlConfirmation, SessionControlRequest, TelemetryRefreshPolicy,
};
use taskmanager_core::core::services::ServiceAction;
use taskmanager_core::core::session::{SessionControlAction, SessionIncarnation, SessionItem};
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
    RequestEnvelope, RequestPort, SubmissionError,
};
use taskmanager_shell::fixture::demo_direct_track;
use taskmanager_shell::fixture::{DirectTrackSeedFact, seed_direct_track_fact};
use taskmanager_telemetry_store::{HistoryRetention, TelemetryStore};
use taskmanager_theme::Theme;

#[test]
fn only_destructive_service_actions_require_confirmation() {
    // Start / Enable are constructive and stay immediate.
    assert!(!requires_service_confirmation(ServiceAction::Start));
    assert!(!requires_service_confirmation(ServiceAction::Enable));
    // Stop / Restart / Disable can lock the session → gated.
    assert!(requires_service_confirmation(ServiceAction::Stop));
    assert!(requires_service_confirmation(ServiceAction::Restart));
    assert!(requires_service_confirmation(ServiceAction::Disable));
}

#[test]
fn service_intent_freezes_display_name_and_id() {
    let intent = ServiceControlConfirmation::Service {
        service_id: ServiceId::new("NetworkManager.service"),
        display_name: "NetworkManager".into(),
        action: ServiceAction::Stop,
    };
    assert!(intent.is_high_risk());
    assert_eq!(intent.confirm_label(), i18n::t("svc.stop"));
    // The message template substitutes the frozen display name.
    assert!(intent.dialog_message().contains("NetworkManager"));
}

#[test]
fn startup_disable_is_not_high_risk_but_is_gated() {
    let entry = StartupEntry {
        id: "desktop:helper.desktop".into(),
        name: "Desktop helper".into(),
        exec: "capture-desktop-helper --background".into(),
        enabled: true,
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: "/home/<user>/.config/autostart/helper.desktop".into(),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    };
    let intent = ServiceControlConfirmation::Startup {
        entry,
        enabled: false,
    };
    // Startup toggles are gated but do not paint in the destructive accent.
    assert!(!intent.is_high_risk());
    assert_eq!(intent.confirm_label(), i18n::t("common.disable"));
    assert!(intent.dialog_message().contains("Desktop helper"));
}

#[test]
fn session_confirmation_copy_uses_the_frozen_session_details() {
    let session = trusted_session("7", 700);
    let identity = session.identity().expect("trusted fixture identity");
    let mut shell = demo_direct_track();
    let action = SessionControlAction::Disconnect;
    let request_id = shell.begin_session_control(identity, action);
    let confirmation = ServiceControlConfirmation::Session {
        intent: SessionControlConfirmation {
            request_id,
            session,
            action,
        },
    };

    assert!(confirmation.is_high_risk());
    assert_eq!(
        confirmation.dialog_title(),
        i18n::t("confirm.session_title")
    );
    assert_eq!(confirmation.confirm_label(), i18n::t("users.disconnect"));
    let message = confirmation.dialog_message();
    assert!(message.contains("7"));
    assert!(message.contains("capture-user"));
    assert!(message.contains(i18n::t("confirm.provider_body")));
}

fn trusted_session(id: &str, created_monotonic_usec: u64) -> SessionItem {
    SessionItem {
        id: id.into(),
        incarnation: Some(SessionIncarnation::LinuxLogind {
            boot_id: "test-boot".into(),
            created_monotonic_usec,
        }),
        uid: 1000,
        user: "capture-user".into(),
        seat: Some("seat0".into()),
        tty: Some("tty7".into()),
        remote: false,
        timestamp: None,
    }
}

fn install_sessions(view: &mut RootView, sessions: Vec<SessionItem>) {
    seed_direct_track_fact(
        &mut view.shell,
        DirectTrackSeedFact::Sessions(Some(sessions.clone())),
    );
    seed_direct_track_fact(
        &mut view.shell,
        DirectTrackSeedFact::SessionsSource(Some(Vec::<SourceStatus>::new())),
    );
    let projection = view.projection().clone();
    view.materialized.replace_sessions(
        projection.sessions_revision,
        projection.sessions.clone().unwrap_or_default(),
        projection.sessions_source.clone().unwrap_or_default(),
    );
    view.page = TopPage::Users;
    view.selected_session = sessions.first().and_then(SessionItem::identity);
}

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

struct RecordingSessionControl(Arc<Mutex<Vec<SessionControlRequest>>>);
impl RequestPort for RecordingSessionControl {
    type Request = SessionControlRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(request.payload);
        Ok(())
    }
}

fn platform_client(submitted: Arc<Mutex<Vec<SessionControlRequest>>>) -> PlatformClient {
    let environment = EnvironmentFacets::default()
        .with_session_control(Arc::new(RecordingSessionControl(submitted)));
    PlatformClient::new(PlatformHandle::new(
        Arc::new(NoCapabilities),
        Arc::new(NoEvents),
        PlatformFacets::default().with_environment(environment),
    ))
}

#[gpui::test]
async fn session_confirmation_confirms_the_exact_identity_once(cx: &mut gpui::TestAppContext) {
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
    let session = trusted_session("7", 700);

    root.update(cx, |view, cx| {
        install_sessions(view, vec![session.clone()]);
        assert!(
            view.request_session_control_confirmation(&session, SessionControlAction::Disconnect,)
        );
        assert!(view.confirm_session_control_confirmation(cx));
        assert!(!view.confirm_session_control_confirmation(cx));
    });

    let requests = submitted
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(requests.len(), 1, "a repeated confirm cannot submit twice");
    assert_eq!(requests[0].identity, session.identity().expect("identity"));
    assert_eq!(requests[0].action, SessionControlAction::Disconnect);
}

#[gpui::test]
async fn session_confirmation_cancel_and_replaced_target_submit_nothing(
    cx: &mut gpui::TestAppContext,
) {
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
    let session = trusted_session("7", 700);

    root.update(cx, |view, _cx| {
        install_sessions(view, vec![session.clone()]);
        assert!(
            view.request_session_control_confirmation(&session, SessionControlAction::Disconnect,)
        );
        view.cancel_control_confirmation();
        assert!(view.pending_confirmation().is_none());
        assert!(
            view.shell
                .projection()
                .session_control_requests
                .pending()
                .is_none()
        );

        assert!(
            view.request_session_control_confirmation(&session, SessionControlAction::Disconnect,)
        );
        let replaced = trusted_session("7", 701);
        install_sessions(view, vec![replaced]);
        assert!(!view.confirm_session_control_confirmation(_cx));
        assert!(view.pending_confirmation().is_none());
    });

    assert!(
        submitted
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty(),
        "Cancel and a replaced incarnation must not submit"
    );
}

#[gpui::test]
async fn session_confirmation_rejects_missing_or_duplicate_current_rows(
    cx: &mut gpui::TestAppContext,
) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    let session = trusted_session("7", 700);
    root.update(cx, |view, _| {
        let mut no_identity = session.clone();
        no_identity.incarnation = None;
        install_sessions(view, vec![no_identity]);
        assert!(!view.request_session_control_confirmation(&session, SessionControlAction::Lock,));

        install_sessions(view, vec![session.clone(), session.clone()]);
        assert!(!view.request_session_control_confirmation(&session, SessionControlAction::Lock,));
        assert!(view.pending_confirmation().is_none());
    });
}
