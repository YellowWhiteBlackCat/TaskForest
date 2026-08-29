//! Dispatch coverage for the nine typed on-demand effect lanes (G-03/G-19):
//! each new `PlatformEffect` variant must reach exactly the matching
//! `PlatformClient` submit method with its payload intact — the same
//! recording-port pattern the twelve original arms use
//! (`tests/service_control.rs`).
use super::super::*;
use std::sync::{Arc, Mutex};
use taskmanager_application::{
    AppPage, CommandLaunchRequest, DirectoryUsageRequest, GpuEngineRowsRequest, IntegrationFacets,
    PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle, ProcessAffinityControlRequest,
    ProcessAffinityRequest, ProcessFacets, ProcessNetworkEscalationRequest,
    ProcessResourceControlRequest, ServiceDependenciesRequest, ServiceFacets,
    ServiceLogSnapshotRequest, SetupScriptRequest, SmartControlRequest, StorageFacets,
    SystemFacets,
};
use taskmanager_core::core::directory_usage::{
    DirectoryScanBounds, DirectoryScanId, DirectoryScanSpec,
};
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_core::core::process_telemetry::ResourceGroupLimitRequest;
use taskmanager_core::core::setup::SetupScriptAction;
use taskmanager_core::core::storage::StorageDeviceTarget;
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityRequest, CapabilitySnapshot, EventEnvelope, EventPort,
    EventPortError, RequestEnvelope, RequestPort, SubmissionError,
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

/// Records every accepted payload on one typed lane.
struct RecordingRequests<T> {
    submitted: Mutex<Vec<T>>,
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
            .push(request.payload);
        Ok(())
    }
}

fn recorded<T: Clone>(recorder: &RecordingRequests<T>) -> Vec<T> {
    recorder
        .submitted
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// The optional open-files insight facet's submission result must reach the
/// shell's error reporter like the five required facets — an absent lane is
/// honest status-line material, never a silently dropped error.
#[test]
fn process_insights_collects_the_optional_open_files_result_too() {
    // No process insight facets at all: every submission result must be
    // reported in order, so the LAST reported error is the optional
    // open-files facet — proving the dispatch arm collects it. Before the
    // fix the arm dropped `open_files`, and the last error was `threads`.
    let mut client = client_with(PlatformFacets::default());
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    let target = selected_demo_identity(&mut app);

    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::ProcessInsights(target),
    );

    assert!(
        app.feedback_text().contains("process.insights.open_files"),
        "the open-files submission error must be the last one reported, got: {}",
        app.feedback_text()
    );
}

fn client_with(facets: PlatformFacets) -> PlatformClient {
    PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        facets,
    ))
}

#[test]
fn directory_usage_effect_submits_the_scan_lifecycle_request() {
    let recorder = Arc::new(RecordingRequests::<DirectoryUsageRequest>::default());
    let mut client = client_with(
        PlatformFacets::default()
            .with_storage(StorageFacets::default().with_directory_usage(recorder.clone())),
    );
    let mut app = crate::demo_app();
    let request = DirectoryUsageRequest::StartScan(DirectoryScanSpec {
        root: "/var".to_owned(),
        bounds: DirectoryScanBounds::default(),
    });

    queue_effect(
        &mut app,
        &mut client,
        ShellApp::request_directory_usage(request.clone()),
    );

    assert_eq!(recorded(&recorder), vec![request]);
    assert!(
        app.feedback_text()
            .contains("Directory usage scan queued for /var"),
        "queued status must name the scan root: {}",
        app.feedback_text()
    );
}

#[test]
fn gpu_engine_rows_effect_begins_the_typed_request_session() {
    let recorder = Arc::new(RecordingRequests::<GpuEngineRowsRequest>::default());
    let mut client = client_with(
        PlatformFacets::default()
            .with_system(SystemFacets::default().with_gpu_engine_rows(recorder.clone())),
    );
    let mut app = crate::demo_app();
    let device_id = DeviceId::new("gpu:fixture");

    queue_effect(
        &mut app,
        &mut client,
        ShellApp::request_gpu_engine_rows(device_id.clone()),
    );

    assert_eq!(
        recorded(&recorder),
        vec![GpuEngineRowsRequest {
            device_id: device_id.clone()
        }]
    );
    assert!(matches!(
        app.gpu_engine_rows_state(),
        taskmanager_application::GpuEngineRowsState::Loading {
            device_id: pending,
            ..
        } if pending == &device_id
    ));
}

#[test]
fn network_escalation_effect_begins_the_typed_request_session() {
    let recorder = Arc::new(RecordingRequests::<ProcessNetworkEscalationRequest>::default());
    let mut client = client_with(
        PlatformFacets::default()
            .with_process(ProcessFacets::default().with_network_escalation(recorder.clone())),
    );
    let mut app = crate::demo_app();

    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::ProcessNetworkEscalation,
    );

    assert_eq!(recorded(&recorder), vec![ProcessNetworkEscalationRequest]);
    assert!(matches!(
        app.network_escalation_state(),
        taskmanager_application::NetworkEscalationState::Loading(
            taskmanager_application::RequestCorrelation::Request(_)
        )
    ));
}

#[test]
fn smart_control_effect_submits_the_typed_control_request() {
    let recorder = Arc::new(RecordingRequests::<SmartControlRequest>::default());
    let mut client = client_with(
        PlatformFacets::default()
            .with_storage(StorageFacets::default().with_smart_control(recorder.clone())),
    );
    let mut app = crate::demo_app();
    let request = SmartControlRequest::StopTracking(StorageDeviceTarget::default());

    queue_effect(
        &mut app,
        &mut client,
        ShellApp::request_smart_control(request.clone()),
    );

    assert_eq!(recorded(&recorder), vec![request]);
    assert!(
        app.feedback_text().contains("SMART tracking stop queued"),
        "{}",
        app.feedback_text()
    );
}

#[test]
fn service_dependencies_effect_submits_the_service_scoped_request() {
    let recorder = Arc::new(RecordingRequests::<ServiceDependenciesRequest>::default());
    let mut client = client_with(
        PlatformFacets::default()
            .with_service(ServiceFacets::default().with_dependencies(recorder.clone())),
    );
    let mut app = crate::demo_app();
    let service_id = ServiceId::new("fixture.service:NetworkManager.service");

    queue_effect(
        &mut app,
        &mut client,
        ShellApp::request_service_dependencies(service_id.clone()),
    );

    assert_eq!(
        recorded(&recorder),
        vec![ServiceDependenciesRequest {
            service_id: service_id.clone()
        }]
    );
    assert!(
        app.feedback_text()
            .contains("Service dependencies queued for")
            && app.feedback_text().contains(service_id.as_str()),
        "{}",
        app.feedback_text()
    );
}

#[test]
fn service_log_snapshot_effect_submits_the_service_scoped_request() {
    let recorder = Arc::new(RecordingRequests::<ServiceLogSnapshotRequest>::default());
    let mut client = client_with(
        PlatformFacets::default()
            .with_service(ServiceFacets::default().with_log_snapshot(recorder.clone())),
    );
    let mut app = crate::demo_app();
    let service_id = ServiceId::new("fixture.service:NetworkManager.service");

    queue_effect(
        &mut app,
        &mut client,
        ShellApp::request_service_log_snapshot(service_id.clone()),
    );

    assert_eq!(
        recorded(&recorder),
        vec![ServiceLogSnapshotRequest {
            service_id: service_id.clone()
        }]
    );
    assert!(
        app.feedback_text()
            .contains("Service log snapshot queued for")
            && app.feedback_text().contains(service_id.as_str()),
        "{}",
        app.feedback_text()
    );
}

fn selected_demo_identity(app: &mut ShellApp) -> FrozenProcessIdentity {
    app.application.active_page = AppPage::Applications;
    app.selected = 1;
    app.selected_process_identity()
        .expect("demo process selection has an authoritative identity")
}

#[test]
fn process_affinity_effect_submits_the_read_and_begins_correlation() {
    let recorder = Arc::new(RecordingRequests::<ProcessAffinityRequest>::default());
    let mut client = client_with(
        PlatformFacets::default()
            .with_process(ProcessFacets::default().with_affinity(recorder.clone())),
    );
    let mut app = crate::demo_app();
    let target = selected_demo_identity(&mut app);

    let effect = app
        .request_process_affinity()
        .expect("selected identity produces an affinity read");
    queue_effect(&mut app, &mut client, effect);

    assert_eq!(
        recorded(&recorder),
        vec![ProcessAffinityRequest {
            target: target.clone()
        }]
    );
    assert!(
        app.feedback_text().contains("Process affinity read queued"),
        "{}",
        app.feedback_text()
    );
    // The read is pending correlation so a later snapshot can land (the
    // fail-closed acceptance is covered in tests/process_control.rs).
    assert!(matches!(
        app.process_affinity_state(),
        taskmanager_application::ProcessAffinityState::Loading { target: pending, .. }
            if pending == &target
    ));
}

#[test]
fn process_affinity_control_effect_submits_the_write() {
    let recorder = Arc::new(RecordingRequests::<ProcessAffinityControlRequest>::default());
    let mut client = client_with(
        PlatformFacets::default()
            .with_process(ProcessFacets::default().with_affinity_control(recorder.clone())),
    );
    let mut app = crate::demo_app();
    let target = selected_demo_identity(&mut app);

    let effect = app
        .request_process_affinity_control(vec![0, 2])
        .expect("selected identity produces an affinity control request");
    queue_effect(&mut app, &mut client, effect);

    assert_eq!(
        recorded(&recorder),
        vec![ProcessAffinityControlRequest {
            target: target.clone(),
            cpus: vec![0, 2],
        }]
    );
    assert!(
        app.feedback_text().contains("Process affinity set queued"),
        "{}",
        app.feedback_text()
    );
}

#[test]
fn command_launch_effect_submits_through_the_integration_port() {
    let recorder = Arc::new(RecordingRequests::<CommandLaunchRequest>::default());
    let mut client = client_with(
        PlatformFacets::default()
            .with_integration(IntegrationFacets::default().with_command_launch(recorder.clone())),
    );
    let mut app = crate::demo_app();
    let request = CommandLaunchRequest {
        command: "xdg-open /usr/share/doc".to_owned(),
    };

    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::CommandLaunch(request.clone()),
    );

    assert_eq!(recorded(&recorder), vec![request]);
    assert!(matches!(
        app.shell_ui_action_state(),
        taskmanager_application::ShellUiActionState::Loading {
            intent: taskmanager_application::ShellUiActionIntent::Command(_),
            ..
        }
    ));
    assert!(
        app.feedback_text().contains("Command launch queued"),
        "{}",
        app.feedback_text()
    );
}

#[test]
fn setup_script_effect_submits_the_typed_action() {
    let recorder = Arc::new(RecordingRequests::<SetupScriptRequest>::default());
    let mut client = client_with(
        PlatformFacets::default()
            .with_integration(IntegrationFacets::default().with_setup_script(recorder.clone())),
    );
    let mut app = crate::demo_app();
    let request = SetupScriptRequest {
        action: SetupScriptAction::Revert,
    };

    queue_effect(&mut app, &mut client, PlatformEffect::SetupScript(request));

    assert_eq!(
        recorded(&recorder),
        vec![SetupScriptRequest {
            action: SetupScriptAction::Revert
        }]
    );
    assert!(
        app.feedback_text().contains("Setup script Revert queued"),
        "{}",
        app.feedback_text()
    );
}

#[test]
fn resource_group_control_effect_submits_the_limit_writes() {
    let recorder = Arc::new(RecordingRequests::<ProcessResourceControlRequest>::default());
    let mut client = client_with(
        PlatformFacets::default()
            .with_process(ProcessFacets::default().with_resource_control(recorder.clone())),
    );
    let mut app = crate::demo_app();
    let target = selected_demo_identity(&mut app);
    let request = ProcessResourceControlRequest {
        target: target.clone(),
        limits: ResourceGroupLimitRequest::default(),
    };

    queue_effect(
        &mut app,
        &mut client,
        PlatformEffect::ResourceGroupControl(request),
    );

    assert_eq!(
        recorded(&recorder),
        vec![ProcessResourceControlRequest {
            target,
            limits: ResourceGroupLimitRequest::default(),
        }]
    );
    assert!(
        app.feedback_text()
            .contains("Process resource limits queued"),
        "{}",
        app.feedback_text()
    );
}

#[test]
fn on_demand_submission_failure_reports_the_typed_capability() {
    // No facets at all: the client must return the typed
    // UnsupportedCapability error and queue_effect must surface it instead of
    // claiming success.
    let mut client = client_with(PlatformFacets::default());
    let mut app = crate::demo_app();

    queue_effect(
        &mut app,
        &mut client,
        ShellApp::request_directory_usage(DirectoryUsageRequest::Cancel(DirectoryScanId::new(4))),
    );

    assert!(
        !app.feedback_text().contains("queued"),
        "a rejected submission must not report queued: {}",
        app.feedback_text()
    );
    assert!(
        app.feedback_text().contains("filesystem.directory.usage"),
        "the typed capability must surface: {}",
        app.feedback_text()
    );
}
