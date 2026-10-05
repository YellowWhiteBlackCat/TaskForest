//! Normal mounted controls, effect submission, independent results and stale replies.
use super::opened_shell;
use crate::app::FrontendTrack;
use crate::drain::{ShellProjectionFolded, run_drain_cycle};
use crate::input::PendingEffects;
use crate::pages::processes::properties_modal::{
    ProcessPropertiesAction, ProcessPropertiesOverlay, ProcessPropertiesSection,
    ProcessPropertiesTab, register,
};
use crate::palette::ui_palette;
use crate::window::{AppShellRoot, WindowPalette};
use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::{entity::Entity, query::With};
use bevy::scene::ScenePlugin;
use bevy::text::Font;
use bevy::ui::widget::Text;
use bevy::ui_widgets::Activate;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use taskmanager_application::ProcessInsightFacet;
use taskmanager_application::{
    PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle, ProcessFacets,
    ProcessInsightFacetEvent, ProcessInsightObservation, ProcessNetworkRequest, i18n::t,
};
use taskmanager_core::core::FailureKind;
use taskmanager_core::core::process_telemetry::{ProcessIdentity, ProcessInsightSnapshot};
use taskmanager_platform_contract::{
    CapabilityId, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError, EventSequence,
    OperationFailure, ProviderFailure, RequestEnvelope, RequestPort, SubmissionError,
};
use taskmanager_shell::{fixture, queue_effect};
use taskmanager_theme::Theme;

#[derive(Default)]
struct NetworkPort(Mutex<Vec<RequestEnvelope<ProcessNetworkRequest>>>);
impl RequestPort for NetworkPort {
    type Request = ProcessNetworkRequest;
    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0.lock().expect("requests").push(request);
        Ok(())
    }
}
#[derive(Default)]
struct Events(Mutex<VecDeque<EventEnvelope<PlatformEvent>>>);
impl EventPort for Events {
    type Event = PlatformEvent;
    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(self.0.lock().expect("events").pop_front())
    }
}
fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app.insert_resource(WindowPalette {
        inner: ui_palette(&Theme::dark()),
    });
    app.init_resource::<PendingEffects>();
    app.insert_non_send(FrontendTrack {
        shell: opened_shell(),
        initial_refresh_submitted: true,
        process_tree_expansion: Default::default(),
    });
    app.world_mut().spawn(AppShellRoot);
    register(&mut app);
    app.update();
    app
}
fn submit_and_fold(app: &mut App, client: &mut PlatformClient) {
    let effects = std::mem::take(&mut app.world_mut().resource_mut::<PendingEffects>().0);
    let mut track = app.world_mut().non_send_mut::<FrontendTrack>();
    for effect in effects {
        queue_effect(&mut track.shell, client, effect);
    }
    let _ = run_drain_cycle(client, &mut track.shell, 100, None);
    app.world_mut().trigger(ShellProjectionFolded);
    app.update();
}
fn reply(
    request: &RequestEnvelope<ProcessNetworkRequest>,
    sequence: u64,
    denied: bool,
) -> EventEnvelope<PlatformEvent> {
    let capability = CapabilityId::PROCESS_INSIGHTS_NETWORK;
    let sequence = EventSequence::new(sequence);
    let outcome = if denied {
        Err(OperationFailure {
            request_id: request.id,
            capability: capability.clone(),
            sequence,
            kind: FailureKind::RequiresEscalation,
            retry: ProviderFailure::from_kind(FailureKind::RequiresEscalation).retry(),
            provider: None,
            observed_at_ms: 100,
        })
    } else {
        let mut snapshot = fixture::process_insights::process_insights_snapshot();
        snapshot.network.rx_bytes_per_sec = Some(0);
        snapshot.network.tx_bytes_per_sec = Some(1024);
        Ok(PlatformEvent::ProcessInsightFacet(
            ProcessInsightFacetEvent::Network(Box::new(ProcessInsightObservation {
                target: request.payload.target.clone(),
                revision: request.payload.revision,
                snapshot: ProcessInsightSnapshot {
                    identity: ProcessIdentity {
                        pid: request.payload.target.pid,
                        start_token: request
                            .payload
                            .target
                            .authoritative_start_token()
                            .expect("token"),
                    },
                    value: snapshot.network,
                },
            })),
        ))
    };
    EventEnvelope {
        request_id: request.id,
        capability,
        provider: None,
        sequence,
        observed_at_ms: 100,
        outcome,
    }
}
#[test]
fn mounted_properties_submit_once_fold_partial_results_and_reject_late_refresh_replies() {
    let mut app = app();
    let events = Arc::new(Events::default());
    let network = Arc::new(NetworkPort::default());
    let mut client = PlatformClient::new(PlatformHandle::new(
        Arc::new(CapabilitySnapshot::default()),
        events.clone(),
        PlatformFacets::default()
            .with_process(ProcessFacets::default().with_network(network.clone())),
    ));
    let frozen = app
        .world()
        .non_send::<FrontendTrack>()
        .shell
        .process_properties_target()
        .expect("target")
        .clone();
    app.world_mut().non_send_mut::<FrontendTrack>().shell.query = "hidden selection".to_owned();
    let tab = app
        .world_mut()
        .query::<(Entity, &ProcessPropertiesTab)>()
        .iter(app.world())
        .find(|(_, tab)| tab.0 == ProcessPropertiesSection::Insights)
        .map(|(e, _)| e)
        .expect("tab");
    app.world_mut().trigger(Activate { entity: tab });
    app.world_mut().trigger(Activate { entity: tab });
    assert_eq!(app.world().resource::<PendingEffects>().0.len(), 1);
    submit_and_fold(&mut app, &mut client);
    let first = network.0.lock().expect("requests")[0].clone();
    assert_eq!(first.payload.target, frozen);
    assert_eq!(network.0.lock().expect("requests").len(), 1);
    let shell = &app.world().non_send::<FrontendTrack>().shell;
    let missing_gpu = super::view_model(
        shell,
        ProcessPropertiesSection::Insights,
        ProcessInsightFacet::Gpu,
    )
    .expect("gpu");
    assert!(
        missing_gpu.rows[0]
            .1
            .contains(t("proc_insights.unsupported_provider"))
    );
    events
        .0
        .lock()
        .expect("events")
        .push_back(reply(&first, 1, true));
    submit_and_fold(&mut app, &mut client);
    assert!(
        app.world_mut()
            .query::<&ProcessPropertiesAction>()
            .iter(app.world())
            .any(|action| *action == ProcessPropertiesAction::AuthorizeNetwork)
    );
    let refresh = app
        .world_mut()
        .query::<(Entity, &ProcessPropertiesAction)>()
        .iter(app.world())
        .find(|(_, action)| **action == ProcessPropertiesAction::Refresh)
        .map(|(e, _)| e)
        .expect("refresh");
    app.world_mut().trigger(Activate { entity: refresh });
    app.world_mut().trigger(Activate { entity: refresh });
    submit_and_fold(&mut app, &mut client);
    assert_eq!(network.0.lock().expect("requests").len(), 2);
    let second = network.0.lock().expect("requests")[1].clone();
    events
        .0
        .lock()
        .expect("events")
        .push_back(reply(&first, 2, false));
    submit_and_fold(&mut app, &mut client);
    let shell = &app.world().non_send::<FrontendTrack>().shell;
    assert_eq!(
        super::view_model(
            shell,
            ProcessPropertiesSection::Insights,
            ProcessInsightFacet::Network
        )
        .expect("network")
        .rows[0]
            .1,
        t("proc_insights.collecting")
    );
    events
        .0
        .lock()
        .expect("events")
        .push_back(reply(&second, 3, false));
    submit_and_fold(&mut app, &mut client);
    let values = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(
        values
            .iter()
            .any(|value| value.contains("RX 0 B/s") && value.contains("12.4 ms")),
        "correlated network facts reach the mounted review"
    );
    let overlay = app
        .world_mut()
        .query_filtered::<Entity, With<ProcessPropertiesOverlay>>()
        .single(app.world())
        .expect("overlay");
    app.world_mut().trigger(ShellProjectionFolded);
    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<ProcessPropertiesOverlay>>()
            .single(app.world())
            .expect("overlay"),
        overlay,
        "unchanged folds retain entity identity"
    );
}
