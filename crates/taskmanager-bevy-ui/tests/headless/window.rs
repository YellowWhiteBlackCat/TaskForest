//! test-intent: behavior
//!
//! Headless wiring test for the window plugin composition.
//!
//! The REAL `FrontendWindowPlugin` composition is mounted on a
//! `MinimalPlugins` app — no window, no winit, no GPU — with a scripted
//! platform client behind the real shared-runtime cache. The updates prove
//! the data flow end to end: the `PreUpdate` drain folds the capability
//! inventory, the `CapabilitySummaryChanged` observer rewrites the summary
//! text, the initial full refresh is submitted exactly once, the bsn! app
//! shell spawns (header/nav/content slot), and the text-role observer stamps
//! palette typography without a compositor.

use std::sync::{Arc, Mutex};

use bevy::MinimalPlugins;
use bevy::app::{App, Plugin};
use bevy::asset::AssetPlugin;
use bevy::ecs::query::With;
use bevy::input_focus::InputFocusPlugin;
use bevy::scene::ScenePlugin;
use bevy::text::{FontSize, TextColor, TextFont};
use bevy::ui::widget::Text;
use taskmanager_application::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, HostTelemetryRequest, PlatformClient, PlatformEvent,
    PlatformFacets, PlatformHandle, RequestEnvelope, RequestPort, SubmissionError, SystemFacets,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use super::{FrontendWindowPlugin, Role, SummaryLine, TextRole};
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};

/// Headless-only infrastructure composition. The production launcher owns
/// these plugins through `DefaultPlugins`; keeping this in the test module
/// prevents the frontend shell from registering host infrastructure.
pub(crate) struct HeadlessFrontendPlugins;

impl Plugin for HeadlessFrontendPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            AssetPlugin::default(),
            ScenePlugin,
            bevy::input::InputPlugin,
            InputFocusPlugin,
        ));
    }
}

struct FixedCapabilities(CapabilitySnapshot);

impl CapabilityCatalog for FixedCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        self.0.clone()
    }
}

struct QuietEvents;

impl EventPort for QuietEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

/// Records every host-telemetry refresh submission: the observable half of
/// "the first frame submits the initial full refresh exactly once".
#[derive(Default)]
struct RecordingHostRequests(Mutex<Vec<HostTelemetryRequest>>);

impl RecordingHostRequests {
    fn len(&self) -> usize {
        self.0.lock().expect("host request recorder lock").len()
    }
}

impl RequestPort for RecordingHostRequests {
    type Request = HostTelemetryRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .expect("host request recorder lock")
            .push(request.payload);
        Ok(())
    }
}

fn descriptor(id: CapabilityId, status: CapabilityStatus) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id,
        status,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }
}

fn scripted_snapshot() -> CapabilitySnapshot {
    CapabilitySnapshot::from_descriptors([
        descriptor(CapabilityId::TELEMETRY_HOST, CapabilityStatus::Available),
        descriptor(CapabilityId::TELEMETRY_CPU, CapabilityStatus::Available),
        descriptor(
            CapabilityId::HARDWARE_INVENTORY,
            CapabilityStatus::PermissionRequired,
        ),
    ])
}

fn scripted_client() -> (PlatformClient, Arc<RecordingHostRequests>) {
    let host_requests = Arc::new(RecordingHostRequests::default());
    let client = PlatformClient::new(PlatformHandle::new(
        Arc::new(FixedCapabilities(scripted_snapshot())),
        Arc::new(QuietEvents),
        PlatformFacets::default()
            .with_system(SystemFacets::default().with_host(host_requests.clone())),
    ));
    (client, host_requests)
}

/// `MinimalPlugins` + the real plugin composition: everything the window
/// wires except `DefaultPlugins`/`WindowPlugin`, so no compositor is
/// involved while the schedule, observers and resources stay production-real.
/// The font store is the one resource `AssetPlugin` does not auto-create.
fn headless_window_app(runtime: &'static SharedRuntime) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime,
        palette: ui_palette(&Theme::dark()),
    });
    app.init_resource::<bevy::asset::Assets<bevy::text::Font>>();
    app
}

#[test]
fn drain_reaches_the_summary_line_and_initial_refresh_is_one_shot() {
    // The bevy World holds a `'static` shared-runtime handle, exactly like the
    // production window; leaking the test cache models the process-lifetime
    // cache without touching the real native composition.
    let (client, host_requests) = scripted_client();
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime = cache
        .get_or_init(move || Ok(client))
        .expect("scripted runtime starts");
    let mut app = headless_window_app(runtime);

    // `resource_mut` below panics when the plugin failed to install the
    // track, so reaching the recorder read already proves the resources.
    app.update();
    let submitted_on_first_frame = host_requests.len();
    assert!(
        submitted_on_first_frame >= 1,
        "the first frame must submit the initial full refresh"
    );

    app.update();
    assert_eq!(
        host_requests.len(),
        submitted_on_first_frame,
        "the startup burst is one-shot: no scheduler, no input, no replay"
    );
    let world = app.world_mut();
    let summary = world
        .query_filtered::<&Text, With<SummaryLine>>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect::<Vec<String>>();
    assert_eq!(
        summary.len(),
        1,
        "the app shell spawns exactly one summary line"
    );
    let line = &summary[0];
    assert!(
        line.contains("3 capabilities") && line.contains("2 available"),
        "the drain's folded inventory must have rewritten the summary line: {line}"
    );
    assert!(
        !line.contains("waiting for"),
        "the cold-start placeholder text must be gone: {line}"
    );
    assert!(
        app.world_mut()
            .non_send::<crate::app::FrontendTrack>()
            .initial_refresh_submitted,
        "the track records the one-shot startup submission"
    );
}

#[test]
fn text_role_observer_stamps_palette_typography() {
    let (client, _host_requests) = scripted_client();
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime = cache
        .get_or_init(move || Ok(client))
        .expect("scripted runtime starts");
    let mut app = headless_window_app(runtime);
    app.update();
    app.update();

    let world = app.world_mut();
    let summary_style = world
        .query_filtered::<(&TextRole, &TextFont, &TextColor), With<SummaryLine>>()
        .iter(world)
        .collect::<Vec<_>>();
    assert_eq!(summary_style.len(), 1);
    let (role, font, ink) = summary_style[0];
    assert_eq!(role.0, Role::Caption, "the summary line is caption-styled");
    let FontSize::Px(size) = font.font_size else {
        panic!("caption size must resolve to px");
    };
    assert_eq!(
        size,
        f32::from(tokens::FONT_CAPTION),
        "the observer stamped the caption type token"
    );
    assert_eq!(
        ink.0.to_srgba(),
        ui_palette(&Theme::dark()).dim_color.to_srgba(),
        "the observer stamped the dim ink token"
    );
}
