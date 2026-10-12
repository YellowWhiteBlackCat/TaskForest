//! The Process Details observers queue typed effects; the drain submits them.

use std::sync::{Arc, Mutex};

use bevy::MinimalPlugins;
use bevy::app::{App, PreUpdate};
use bevy::asset::{AssetPlugin, Assets};
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui_widgets::Activate;
use taskmanager_application::{
    AppAction, AppPage, PlatformClient, PlatformEffect, PlatformEvent, PlatformFacets,
    PlatformHandle, ProcessFacets, ProcessNetworkEscalationRequest, ProcessNetworkRequest,
};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
    RequestEnvelope, RequestPort, SubmissionError,
};
use taskmanager_shell::{ShellApp, fixture};

use crate::app::{FrontendTrack, PageContext, SharedRuntimeHandle};
use crate::drain::{FeedbackCache, drain_system};
use crate::input::PendingEffects;
use crate::palette::{UiPalette, ui_palette};
use crate::runtime::SharedRuntime;
use crate::window::WindowPalette;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::panel_scene;
use super::processes::ProcessSelectionChanged;
use crate::pages::history::HistoryProjectionResource;

#[derive(Default)]
struct NetworkRequests(Mutex<Vec<ProcessNetworkRequest>>);

impl RequestPort for NetworkRequests {
    type Request = ProcessNetworkRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .expect("process network recorder lock")
            .push(request.payload);
        Ok(())
    }
}

#[derive(Default)]
struct EscalationRequests(Mutex<Vec<ProcessNetworkEscalationRequest>>);

impl RequestPort for EscalationRequests {
    type Request = ProcessNetworkEscalationRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .expect("network escalation recorder lock")
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

fn shell_with_selected_process() -> ShellApp {
    let mut process = ProcessItem::new(42, "worker");
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(7, 1),
        ..Default::default()
    });
    let mut second_process = ProcessItem::new(43, "worker-two");
    second_process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(8, 1),
        ..Default::default()
    });
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |rows| {
        *rows = Some(vec![process, second_process]);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(shell.select_row(0));
    shell
}

fn app_with_process_requests(
    shell: ShellApp,
    network: Arc<NetworkRequests>,
    escalation: Arc<EscalationRequests>,
) -> App {
    let client = PlatformClient::new(PlatformHandle::new(
        Arc::new(Capabilities),
        Arc::new(Events),
        PlatformFacets::default().with_process(
            ProcessFacets::default()
                .with_network(network)
                .with_network_escalation(escalation),
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
    app.add_systems(PreUpdate, drain_system);
    app
}

fn app_with_mounted_details(
    shell: ShellApp,
    network: Arc<NetworkRequests>,
    escalation: Arc<EscalationRequests>,
) -> App {
    let client = PlatformClient::new(PlatformHandle::new(
        Arc::new(Capabilities),
        Arc::new(Events),
        PlatformFacets::default().with_process(
            ProcessFacets::default()
                .with_network(network)
                .with_network_escalation(escalation),
        ),
    ));
    let shared = Box::leak(Box::new(SharedRuntime::new(client)));
    let palette: UiPalette = ui_palette(&Theme::dark(), UiSize::Standard);
    let history = HistoryProjectionResource::default();
    let sidebar = Default::default();
    let context = PageContext {
        sidebar: &sidebar,
        gray_zero_values: false,
        shell: &shell,
        palette: &palette,
        history: &history.0,
    };
    let scene = panel_scene(&context);

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app.init_resource::<PendingEffects>();
    app.init_resource::<FeedbackCache>();
    app.insert_resource(SharedRuntimeHandle { shared });
    app.insert_resource(WindowPalette { inner: palette });
    app.insert_non_send(FrontendTrack {
        shell,
        initial_refresh_submitted: true,
        process_tree_expansion: Default::default(),
    });
    app.world_mut()
        .spawn_scene(scene)
        .expect("the production details panel scene resolves headlessly");
    app.add_systems(PreUpdate, drain_system);
    app
}

#[test]
fn details_page_mount_and_selection_observers_queue_until_the_drain() {
    let network = Arc::new(NetworkRequests::default());
    let escalation = Arc::new(EscalationRequests::default());
    let shell = shell_with_selected_process();
    let first_target = shell
        .selected_process_identity()
        .expect("the first process has a frozen identity")
        .clone();
    let mut app = app_with_mounted_details(shell, network.clone(), escalation.clone());

    // `spawn_scene` applies the production scene immediately, so adding its
    // ProcessDetailsRoot synchronously runs `bootstrap_details_page`. Assert
    // the queued request before the first update lets PreUpdate drain it.
    let pending = &app.world().resource::<PendingEffects>().0;
    let initial_requests = pending
        .iter()
        .filter_map(|effect| match effect {
            PlatformEffect::ProcessInsights(target) if target == &first_target => Some(()),
            _ => None,
        })
        .count();
    assert_eq!(
        initial_requests, 1,
        "mount queues exactly one ProcessInsights request for the selected target"
    );
    assert!(
        network
            .0
            .lock()
            .expect("process network recorder lock")
            .is_empty()
    );

    // The first scheduled drain submits the bootstrap request.
    app.update();
    assert!(app.world().resource::<PendingEffects>().0.is_empty());
    {
        let submitted = network.0.lock().expect("process network recorder lock");
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0].target, first_target);
    }

    let second_target = {
        let mut track = app.world_mut().non_send_mut::<FrontendTrack>();
        assert!(track.shell.select_row(1));
        track
            .shell
            .selected_process_identity()
            .expect("the second process has a frozen identity")
            .clone()
    };
    assert_ne!(
        first_target, second_target,
        "the selection event must carry a different process identity"
    );
    app.world_mut().trigger(ProcessSelectionChanged);

    let pending = &app.world().resource::<PendingEffects>().0;
    let selection_requests = pending
        .iter()
        .filter_map(|effect| match effect {
            PlatformEffect::ProcessInsights(target) if target == &second_target => Some(()),
            _ => None,
        })
        .count();
    assert_eq!(
        selection_requests, 1,
        "selection change queues exactly one ProcessInsights request for the new target"
    );
    assert_eq!(
        network
            .0
            .lock()
            .expect("process network recorder lock")
            .len(),
        1,
        "selection change queues the new identity without submitting it"
    );

    app.update();

    assert!(app.world().resource::<PendingEffects>().0.is_empty());
    let submitted = network.0.lock().expect("process network recorder lock");
    assert_eq!(submitted.len(), 2);
    assert_eq!(submitted[1].target, second_target);
    assert!(
        escalation
            .0
            .lock()
            .expect("network escalation recorder lock")
            .is_empty()
    );
}

#[test]
fn process_details_observers_queue_until_drain_submits_each_effect() {
    let network = Arc::new(NetworkRequests::default());
    let escalation = Arc::new(EscalationRequests::default());
    let mut refresh_app = app_with_process_requests(
        shell_with_selected_process(),
        network.clone(),
        escalation.clone(),
    );
    let expected_target = refresh_app
        .world()
        .non_send::<FrontendTrack>()
        .shell
        .selected_process_identity()
        .expect("the selected process has a frozen identity")
        .clone();
    let refresh_entity = refresh_app.world_mut().spawn_empty().id();

    refresh_app.add_observer(super::on_refresh_activated);
    refresh_app.world_mut().trigger(Activate {
        entity: refresh_entity,
    });

    let pending = &refresh_app.world().resource::<PendingEffects>().0;
    assert_eq!(pending.len(), 1);
    assert!(matches!(
        &pending[0],
        PlatformEffect::ProcessInsights(target)
            if target == &expected_target
    ));
    assert!(
        network
            .0
            .lock()
            .expect("process network recorder lock")
            .is_empty()
    );
    assert!(
        escalation
            .0
            .lock()
            .expect("network escalation recorder lock")
            .is_empty()
    );

    // A repeated automatic request for the same frozen target coalesces while
    // the first effect is still waiting for the drain.
    {
        let mut track = FrontendTrack {
            shell: shell_with_selected_process(),
            initial_refresh_submitted: true,
            process_tree_expansion: Default::default(),
        };
        let mut pending = PendingEffects::default();
        super::queue_selected_process_insights(&mut track, &mut pending, false);
        super::queue_selected_process_insights(&mut track, &mut pending, false);
        assert_eq!(pending.0.len(), 1);
    }

    refresh_app.update();

    assert!(
        refresh_app
            .world()
            .resource::<PendingEffects>()
            .0
            .is_empty()
    );
    let submitted_network = network.0.lock().expect("process network recorder lock");
    assert_eq!(submitted_network.len(), 1);
    assert_eq!(submitted_network[0].target, expected_target);
    drop(submitted_network);
    assert!(
        escalation
            .0
            .lock()
            .expect("network escalation recorder lock")
            .is_empty()
    );

    let mut escalation_app = app_with_process_requests(
        shell_with_selected_process(),
        network.clone(),
        escalation.clone(),
    );
    let escalation_entity = escalation_app.world_mut().spawn_empty().id();
    escalation_app.add_observer(super::on_network_escalate_activated);
    escalation_app.world_mut().trigger(Activate {
        entity: escalation_entity,
    });
    assert!(matches!(
        escalation_app
            .world()
            .resource::<PendingEffects>()
            .0
            .as_slice(),
        [PlatformEffect::ProcessNetworkEscalation]
    ));
    assert_eq!(
        escalation
            .0
            .lock()
            .expect("network escalation recorder lock")
            .len(),
        0,
        "the page observer only queues its intent"
    );

    escalation_app.update();

    assert!(
        escalation_app
            .world()
            .resource::<PendingEffects>()
            .0
            .is_empty()
    );
    assert_eq!(
        escalation
            .0
            .lock()
            .expect("network escalation recorder lock")
            .len(),
        1
    );
}
