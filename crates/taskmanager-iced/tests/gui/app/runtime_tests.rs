use std::collections::VecDeque;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use super::*;

#[test]
fn instance_activation_is_drained_without_blocking_and_coalesced() {
    let mut app = IcedApp::default();
    let (sender, receiver) = channel();
    app.install_instance_runtime(None, Some(receiver));
    sender
        .send(taskmanager_platform_contract::InstanceEvent::Activate)
        .expect("test instance receiver is live");
    sender
        .send(taskmanager_platform_contract::InstanceEvent::Activate)
        .expect("test instance receiver is live");

    assert!(app.runtime.drain_instance_events());
    assert!(!app.runtime.drain_instance_events());
}

#[test]
fn lifecycle_event_drains_are_bounded_and_converge_across_ticks() {
    let mut runtime = IcedRuntime::new(None);
    let (instance_sender, instance_receiver) = channel();
    runtime.install_instance(None, Some(instance_receiver));
    for _ in 0..=MAX_LIFECYCLE_EVENTS_PER_TICK {
        instance_sender
            .send(taskmanager_platform_contract::InstanceEvent::Activate)
            .expect("test instance receiver is live");
    }

    assert!(runtime.drain_instance_events(), "first bounded batch");
    assert!(
        runtime.drain_instance_events(),
        "the overflow remains queued for the next tick"
    );
    assert!(
        !runtime.drain_instance_events(),
        "finite input eventually converges"
    );

    let (tray_sender, tray_receiver) = channel();
    runtime.install_tray(None, Some(tray_receiver));
    for _ in 0..=MAX_LIFECYCLE_EVENTS_PER_TICK {
        tray_sender
            .send(TrayEvent::IconActivated)
            .expect("test tray receiver is live");
    }

    assert_eq!(
        runtime.drain_tray_events().len(),
        MAX_LIFECYCLE_EVENTS_PER_TICK
    );
    assert_eq!(
        runtime.drain_tray_events(),
        [TrayEvent::IconActivated],
        "the overflow remains queued for the next tick"
    );
    assert!(
        runtime.drain_tray_events().is_empty(),
        "finite input eventually converges"
    );
}

#[test]
fn foreground_activation_request_is_named_coalesced_and_one_shot() {
    let mut app = IcedApp::default();

    assert!(!app.take_activation_request());
    app.runtime.request_activation();
    app.runtime.request_activation();
    assert!(app.take_activation_request());
    assert!(!app.take_activation_request());
}

#[test]
fn no_platform_reports_submission_failure_and_still_runs_tick_finish() {
    let mut app = IcedApp::demo();
    let first = app
        .shell
        .projection()
        .processes
        .as_ref()
        .and_then(|processes| processes.first())
        .cloned()
        .expect("demo process");
    let identity = taskmanager_core::core::process::FrozenProcessIdentity::from_process(&first)
        .expect("demo process carries identity");
    app.shell.application.selected_process = Some(identity.clone());
    let _ = app.shell.open_process_properties_for(identity);

    app.queue(taskmanager_application::PlatformEffect::Refresh(
        taskmanager_application::RefreshRequest::Processes,
    ));
    assert_eq!(
        app.shell.feedback_text(),
        "Demo mode suppresses platform actions"
    );
    assert!(app.process_perf_history().is_none());

    app.tick();
    assert!(
        app.process_perf_history().is_some(),
        "view-local sampling runs even without a platform owner"
    );

    let committed_snapshot = app.shell.projection().snapshot.clone();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(None)),
    );
    assert!(
        app.shell.telemetry_frame_state().is_collecting(),
        "missing first snapshot exercises frontend-local motion finalization"
    );
    app.tick();
    assert!(
        app.warmup_spin_phase().is_some(),
        "the same finish phase advances frontend-local motion without a platform"
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(committed_snapshot)),
    );
}

#[derive(Default)]
struct EmptyCapabilities;

impl taskmanager_platform_contract::CapabilityCatalog for EmptyCapabilities {
    fn snapshot(&self) -> taskmanager_platform_contract::CapabilitySnapshot {
        taskmanager_platform_contract::CapabilitySnapshot::default()
    }
}

#[derive(Default)]
struct QueuedEvents(
    Mutex<
        VecDeque<
            taskmanager_platform_contract::EventEnvelope<taskmanager_application::PlatformEvent>,
        >,
    >,
);

impl taskmanager_platform_contract::EventPort for QueuedEvents {
    type Event = taskmanager_application::PlatformEvent;

    fn try_recv(
        &self,
    ) -> Result<
        Option<taskmanager_platform_contract::EventEnvelope<Self::Event>>,
        taskmanager_platform_contract::EventPortError,
    > {
        Ok(self.0.lock().expect("queue lock").pop_front())
    }
}

#[test]
fn tick_drains_the_directly_owned_platform_before_view_local_finish() {
    let events = Arc::new(QueuedEvents::default());
    events
        .0
        .lock()
        .expect("queue lock")
        .push_back(taskmanager_platform_contract::EventEnvelope {
            request_id: taskmanager_platform_contract::RequestId::new(1)
                .expect("non-zero request id"),
            capability: taskmanager_platform_contract::CapabilityId::PROCESS_LIST,
            provider: None,
            sequence: taskmanager_platform_contract::EventSequence::new(1),
            observed_at_ms: 100,
            outcome: Ok(taskmanager_application::PlatformEvent::Processes(
                taskmanager_application::ProcessEvent::Snapshot(std::sync::Arc::new(vec![
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(77)
                        .name("runtime-owned".into())
                        .build(),
                ])),
            )),
        });
    let client =
        taskmanager_application::PlatformClient::new(taskmanager_application::PlatformHandle::new(
            Arc::new(EmptyCapabilities),
            events,
            taskmanager_application::PlatformFacets::default(),
        ));
    let mut app = IcedApp::new(Some(client));

    app.tick();

    assert_eq!(
        app.shell
            .projection()
            .processes
            .as_deref()
            .map(|rows| rows[0].pid),
        Some(77)
    );
}
