use super::{MenuControlRequest, menu_control_submission};
use gpui::AppContext;
use std::sync::{Arc, Mutex};

use taskmanager_application::{
    PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle, ProcessControlRequest,
    ProcessFacets, TelemetryRefreshPolicy,
};
use taskmanager_core::core::process::ProcessSignal;
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
    RequestEnvelope, RequestPort, SubmissionError,
};

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

/// Records every accepted control payload (the GPUI analogue of the shell
/// correlation tests' `RecordingRequests`).
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

fn fixture_process(pid: u32) -> taskmanager_core::core::process::ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .parent_pid(None)
        .name("fixture-worker".into())
        .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
            start_token: taskmanager_core::core::ScalarObservation::available(1_000, 1),
            ..Default::default()
        })
        .current_start_time_secs(100)
        .build()
}

#[test]
fn menu_control_vocabulary_maps_to_neutral_requests() {
    let target = taskmanager_core::core::process::FrozenProcessIdentity::from_authoritative_parts(
        42,
        "fixture-worker".to_string(),
        100,
        1_000,
    )
    .expect("valid fixture identity");

    assert_eq!(
        menu_control_submission(MenuControlRequest::Suspend, target.clone()),
        (
            ProcessControlRequest::Suspend {
                target: target.clone(),
            },
            super::ProcessControlAction::Suspend,
        )
    );
    assert_eq!(
        menu_control_submission(MenuControlRequest::Resume, target.clone()),
        (
            ProcessControlRequest::Resume {
                target: target.clone(),
            },
            super::ProcessControlAction::Resume,
        )
    );
    assert_eq!(
        menu_control_submission(MenuControlRequest::Signal(ProcessSignal::Hangup), target),
        (
            ProcessControlRequest::SendSignal {
                target: taskmanager_core::core::process::FrozenProcessIdentity::from_authoritative_parts(
                    42,
                    "fixture-worker".to_string(),
                    100,
                    1_000,
                )
                .expect("valid fixture identity"),
                signal: ProcessSignal::Hangup,
            },
            super::ProcessControlAction::Signal(ProcessSignal::Hangup),
        )
    );
}

/// Full menu dispatch through a recording platform client: the Suspend /
/// Resume menu items must submit the NEUTRAL `ProcessControlRequest`
/// variants (§8.1), never `SendSignal(Stop/Continue)`.
#[gpui::test]
async fn menu_suspend_resume_submit_the_neutral_request(cx: &mut gpui::TestAppContext) {
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
        taskmanager_telemetry_store::TelemetryStore::shared_with_correlated_ingestion(60);
    let view = cx.new(|cx| {
        super::RootView::new_with_platform(
            taskmanager_theme::Theme::dark(),
            telemetry,
            ingestor,
            TelemetryRefreshPolicy::default(),
            client,
            cx,
        )
    });

    let item = fixture_process(42);
    let target = taskmanager_core::core::process::FrozenProcessIdentity::from_process(&item)
        .expect("fixture carries an authoritative start token");
    let cases = [
        (
            crate::gpui_app::root::ProcMenuAction::Suspend,
            ProcessControlRequest::Suspend {
                target: target.clone(),
            },
        ),
        (
            crate::gpui_app::root::ProcMenuAction::Resume,
            ProcessControlRequest::Resume {
                target: target.clone(),
            },
        ),
        (
            crate::gpui_app::root::ProcMenuAction::Signal(ProcessSignal::Hangup),
            ProcessControlRequest::SendSignal {
                target: target.clone(),
                signal: ProcessSignal::Hangup,
            },
        ),
    ];
    for (menu, _) in &cases {
        view.update(cx, |view, cx| {
            view.replace_processes_for_test(vec![item.clone()]);
            view.apply_proc_action(42, *menu, cx);
        });
    }

    let recorded = submitted
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(
        *recorded,
        cases
            .into_iter()
            .map(|(_, request)| request)
            .collect::<Vec<_>>(),
        "the menu must submit the neutral Suspend/Resume request vocabulary"
    );
}
