//! Directory-usage scan trigger: the `d` key on the Disk
//! device toggles StartScan / Cancel through the SHARED effect seam (G-03) —
//! `ShellApp::request_directory_usage` wraps the typed request in
//! `PlatformEffect::DirectoryUsage`, which the runtime routes through
//! `queue_effect` exactly like every other on-demand lane (the old direct
//! `PlatformClient::submit_directory_usage` bypass is gone).

use super::super::*;

use taskmanager_application::{AppAction, AppPage, DirectoryUsageRequest};
use taskmanager_core::core::directory_usage::{
    DirectoryScanBounds, DirectoryScanId, DirectoryScanStatus, DirectoryScanTotals,
    DirectoryUsageSnapshot,
};

/// Helper: place the app on the Performance page's Disk device.
fn on_disk_device(app: &mut crate::TuiApp) {
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    app.perf_device = crate::PerfDevice::Disk;
}

/// Pressing `d` on the Disk device yields the `DirectoryUsage(StartScan)`
/// effect for the first mounted partition (the demo disk mounts `/`), built
/// through the shell's request helper. The request carries the default
/// bounds — the UI never customizes depth/entry caps, mirroring GPUI.
#[test]
fn d_on_disk_device_yields_directory_usage_start_scan_for_first_mount_point() {
    let mut app = crate::demo_app();
    on_disk_device(&mut app);
    // No active scan → the toggle must request a StartScan.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::DirectoryUsage(None),
    );

    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('d'),
            KeyModifiers::NONE,
        ),
    );

    let Some(PlatformEffect::DirectoryUsage(request)) = effect else {
        panic!("`d` must yield the DirectoryUsage effect, got {effect:?}");
    };
    let DirectoryUsageRequest::StartScan(spec) = &request else {
        panic!("expected StartScan, got {request:?}");
    };
    // The demo disk's only partition mounts at `/`.
    assert_eq!(spec.root, "/", "scan root must be the first mount point");
    assert_eq!(
        spec.bounds,
        DirectoryScanBounds::default(),
        "bounds must be the default policy (GPUI parity)"
    );
}

/// Pressing `d` while a scan is `Scanning` yields the `DirectoryUsage(Cancel)`
/// effect for that scan id — the keyboard equivalent of GPUI's conditional
/// cancel pill. The scan state comes from the SHARED `ShellData` slot the
/// platform batch fold fills.
#[test]
fn d_while_scanning_yields_directory_usage_cancel_for_the_active_scan_id() {
    let mut app = crate::demo_app();
    on_disk_device(&mut app);
    // Seed an active Scanning snapshot with a known scan id in the shared slot.
    let scan_id = DirectoryScanId::new(42);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::DirectoryUsage(Some(
            DirectoryUsageSnapshot {
                scan_id,
                root: "/".into(),
                status: DirectoryScanStatus::Scanning,
                entries: Vec::new(),
                totals: DirectoryScanTotals::fresh(10),
            },
        )),
    );

    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('d'),
            KeyModifiers::NONE,
        ),
    );

    let Some(PlatformEffect::DirectoryUsage(request)) = effect else {
        panic!("`d` must yield the DirectoryUsage effect, got {effect:?}");
    };
    let DirectoryUsageRequest::Cancel(cancelled_id) = &request else {
        panic!("expected Cancel, got {request:?}");
    };
    assert_eq!(*cancelled_id, scan_id, "must cancel the active scan id");
}

/// `d` is a no-op off the Disk device: the Cpu device must yield no effect.
#[test]
fn d_on_cpu_device_yields_nothing() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    app.perf_device = crate::PerfDevice::Cpu;
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('d'),
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none(), "`d` must be a no-op off the Disk device");
}

/// The full round-trip through the shared seam: `d` → DirectoryUsage effect →
/// `queue_effect` → `submit_directory_usage` → the provider port receives the
/// typed `StartScan`. Proves the TUI's `d` key rides the same application
/// lane every on-demand effect uses — no frontend-owned platform bypass.
#[test]
fn directory_scan_round_trips_through_queue_effect_to_the_provider() {
    use std::sync::{Arc, Mutex};
    use taskmanager_application::{PlatformEvent, PlatformFacets, PlatformHandle, StorageFacets};
    use taskmanager_platform_contract::{
        CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
        RequestEnvelope, RequestPort, SubmissionError,
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

    #[derive(Default)]
    struct RecordingDirectoryUsage(Mutex<Vec<DirectoryUsageRequest>>);
    impl RequestPort for RecordingDirectoryUsage {
        type Request = DirectoryUsageRequest;
        fn try_submit(
            &self,
            request: RequestEnvelope<Self::Request>,
        ) -> Result<(), SubmissionError> {
            self.0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(request.payload);
            Ok(())
        }
    }

    let recorded = Arc::new(RecordingDirectoryUsage::default());
    let mut client = taskmanager_application::PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default()
            .with_storage(StorageFacets::default().with_directory_usage(recorded.clone())),
    ));

    let mut app = crate::demo_app();
    on_disk_device(&mut app);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::DirectoryUsage(None),
    );

    // The key yields the typed effect; the runtime queues it — exactly what
    // the live loop does with every handle_key result.
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('d'),
            KeyModifiers::NONE,
        ),
    )
    .expect("StartScan effect yielded");
    taskmanager_shell::queue_effect(&mut app.shell, &mut client, effect);

    let submitted = recorded
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(submitted.len(), 1, "exactly one request submitted");
    let DirectoryUsageRequest::StartScan(spec) = &submitted[0] else {
        panic!("provider must receive StartScan: {:?}", submitted[0]);
    };
    assert_eq!(spec.root, "/", "the scan root must reach the provider");
    assert!(
        spec.bounds.max_depth > 0 && spec.bounds.max_entries > 0,
        "the default bounds must reach the provider: {:?}",
        spec.bounds
    );
}
