//! The Alerts observer emits a typed effect; only the PreUpdate drain submits it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::{App, PreUpdate};
use taskmanager_application::{
    AlertRuleImportMode, DesktopNotificationRequest, IntegrationFacets, ManagedAlertRule,
    ManagedAlertRuleEdit, PlatformClient, PlatformEffect, PlatformEvent, PlatformEventBatch,
    PlatformFacets, PlatformHandle,
};
use taskmanager_core::core::alerts::{AlertMetric, AlertRule, AlertSeverity, NotificationPolicy};
use taskmanager_core::core::metrics::{CpuMetrics, CpuScalarObservations, ScalarObservation};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
    RequestEnvelope, RequestPort, SubmissionError,
};
use taskmanager_shell::{ShellApp, fixture};

use crate::app::{FrontendTrack, SharedRuntimeHandle};
use crate::drain::{FeedbackCache, drain_system};
use crate::input::PendingEffects;
use crate::runtime::SharedRuntime;

#[derive(Default)]
struct Notifications(Mutex<Vec<DesktopNotificationRequest>>);

impl RequestPort for Notifications {
    type Request = DesktopNotificationRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .expect("notification recorder lock")
            .push(request.payload);
        Ok(())
    }
}

struct Capabilities;

impl CapabilityCatalog for Capabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::default()
    }
}

struct Events;

impl EventPort for Events {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

fn app_with_notifications(shell: ShellApp, notifications: Arc<Notifications>) -> App {
    let client = PlatformClient::new(PlatformHandle::new(
        Arc::new(Capabilities),
        Arc::new(Events),
        PlatformFacets::default().with_integration(
            IntegrationFacets::default().with_desktop_notification(notifications),
        ),
    ));
    let shared = Box::leak(Box::new(SharedRuntime::new(client)));
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.init_resource::<PendingEffects>();
    app.init_resource::<FeedbackCache>();
    app.insert_resource(SharedRuntimeHandle { shared });
    app.insert_non_send(FrontendTrack {
        shell,
        initial_refresh_submitted: true,
        process_tree_expansion: Default::default(),
    });
    app.add_observer(super::alerts_fold_observer);
    app.add_systems(PreUpdate, drain_system);
    app
}

#[test]
fn alerts_fold_observer_queues_notification_until_drain_submits_it() {
    let notifications = Arc::new(Notifications::default());
    let mut shell = crate::demo_fixture::demo_shell();
    shell.set_alert_policy(NotificationPolicy {
        enabled: true,
        cooldown_ms: 0,
        ..NotificationPolicy::default()
    });
    shell
        .edit_alert_rules(ManagedAlertRuleEdit::Import {
            rules: vec![ManagedAlertRule::new(
                AlertRule::new(
                    "bevy-test-cpu-high",
                    AlertMetric::CpuUsagePercent,
                    AlertSeverity::Warning,
                    90.0,
                    Duration::ZERO,
                    0.0,
                ),
                true,
            )],
            mode: AlertRuleImportMode::Replace,
        })
        .expect("the test rule is valid");
    for timestamp_ms in [100_000, 101_000] {
        fixture::edit_snapshot(&mut shell, |snapshot| {
            let snapshot = snapshot.as_mut().expect("demo snapshot");
            snapshot.timestamp_ms = timestamp_ms;
            snapshot.cpu = CpuMetrics::from_observations(CpuScalarObservations {
                global_usage_pct: ScalarObservation::available(95.0, timestamp_ms),
                ..Default::default()
            });
        });
        shell.apply_platform_batch(PlatformEventBatch::default());
    }
    assert_eq!(
        shell.projection().alert_center.active_alerts().len(),
        1,
        "the two high-CPU folds activate the test rule"
    );
    let mut app = app_with_notifications(shell, notifications.clone());

    app.world_mut().trigger(crate::drain::ShellProjectionFolded);
    let pending = &app.world().resource::<PendingEffects>().0;
    assert_eq!(pending.len(), 1);
    assert!(matches!(
        &pending[0],
        PlatformEffect::DesktopNotification(_)
    ));
    assert!(
        notifications
            .0
            .lock()
            .expect("notification recorder lock")
            .is_empty(),
        "the page observer only queues its intent"
    );

    app.update();

    assert!(app.world().resource::<PendingEffects>().0.is_empty());
    assert_eq!(
        notifications
            .0
            .lock()
            .expect("notification recorder lock")
            .len(),
        1,
        "the PreUpdate drain submits the queued notification"
    );
}
