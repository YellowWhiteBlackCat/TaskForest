//! Apps action-bar behavior: a priority pick is a typed neutral verb.
//!
//! The priority submenu is the frontend's only priority-mapping surface. These
//! tests pin both halves of that mapping — the visible label resolves through
//! the shared tier ladder, and the pick submits the neutral typed
//! [`ProcessBatchAction::SetPriority`] over the control port rather than any
//! frontend-local priority spelling.

use super::ProcessToolbarAction;
use gpui::AppContext;
use std::sync::{Arc, Mutex};
use taskforest_contract::{
    CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
    RequestEnvelope, RequestPort, SubmissionError,
};
use taskforest_core::core::ScalarObservation;
use taskforest_core::core::process::{
    PriorityTier, ProcessBatchAction, ProcessItem, ProcessLiveKey, ProcessScalarObservations,
};
use taskmanager_application::{
    PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle, ProcessControlRequest,
    ProcessFacets, TelemetryRefreshPolicy,
};
use taskmanager_shell::presentation::priority_tier_label;
use taskmanager_telemetry_store::HistoryRetention;
use taskmanager_telemetry_store::TelemetryStore;
use taskmanager_test_support::ProcessItemFixtureBuilder;
use taskmanager_theme::Theme;

/// The three tiers the priority submenu offers, in menu order.
const MENU_TIERS: [PriorityTier; 3] = [PriorityTier::High, PriorityTier::Normal, PriorityTier::Low];

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

/// Records every accepted control payload (single request or batch intent).
struct RecordingControl(Arc<Mutex<Vec<ProcessControlRequest>>>);
impl RequestPort for RecordingControl {
    type Request = ProcessControlRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(request.payload);
        Ok(())
    }
}

/// The menu's priority labels are the shared tier ladder, not a second
/// table: every offered tier resolves through the owner and stays distinct.
#[test]
fn priority_picks_bind_the_shared_tier_ladder() {
    let labeled = MENU_TIERS.map(|tier| ProcessToolbarAction::SetPriority(tier).label());
    assert_eq!(labeled, MENU_TIERS.map(priority_tier_label));
    assert!(labeled.iter().all(|label| !label.trim().is_empty()));
    assert_ne!(labeled[0], labeled[1]);
    assert_ne!(labeled[1], labeled[2]);
}

/// A priority pick over one explicit target submits the neutral typed verb —
/// never a frontend-local priority name and never a single-request substitute.
#[gpui::test]
async fn priority_pick_submits_the_neutral_typed_verb(cx: &mut gpui::TestAppContext) {
    use crate::gpui_app::root::RootView;

    let submitted = Arc::new(Mutex::new(Vec::new()));
    let facets = PlatformFacets::default().with_process(
        ProcessFacets::default().with_control(Arc::new(RecordingControl(submitted.clone()))),
    );
    let client = PlatformClient::new(PlatformHandle::new(
        Arc::new(NoCapabilities),
        Arc::new(NoEvents),
        facets,
    ));
    let (telemetry, ingestor) =
        TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::uniform(60));
    let view = cx.new(|cx| {
        RootView::new_with_platform(
            Theme::dark(),
            telemetry,
            ingestor,
            TelemetryRefreshPolicy::default(),
            client,
            cx,
        )
    });

    let item: ProcessItem = ProcessItemFixtureBuilder::new()
        .pid(44)
        .name("worker-44".to_string())
        .scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(144, 1),
            ..Default::default()
        })
        .build();
    let identity = ProcessLiveKey::from_parts(44, 144).expect("fixture identity");
    view.update(cx, |view, cx| {
        view.replace_processes_for_test(vec![item]);
        view.select_process_single(identity);
        assert!(
            view.process_control_availability().is_ready(),
            "one explicit live target must be actionable"
        );
        ProcessToolbarAction::SetPriority(PriorityTier::High).perform(view, cx);
    });

    let recorded = submitted
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(
        recorded.len(),
        1,
        "one pick must submit exactly one request"
    );
    match &recorded[0] {
        ProcessControlRequest::ExecuteBatch(intent) => assert_eq!(
            intent.action,
            ProcessBatchAction::SetPriority(PriorityTier::High),
            "the pick must carry the typed tier, not a local priority spelling"
        ),
        other => panic!("a priority pick must submit a batch intent, got {other:?}"),
    }
}
