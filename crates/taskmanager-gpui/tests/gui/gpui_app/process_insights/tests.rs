use gpui::{AppContext, Context, IntoElement, Render, TestAppContext, Window, px, size};

use taskmanager_application::NetworkEscalationState;
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_core::core::process_telemetry::{ProcessIdentity, ProcessTelemetrySnapshot};
use taskmanager_theme::Theme;

use super::view::format_connection;
use super::{
    ProcessInsightsError, ProcessInsightsErrorKind, ProcessInsightsLabels,
    ProcessInsightsRenderState, ProcessInsightsState, process_insights_capture_fixture,
    process_insights_layout, render_process_insights, state_from_snapshot,
};

fn escalation_failed(state: NetworkEscalationState) -> bool {
    matches!(state, NetworkEscalationState::Failed(_))
}

struct FixtureView {
    state: ProcessInsightsState,
    labels: ProcessInsightsLabels,
    theme: Theme,
    net_escalation: NetworkEscalationState,
    entity: gpui::Entity<crate::gpui_app::root::RootView>,
}

impl Render for FixtureView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let available = (f32::from(window.viewport_size().width) - 80.0).max(240.0);
        let state = match &self.state {
            ProcessInsightsState::Loading { .. } => ProcessInsightsRenderState::Loading,
            ProcessInsightsState::Ready(snapshot) => ProcessInsightsRenderState::Ready(snapshot),
            ProcessInsightsState::Error(error) => ProcessInsightsRenderState::Error(error),
        };
        render_process_insights(
            &self.theme,
            state,
            &self.labels,
            available,
            self.net_escalation,
            self.entity.clone(),
            taskmanager_core::core::units::UnitPreferences::default(),
        )
    }
}

#[test]
fn typed_state_preserves_unavailable_as_an_error_not_zero() {
    let mut snapshot = ProcessTelemetrySnapshot {
        identity: ProcessIdentity {
            pid: 77,
            start_token: 1,
        },
        state: DeviceState::healthy(10),
        ..Default::default()
    };
    snapshot.state = snapshot
        .state
        .transition(DeviceStatus::PermissionDenied, 20);
    assert_eq!(
        state_from_snapshot(snapshot),
        ProcessInsightsState::Error(ProcessInsightsError {
            identity: ProcessLiveKey::from_parts(77, 1),
            kind: ProcessInsightsErrorKind::PermissionDenied,
            last_success_ms: Some(10),
        })
    );
    let ProcessInsightsState::Ready(fixture) = process_insights_capture_fixture() else {
        panic!("capture fixture must be ready")
    };
    assert_eq!(fixture.network.rx_bytes_per_sec, None);
    assert_eq!(
        fixture.network.traffic_state.status,
        DeviceStatus::Unsupported
    );
}

#[test]
fn responsive_layout_switches_to_one_column_at_compact_width() {
    assert_eq!(process_insights_layout(1100.0).columns, 2);
    assert_eq!(process_insights_layout(640.0).columns, 1);
    assert!(process_insights_layout(180.0).card_width >= 240.0);
}

#[test]
fn connection_readout_preserves_ip_labels_and_displays_local_endpoint_truth() {
    let ProcessInsightsState::Ready(fixture) = process_insights_capture_fixture() else {
        panic!("capture fixture must be ready")
    };

    assert_eq!(
        format_connection(&fixture.network.connections[0]),
        "TCP  127.0.0.1:51842 → 10.20.0.8:443"
    );
    assert_eq!(
        format_connection(&fixture.network.connections[1]),
        "UDP6  [::1]:53535 → [::1]:53"
    );
    assert_eq!(
        format_connection(&fixture.network.connections[2]),
        "LOCAL  /run/taskmanager.sock → —"
    );
    assert!(
        fixture.network.connections[2]
            .local
            .as_socket_addr()
            .is_none()
    );
}

#[gpui::test]
async fn capture_fixture_renders_at_reference_and_compact_sizes(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| {
        let entity = _cx.new(|cx| crate::gpui_app::root::RootView::new(Theme::dark(), cx));
        FixtureView {
            state: process_insights_capture_fixture(),
            labels: ProcessInsightsLabels::capture_fixture(),
            theme: Theme::dark(),
            net_escalation: NetworkEscalationState::Closed,
            entity,
        }
    });
    for viewport in [(1180.0, 780.0), (720.0, 480.0)] {
        cx.simulate_window_resize(window.into(), size(px(viewport.0), px(viewport.1)));
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
    }
}

/// The per-process-network escalation affordance (ADR-023/024/025): a typed
/// `RequiresEscalation` traffic failure renders the authorization pill, and
/// clicking it routes through RootView's escalation submission (a missing
/// platform client degrades to the typed `RuntimeStopped` failure state).
#[gpui::test]
async fn escalation_pill_renders_for_requires_escalation_and_submits_on_click(
    cx: &mut TestAppContext,
) {
    use taskmanager_core::core::{DeviceStatus, FailureKind};

    let mut snapshot = {
        let ProcessInsightsState::Ready(snapshot) = process_insights_capture_fixture() else {
            panic!("capture fixture must be ready");
        };
        *snapshot
    };
    snapshot.network.traffic_failure = Some(FailureKind::RequiresEscalation);
    snapshot.network.traffic_state =
        DeviceState::default().transition(DeviceStatus::PermissionDenied, 1);
    let entity = cx.new(|cx| crate::gpui_app::root::RootView::new(Theme::dark(), cx));
    let window = cx.add_window(|_window, _cx| FixtureView {
        state: ProcessInsightsState::Ready(Box::new(snapshot)),
        labels: ProcessInsightsLabels::capture_fixture(),
        theme: Theme::dark(),
        net_escalation: NetworkEscalationState::Closed,
        entity: entity.clone(),
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    let pill_bounds = vcx
        .debug_bounds("process-insights-net-escalation")
        .expect("the escalation pill must render for a RequiresEscalation traffic failure");
    vcx.simulate_click(pill_bounds.center(), Default::default());
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    // No platform client exists in this harness: the submission degrades to
    // the typed RuntimeStopped failure (never a silent no-op).
    let escalation = entity.read_with(cx, |view, _| *view.shell.network_escalation_state());
    assert!(
        escalation_failed(escalation),
        "escalation without a platform client must land in the typed failure state"
    );
}

// ── G-04a: the escalation-lane failure arm ─────────────────────────────────
//
// A declined OS-native prompt surfaces as an `OperationFailure` on the
// `PROCESS_NETWORK_ESCALATION` lane. These tests drive the full batch path
// (`apply_platform_event_batch`) against a RootView wired to a stub platform
// client whose escalation port accepts submissions — the same harness shape as
// `taskmanager-application`'s client tests (`EmptyCapabilities`/`EmptyEvents`).

/// Capability catalog stub: reports an empty snapshot, never a real catalog.
struct NoCapabilities;
impl taskmanager_platform_contract::CapabilityCatalog for NoCapabilities {
    fn snapshot(&self) -> taskmanager_platform_contract::CapabilitySnapshot {
        taskmanager_platform_contract::CapabilitySnapshot::default()
    }
}

/// Event-port stub: the runtime lane never delivers events itself here; the
/// tests feed correlated batches directly.
struct NoEvents;
impl taskmanager_platform_contract::EventPort for NoEvents {
    type Event = taskmanager_application::PlatformEvent;

    fn try_recv(
        &self,
    ) -> Result<
        Option<taskmanager_platform_contract::EventEnvelope<Self::Event>>,
        taskmanager_platform_contract::EventPortError,
    > {
        Ok(None)
    }
}

/// Escalation-port stub: accepts every submission and records the correlated
/// request ids so the tests can assert re-submission behavior.
struct AcceptingEscalation(
    std::sync::Arc<std::sync::Mutex<Vec<taskmanager_platform_contract::RequestId>>>,
);
impl taskmanager_platform_contract::RequestPort for AcceptingEscalation {
    type Request = taskmanager_application::ProcessNetworkEscalationRequest;

    fn try_submit(
        &self,
        request: taskmanager_platform_contract::RequestEnvelope<Self::Request>,
    ) -> Result<(), taskmanager_platform_contract::SubmissionError> {
        if let Ok(mut submitted) = self.0.lock() {
            submitted.push(request.id);
        }
        Ok(())
    }
}

fn root_with_escalation_platform(
    cx: &mut gpui::TestAppContext,
) -> (
    gpui::Entity<crate::gpui_app::root::RootView>,
    std::sync::Arc<std::sync::Mutex<Vec<taskmanager_platform_contract::RequestId>>>,
) {
    use taskmanager_application::{PlatformClient, PlatformFacets, PlatformHandle, ProcessFacets};

    let submitted = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let facets = PlatformFacets::default().with_process(
        ProcessFacets::default()
            .with_network_escalation(std::sync::Arc::new(AcceptingEscalation(submitted.clone()))),
    );
    let client = PlatformClient::new(PlatformHandle::new(
        std::sync::Arc::new(NoCapabilities),
        std::sync::Arc::new(NoEvents),
        facets,
    ));
    let (telemetry, ingestor) =
        taskmanager_telemetry_store::TelemetryStore::shared_with_correlated_ingestion(60);
    let view = cx.new(|cx| {
        crate::gpui_app::root::RootView::new_with_platform(
            taskmanager_theme::Theme::dark(),
            telemetry,
            ingestor,
            taskmanager_application::TelemetryRefreshPolicy::default(),
            client,
            cx,
        )
    });
    (view, submitted)
}

fn escalation_failure_batch(
    request_id: taskmanager_platform_contract::RequestId,
    kind: taskmanager_core::core::FailureKind,
) -> taskmanager_application::PlatformEventBatch {
    taskmanager_application::PlatformEventBatch {
        failures: vec![taskmanager_platform_contract::OperationFailure {
            request_id,
            capability: taskmanager_platform_contract::CapabilityId::PROCESS_NETWORK_ESCALATION,
            sequence: taskmanager_platform_contract::EventSequence::new(1),
            kind,
            retry: taskmanager_platform_contract::RetryDisposition::AfterCapabilityChange,
            provider: Some(taskmanager_core::core::identity::ProviderId::borrowed(
                "linux.net-launcher",
            )),
            observed_at_ms: 1_001,
        }],
        ..Default::default()
    }
}

fn escalated_batch(
    request_id: taskmanager_platform_contract::RequestId,
) -> taskmanager_application::PlatformEventBatch {
    taskmanager_application::PlatformEventBatch {
        process_events: vec![taskmanager_application::CorrelatedEvent::new(
            taskmanager_application::PlatformEventContext {
                request_id,
                capability: taskmanager_platform_contract::CapabilityId::PROCESS_NETWORK_ESCALATION,
                provider: Some(taskmanager_core::core::identity::ProviderId::borrowed(
                    "linux.net-launcher",
                )),
                sequence: taskmanager_platform_contract::EventSequence::new(2),
                observed_at_ms: 1_002,
            },
            taskmanager_application::ProcessEvent::NetworkCaptureEscalated,
        )],
        ..Default::default()
    }
}

/// The declined-prompt failure path (G-04a): a lane `PermissionDenied` for the
/// correlated in-flight request transitions the pill from the stuck
/// "Waiting for authorization…" `Pending` to the typed `Failed` state with the
/// runtime reason, a stale failure from an abandoned request cannot clobber a
/// newer attempt, the failed pill still re-submits on click, and the success
/// event clears everything back to normal.
#[gpui::test]
async fn declined_prompt_transitions_the_pill_and_retry_resubmits(cx: &mut gpui::TestAppContext) {
    use taskmanager_application::{NetworkEscalationFailed, RequestCorrelation};
    use taskmanager_platform_contract::RequestId;

    let (view, submitted) = root_with_escalation_platform(cx);

    // Click 1: submission accepted → the pill waits for the prompt.
    view.update(cx, |view, cx| {
        view.request_process_network_escalation(cx);
        assert!(matches!(
            view.shell.network_escalation_state(),
            NetworkEscalationState::Loading(_)
        ));
    });
    let first = submitted
        .lock()
        .expect("submitted ids")
        .pop()
        .expect("first escalation submission");

    // A stale lane failure for an unrelated request id must NOT flip the pill:
    // only the correlated in-flight request is consumable.
    let stale = RequestId::new(999_999).expect("stale fixture request");
    view.update(cx, |view, cx| {
        view.apply_platform_event_batch(
            escalation_failure_batch(stale, taskmanager_core::core::FailureKind::PermissionDenied),
            cx,
        );
        assert_eq!(
            *view.shell.network_escalation_state(),
            NetworkEscalationState::Loading(RequestCorrelation::Request(first)),
            "a stale lane failure must not clobber the pending pill"
        );
    });

    // The user declines the polkit prompt: the lane reports PermissionDenied
    // for the correlated id → the pill shows the typed failure + retry.
    view.update(cx, |view, cx| {
        view.apply_platform_event_batch(
            escalation_failure_batch(first, taskmanager_core::core::FailureKind::PermissionDenied),
            cx,
        );
        assert_eq!(
            *view.shell.network_escalation_state(),
            NetworkEscalationState::Failed(NetworkEscalationFailed {
                correlation: RequestCorrelation::Request(first),
                failure: taskmanager_core::core::FailureKind::PermissionDenied,
            }),
            "the declined prompt must land in the typed provider-failure state"
        );
    });

    // Click 2 (retry): the pill stays clickable in every state and submits a
    // fresh correlated request.
    view.update(cx, |view, cx| {
        view.request_process_network_escalation(cx);
        assert!(matches!(
            view.shell.network_escalation_state(),
            NetworkEscalationState::Loading(_)
        ));
    });
    let second = submitted
        .lock()
        .expect("submitted ids")
        .pop()
        .expect("retry submission");
    assert_ne!(first, second, "retry must submit a fresh request");

    view.update(cx, |view, cx| {
        view.apply_platform_event_batch(escalated_batch(first), cx);
        assert_eq!(
            *view.shell.network_escalation_state(),
            NetworkEscalationState::Loading(RequestCorrelation::Request(second)),
            "a stale success must not clear the newer pending attempt"
        );
    });

    // The re-authorized capture succeeds: the correlated event clears the pill.
    view.update(cx, |view, cx| {
        view.apply_platform_event_batch(escalated_batch(second), cx);
        assert_eq!(
            *view.shell.network_escalation_state(),
            NetworkEscalationState::Ready(taskmanager_application::NetworkEscalationReady {
                request_id: second,
            })
        );
    });
}

/// Other runtime lane failures (launcher helper missing, lane timeout) carry
/// their typed kinds into the failed state too — the payload is the runtime
/// `FailureKind` verbatim, never a fabricated transport error.
#[gpui::test]
async fn helper_missing_and_timeout_lane_failures_carry_typed_kinds(cx: &mut gpui::TestAppContext) {
    use taskmanager_application::{NetworkEscalationFailed, RequestCorrelation};

    for kind in [
        taskmanager_core::core::FailureKind::MissingDependency,
        taskmanager_core::core::FailureKind::TimedOut,
    ] {
        let (view, submitted) = root_with_escalation_platform(cx);
        view.update(cx, |view, cx| {
            view.request_process_network_escalation(cx);
        });
        let request = submitted
            .lock()
            .expect("submitted ids")
            .pop()
            .expect("escalation submission");
        view.update(cx, |view, cx| {
            view.apply_platform_event_batch(escalation_failure_batch(request, kind), cx);
            assert_eq!(
                *view.shell.network_escalation_state(),
                NetworkEscalationState::Failed(NetworkEscalationFailed {
                    correlation: RequestCorrelation::Request(request),
                    failure: kind,
                }),
                "lane failure {kind:?} must be carried verbatim"
            );
        });
    }
}

/// The failed pill RENDERS: the runtime `Provider` failure state — which only
/// the G-04a failure arm can produce — must expose the clickable retry
/// affordance in the laid-out view, not just exist as unreachable state.
#[gpui::test]
async fn failed_escalation_state_renders_the_clickable_retry_pill(cx: &mut gpui::TestAppContext) {
    use taskmanager_application::{NetworkEscalationFailed, RequestCorrelation};
    use taskmanager_core::core::{DeviceStatus, FailureKind};
    use taskmanager_platform_contract::RequestId;

    let mut snapshot = {
        let ProcessInsightsState::Ready(snapshot) = process_insights_capture_fixture() else {
            panic!("capture fixture must be ready");
        };
        *snapshot
    };
    snapshot.network.traffic_failure = Some(FailureKind::RequiresEscalation);
    snapshot.network.traffic_state =
        DeviceState::default().transition(DeviceStatus::PermissionDenied, 1);
    let entity = cx.new(|cx| crate::gpui_app::root::RootView::new(Theme::dark(), cx));
    let window = cx.add_window(|_window, _cx| FixtureView {
        state: ProcessInsightsState::Ready(Box::new(snapshot)),
        labels: ProcessInsightsLabels::capture_fixture(),
        theme: Theme::dark(),
        net_escalation: NetworkEscalationState::Failed(NetworkEscalationFailed {
            correlation: RequestCorrelation::Request(
                RequestId::new(77).expect("fixture request id"),
            ),
            failure: FailureKind::PermissionDenied,
        }),
        entity: entity.clone(),
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    let pill_bounds = vcx
        .debug_bounds("process-insights-net-escalation")
        .expect("the failed pill must render its retry affordance");
    // The retry affordance stays clickable: clicking re-submits through the
    // same RootView path (no platform client here → typed failure again).
    vcx.simulate_click(pill_bounds.center(), Default::default());
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let escalation = entity.read_with(cx, |view, _| *view.shell.network_escalation_state());
    assert!(
        escalation_failed(escalation),
        "clicking the failed pill must re-submit (degrading to the typed failure \
         in this no-platform harness), never be a dead control"
    );
}
