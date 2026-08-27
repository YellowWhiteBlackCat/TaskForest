use std::sync::Arc;

use super::*;
use taskmanager_application::PlatformClient;
use taskmanager_application::{
    CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError, PlatformEvent,
    PlatformFacets, PlatformHandle, RequestEnvelope, RequestPort, ServiceControlRequest,
    ServiceFacets, SubmissionError,
};

/// Minimal recording service-control port (the same mock shape the shell
/// tests use): submissions are recorded, never forwarded.
#[derive(Default)]
struct RecordingServiceControl(std::sync::Mutex<Vec<ServiceControlRequest>>);

impl RequestPort for RecordingServiceControl {
    type Request = ServiceControlRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0.lock().unwrap().push(request.payload);
        Ok(())
    }
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

fn service_control_client(port: Arc<RecordingServiceControl>) -> PlatformClient {
    PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default().with_service(ServiceFacets::default().with_control(port)),
    ))
}

/// Minimal recording directory-usage port (same mock shape): scan lifecycle
/// submissions are recorded, never forwarded (G-13).
#[derive(Default)]
struct RecordingDirectoryUsage(
    std::sync::Mutex<Vec<taskmanager_application::DirectoryUsageRequest>>,
);

impl RequestPort for RecordingDirectoryUsage {
    type Request = taskmanager_application::DirectoryUsageRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0.lock().unwrap().push(request.payload);
        Ok(())
    }
}

fn directory_usage_client(port: Arc<RecordingDirectoryUsage>) -> PlatformClient {
    use taskmanager_application::StorageFacets;
    PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default().with_storage(StorageFacets::default().with_directory_usage(port)),
    ))
}

/// The Disk-device directory-usage action queues the typed effect through the
/// shell lane (G-13): the message resolves Start/Cancel from the selected
/// disk + the shared latest snapshot, freezes the payload through
/// `ShellApp::request_directory_usage`, and the submission reaches the
/// platform's directory-usage port verbatim. A Scanning snapshot toggles to
/// Cancel by its own scan id.
#[test]
fn directory_usage_action_queues_start_then_cancel_through_the_shell_lane() {
    use taskmanager_application::{
        DirectoryScanBounds, DirectoryScanId, DirectoryScanSpec, DirectoryScanStatus,
        DirectoryScanTotals, DirectoryUsageRequest, DirectoryUsageSnapshot,
    };

    let recorded = Arc::new(RecordingDirectoryUsage::default());
    let mut app = IcedApp::new(Some(directory_usage_client(recorded.clone())));
    app.shell = taskmanager_shell::demo_app();
    // The demo fixture's disk carries a disk-level "/" mount and no partition
    // children; select the Disk device so the action resolves against it.
    let _ = app.update(Message::SelectPerfDevice(crate::app::PerfDevice::Disk(0)));

    let _ = app.update(Message::ToggleDirectoryUsageScan);
    let submitted = recorded.0.lock().unwrap();
    assert_eq!(submitted.len(), 1, "exactly one scan submission");
    assert_eq!(
        submitted[0],
        DirectoryUsageRequest::StartScan(DirectoryScanSpec {
            root: "/".to_string(),
            bounds: DirectoryScanBounds::default(),
        }),
        "the start resolves the disk's own mount with the default bounds"
    );
    drop(submitted);

    // An active scan of that disk toggles to Cancel by its own scan id.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::DirectoryUsage(Some(
            DirectoryUsageSnapshot {
                scan_id: DirectoryScanId::new(9),
                root: "/".to_string(),
                status: DirectoryScanStatus::Scanning,
                entries: Vec::new(),
                totals: DirectoryScanTotals::fresh(10),
            },
        )),
    );
    let _ = app.update(Message::ToggleDirectoryUsageScan);
    let submitted = recorded.0.lock().unwrap();
    assert_eq!(submitted.len(), 2);
    assert_eq!(
        submitted[1],
        DirectoryUsageRequest::Cancel(DirectoryScanId::new(9)),
        "the cancel carries the active scan id"
    );
}

#[test]
fn service_control_select_request_confirm_round_trip_reaches_the_port() {
    let recorded = Arc::new(RecordingServiceControl::default());
    let mut app = IcedApp::new(Some(service_control_client(recorded.clone())));
    app.shell = taskmanager_shell::demo_app();
    app.shell.application.active_page = AppPage::Services;

    let _ = app.update(Message::RequestServiceAction {
        index: 4,
        action: ServiceAction::Restart,
    });
    assert_eq!(
        app.shell
            .pending_service_control()
            .map(|target| (target.service_id.as_str(), target.action)),
        Some((
            "fixture.service:demo-failed.service",
            ServiceAction::Restart
        ))
    );
    // Request only opened the confirmation; nothing may reach the port.
    assert!(recorded.0.lock().unwrap().is_empty());

    let _ = app.update(Message::ConfirmServiceControl);
    assert!(app.shell.pending_service_control().is_none());
    let requests = recorded.0.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].service_id.as_str(),
        "fixture.service:demo-failed.service"
    );
    assert_eq!(requests[0].action, ServiceAction::Restart);
    assert_ne!(requests[0].request_id.get(), 0);
    drop(requests);
    assert!(app.shell.feedback_text().contains("queued"));
}

#[test]
fn service_control_cancel_dismisses_without_submitting() {
    let recorded = Arc::new(RecordingServiceControl::default());
    let mut app = IcedApp::new(Some(service_control_client(recorded.clone())));
    app.shell = taskmanager_shell::demo_app();

    let _ = app.update(Message::RequestServiceAction {
        index: 0,
        action: ServiceAction::Stop,
    });
    assert!(app.shell.pending_service_control().is_some());

    let _ = app.update(Message::DismissOverlay);
    assert!(app.shell.pending_service_control().is_none());
    assert!(recorded.0.lock().unwrap().is_empty());
}

#[test]
fn request_service_action_rejects_rows_without_provider_authority() {
    let mut app = IcedApp::demo();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(vec![
            taskmanager_application::ServiceItem::from_inventory(
                "",
                "read-only.service",
                taskmanager_application::ServiceStatus::Active,
                "",
                "",
                "",
                "",
            ),
        ])),
    );
    app.shell.application.active_page = AppPage::Services;

    let _ = app.update(Message::RequestServiceAction {
        index: 0,
        action: ServiceAction::Stop,
    });
    assert!(app.shell.pending_service_control().is_none());
    assert!(app.shell.feedback_text().contains("No service selected"));
}

// --- Performance chart history (G-02/ADR-028) ---------------------------

use taskmanager_application::{
    CpuMetrics, CpuScalarObservations, MemoryMetrics, MemoryScalarObservations, ScalarObservation,
    SystemSnapshot,
};
use taskmanager_shell::history::MetricSeries;

/// Build a snapshot carrying exactly one CPU% and an optional memory%
/// reading at a given compatibility watermark. Mirrors the shell's own
/// `history::tests::snapshot_with` shape so the percentages surface
/// through the real `current_global_usage_pct` / `used_percentage_observed`
/// accessors rather than being wired around them.
fn perf_snapshot(cpu: f32, memory_pct: Option<f32>, timestamp_ms: u64) -> SystemSnapshot {
    let memory = match memory_pct {
        Some(pct) => MemoryMetrics::from_observations(
            MemoryScalarObservations {
                total_bytes: ScalarObservation::available(100, timestamp_ms),
                used_bytes: ScalarObservation::available(pct as u64, timestamp_ms),
                ..Default::default()
            },
            Default::default(),
        ),
        None => MemoryMetrics::default(),
    };
    SystemSnapshot {
        timestamp_ms,
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(cpu, timestamp_ms),
            ..Default::default()
        }),
        memory,
        ..SystemSnapshot::default()
    }
}

/// The headline chart reads the SHARED shell history (G-02): each advancing
/// snapshot recorded through the shell's own store surfaces once in both
/// system-wide series the chart and its summary lines render.
#[test]
fn headline_chart_reads_one_finite_point_per_snapshot_from_the_shared_store() {
    let mut app = IcedApp::new(None);
    // A fresh launch has no history yet — the chart shows its honest
    // placeholder, never an invented point.
    assert!(
        app.shell
            .history
            .series(MetricSeries::CpuUsagePercent)
            .is_empty()
    );

    for (cpu, memory_pct, ts) in [
        (12.0, Some(40.0), 1),
        (60.0, Some(55.0), 2),
        (33.0, Some(70.0), 3),
    ] {
        let snapshot = perf_snapshot(cpu, memory_pct, ts);
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &snapshot,
            None,
            None,
        );
        taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
        );
    }
    assert_eq!(
        app.shell.history.series(MetricSeries::CpuUsagePercent),
        vec![12.0, 60.0, 33.0]
    );
    assert_eq!(
        app.shell.history.series(MetricSeries::MemoryUsagePercent),
        vec![40.0, 55.0, 70.0]
    );
}

/// The per-process ring (the overlay window, NOT the headline series) still
/// samples locally, once per advancing snapshot watermark: repeated ticks
/// while a refresh is in flight must not inflate the ring.
#[test]
fn process_ring_samples_once_per_advancing_snapshot_watermark() {
    let mut app = IcedApp::new(None);
    app.shell.application.active_page = AppPage::Applications;
    let mut trusted = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(42)
        .name("sampled".into())
        .current_cpu_percentage(25.0)
        .current_memory_bytes(1_024)
        .current_start_time_secs(1_785_290_000)
        .build();
    // The overlay path freezes a trustworthy identity; without an available
    // start token the properties overlay refuses to open (fixture mirrors the
    // shared demo shell's process shape).
    let mut observations = *trusted.scalar_observations();
    observations.start_token = taskmanager_application::ScalarObservation::available(420_001, 1);
    trusted.apply_scalar_observations(observations);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![trusted])),
    );
    assert!(app.shell.select_row(0));
    let _ = app.shell.apply_action(AppAction::OpenProperties);
    assert!(app.process_properties_open());

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(perf_snapshot(
            50.0,
            Some(50.0),
            42,
        )))),
    );
    app.sample_process_history();
    // Two more ticks arrive before the next snapshot lands — the ring must
    // not inflate on the same timestamp watermark.
    app.sample_process_history();
    app.sample_process_history();
    let ring = app.process_perf_history().expect("ring created on overlay");
    assert_eq!(ring.pid(), 42);
    assert_eq!(ring.cpu_samples(), vec![25.0]);
    assert_eq!(ring.memory_samples(), vec![1_024.0]);
}

#[test]
fn process_property_series_reuse_until_the_ring_revision_advances() {
    let mut app = IcedApp::new(None);
    app.shell.application.active_page = AppPage::Applications;
    let mut trusted = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(42)
        .name("sampled".into())
        .current_cpu_percentage(25.0)
        .current_memory_bytes(1_024)
        .current_start_time_secs(1_785_290_000)
        .build();
    let mut observations = *trusted.scalar_observations();
    observations.start_token = taskmanager_application::ScalarObservation::available(420_001, 1);
    trusted.apply_scalar_observations(observations);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![trusted])),
    );
    assert!(app.shell.select_row(0));
    let _ = app.shell.apply_action(AppAction::OpenProperties);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(perf_snapshot(
            50.0,
            Some(50.0),
            42,
        )))),
    );
    app.sample_process_history();

    let first = app
        .process_perf_series()
        .expect("the open properties ring has a series snapshot");
    let second = app
        .process_perf_series()
        .expect("the cached series remains available");
    assert!(std::rc::Rc::ptr_eq(&first.cpu, &second.cpu));
    assert!(std::rc::Rc::ptr_eq(&first.memory, &second.memory));
    assert!(std::rc::Rc::ptr_eq(&first.disk_read, &second.disk_read));
    assert!(std::rc::Rc::ptr_eq(&first.disk_write, &second.disk_write));

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(perf_snapshot(
            60.0,
            Some(55.0),
            43,
        )))),
    );
    app.sample_process_history();
    let changed = app
        .process_perf_series()
        .expect("a new sample keeps the series available");
    assert!(
        !std::rc::Rc::ptr_eq(&first.cpu, &changed.cpu),
        "a new ring revision must rebuild the contiguous snapshot"
    );
    assert_eq!(&*changed.cpu, &[25.0, 25.0]);
}

/// The Performance page renders end-to-end from the shared series: the
/// collecting branch first, then the populated chart once the store carries
/// enough samples for a strokeable polyline.
#[test]
fn performance_page_renders_the_chart_once_history_is_populated() {
    let mut app = IcedApp::new(None);
    app.shell.application.active_page = AppPage::Performance;
    // Empty store: the view builds and selects the collecting branch.
    {
        let _placeholder_view = crate::ui::view(&app);
    }

    // Feed enough samples for a strokeable polyline on both series.
    for (cpu, mem, ts) in [(10.0, 50.0, 1), (35.0, 52.0, 2), (70.0, 49.0, 3)] {
        let snapshot = perf_snapshot(cpu, Some(mem), ts);
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &snapshot,
            None,
            None,
        );
        taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
        );
    }
    assert!(
        app.shell
            .history
            .series(MetricSeries::CpuUsagePercent)
            .len()
            >= 2
    );

    // The populated Performance page builds end-to-end through the canvas
    // widget — the element tree constructs without panic.
    let _chart_view = crate::ui::view(&app);
}
