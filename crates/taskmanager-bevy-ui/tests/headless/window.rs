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
use taskmanager_theme::tokens::UiSize;

use bevy::MinimalPlugins;
use bevy::app::{App, Plugin, PostUpdate};
use bevy::asset::{AssetPlugin, Assets, Handle};
use bevy::camera::{Camera, Camera2d, RenderTarget};
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::system::RunSystemOnce;
use bevy::image::{Image, TextureAtlasLayout};
use bevy::input_focus::InputFocusPlugin;
use bevy::math::{UVec2, Vec2};
use bevy::picking::DefaultPickingPlugins;
use bevy::picking::Pickable;
use bevy::scene::ScenePlugin;
use bevy::text::{RemSize, TextColor, TextFont, TextPlugin};
use bevy::transform::TransformPlugin;
use bevy::ui::widget::Text;
use bevy::ui::{
    ComputedUiRenderTargetInfo, ComputedUiTargetCamera, DefaultUiCamera, UiPlugin, UiTargetCamera,
};
use bevy::window::{
    ExitCondition, PrimaryWindow, Window, WindowPlugin, WindowRef, WindowResolution,
};
use taskforest_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, RequestEnvelope, RequestPort, SubmissionError,
};
use taskmanager_application::{
    HostTelemetryRequest, PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle,
    SystemFacets,
};

use taskmanager_theme::optical_glass::{
    OpticalGlassContext, OpticalGlassFallback, OpticalGlassUnavailableReason,
};
use taskmanager_theme::{AppearanceOverride, GlassTier, LightDark, Skin, Theme};

use super::appearance::resolve_demo_theme;
use super::backdrop_capture::{
    BackdropCaptureCamera, BackdropCaptureRoot, BackdropCaptureTarget, BackdropChainCamera,
    BackdropRevision, PresentationCamera, drive_backdrop_capture,
};
use super::{
    AppShellRoot, FeedbackActivityLine, FrontendWindowPlugin, Role, SummaryLine, TextRole,
    WindowPalette,
};
use crate::drain::ShellProjectionFolded;
use crate::glass::optical::{
    OpticalGlassBinding, OpticalGlassMaterial, OpticalGlassMaterialKind, OpticalGlassRaster,
    OpticalGlassSurface,
};
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use taskmanager_shell::FeedbackLifecycle;
use taskmanager_shell::FeedbackSeverity;
use taskmanager_shell::FeedbackSource;

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
        // MinimalPlugins has no ImagePlugin; the icon bridge needs the store.
        app.init_resource::<bevy::asset::Assets<bevy::image::Image>>();
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
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });
    app.init_resource::<bevy::asset::Assets<bevy::text::Font>>();
    app
}

/// The real frontend composition and bundled typography, with a quiet injected
/// platform. Layout tests use this so text measurements match the product.
pub(crate) fn scripted_frontend_app() -> App {
    let (client, _) = scripted_client();
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime = cache
        .get_or_init(move || Ok(client))
        .expect("scripted runtime");
    headless_window_app(runtime)
}

#[test]
fn offscreen_capture_camera_never_owns_the_interactive_shell() {
    let (client, _host_requests) = scripted_client();
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime = cache
        .get_or_init(move || Ok(client))
        .expect("scripted runtime starts");
    let mut app = headless_window_app(runtime);
    app.update();

    let world = app.world_mut();
    let presentation_cameras = world
        .query_filtered::<Entity, With<PresentationCamera>>()
        .iter(world)
        .collect::<Vec<_>>();
    let capture_cameras = world
        .query_filtered::<Entity, With<BackdropCaptureCamera>>()
        .iter(world)
        .collect::<Vec<_>>();
    let chain_cameras = world
        .query_filtered::<Entity, With<BackdropChainCamera>>()
        .iter(world)
        .collect::<Vec<_>>();
    assert_eq!(
        presentation_cameras.len(),
        1,
        "the frontend must spawn exactly one presentation camera"
    );
    assert_eq!(
        capture_cameras.len(),
        1,
        "the frontend must spawn exactly one offscreen capture camera"
    );
    assert_eq!(
        chain_cameras.len(),
        1,
        "the frontend must spawn exactly one second-layer chain camera"
    );
    let presentation = presentation_cameras[0];
    let capture = capture_cameras[0];
    let chain = chain_cameras[0];

    assert_eq!(
        world
            .get::<Camera>(presentation)
            .expect("presentation camera component")
            .order,
        -1
    );
    assert_eq!(
        world
            .get::<Camera>(capture)
            .expect("capture camera component")
            .order,
        -3
    );
    assert_eq!(
        world
            .get::<Camera>(chain)
            .expect("chain camera component")
            .order,
        -2,
        "the chain renders after the capture it samples and before the window"
    );
    assert!(matches!(
        world.get::<RenderTarget>(presentation),
        Some(RenderTarget::Window(WindowRef::Primary))
    ));
    assert!(matches!(
        world.get::<RenderTarget>(capture),
        Some(RenderTarget::Image(_))
    ));
    assert!(matches!(
        world.get::<RenderTarget>(chain),
        Some(RenderTarget::Image(_))
    ));

    let default = world
        .run_system_once(|default: DefaultUiCamera| default.get())
        .expect("default UI camera query runs")
        .expect("the presentation camera is the default UI target");
    assert_eq!(
        default, presentation,
        "the offscreen camera must not steal the default UI target"
    );

    let capture_roots = world
        .query_filtered::<Entity, With<BackdropCaptureRoot>>()
        .iter(world)
        .collect::<Vec<_>>();
    assert_eq!(capture_roots.len(), 1);
    assert_eq!(
        world
            .get::<UiTargetCamera>(capture_roots[0])
            .expect("capture root has an explicit target camera")
            .0,
        capture
    );
    assert_eq!(
        world.get::<Pickable>(capture_roots[0]),
        Some(&Pickable::IGNORE),
        "the offscreen backdrop root must stay pointer-transparent"
    );

    let shell_roots = world
        .query_filtered::<Entity, With<AppShellRoot>>()
        .iter(world)
        .collect::<Vec<_>>();
    assert_eq!(shell_roots.len(), 1);
    assert!(
        world.get::<UiTargetCamera>(shell_roots[0]).is_none(),
        "the interactive shell must keep the production default-camera path, not the capture target"
    );
}

#[test]
fn backdrop_target_tracks_window_physical_size_and_scale_factor() {
    let (client, _host_requests) = scripted_client();
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime = cache
        .get_or_init(move || Ok(client))
        .expect("scripted runtime starts");
    let mut app = headless_window_app(runtime);
    app.add_plugins(WindowPlugin {
        primary_window: None,
        exit_condition: ExitCondition::DontExit,
        ..Default::default()
    });
    let initial_size = UVec2::new(1600, 900);
    app.world_mut().spawn((
        Window {
            resolution: WindowResolution::new(initial_size.x, initial_size.y)
                .with_scale_factor_override(1.5),
            ..Default::default()
        },
        PrimaryWindow,
    ));
    app.update();

    let world = app.world_mut();
    let target = world.resource::<BackdropCaptureTarget>().clone();
    assert_eq!(target.physical_size, initial_size);
    assert!((target.scale_factor - 1.5).abs() < f32::EPSILON);
    let image_size = world
        .resource::<Assets<Image>>()
        .get(&target.image)
        .expect("the capture target image is registered")
        .size();
    assert_eq!(image_size, initial_size);
    let capture_target = world
        .get::<RenderTarget>(target.camera)
        .expect("capture camera has a render target");
    let RenderTarget::Image(capture_target) = capture_target else {
        panic!("capture camera must target an image");
    };
    assert!((capture_target.scale_factor - 1.5).abs() < f32::EPSILON);

    let window_entity = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .iter(world)
        .next()
        .expect("primary window exists");
    {
        let mut window = world
            .get_mut::<Window>(window_entity)
            .expect("primary window component");
        window.resolution.set_physical_resolution(1024, 768);
        window.resolution.set_scale_factor_override(Some(2.0));
    }

    app.update();
    let world = app.world_mut();
    let target = world.resource::<BackdropCaptureTarget>();
    assert_eq!(target.physical_size, UVec2::new(1024, 768));
    assert!((target.scale_factor - 2.0).abs() < f32::EPSILON);
    assert_eq!(
        world
            .resource::<Assets<Image>>()
            .get(&target.image)
            .expect("the resized capture target remains registered")
            .size(),
        UVec2::new(1024, 768)
    );
    let capture_target = world
        .get::<RenderTarget>(target.camera)
        .expect("capture camera has a render target");
    let RenderTarget::Image(capture_target) = capture_target else {
        panic!("capture camera must target an image");
    };
    assert!((capture_target.scale_factor - 2.0).abs() < f32::EPSILON);
}

#[test]
fn ui_layout_targets_the_shell_to_presentation_and_the_backdrop_to_capture() {
    let (client, _host_requests) = scripted_client();
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime = cache
        .get_or_init(move || Ok(client))
        .expect("scripted runtime starts");
    let mut app = headless_window_app(runtime);
    app.add_plugins((
        WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..Default::default()
        },
        DefaultPickingPlugins,
        TransformPlugin,
        TextPlugin,
        UiPlugin,
    ));
    app.init_resource::<Assets<TextureAtlasLayout>>();
    let size = UVec2::new(800, 600);
    app.world_mut().spawn((
        Window {
            resolution: size.into(),
            ..Default::default()
        },
        PrimaryWindow,
    ));
    app.update();
    app.update();

    let world = app.world_mut();
    let presentation = world
        .query_filtered::<Entity, With<PresentationCamera>>()
        .iter(world)
        .next()
        .expect("presentation camera");
    let capture = world
        .query_filtered::<Entity, With<BackdropCaptureCamera>>()
        .iter(world)
        .next()
        .expect("capture camera");
    let shell = world
        .query_filtered::<Entity, With<AppShellRoot>>()
        .iter(world)
        .next()
        .expect("interactive shell root");
    let capture_root = world
        .query_filtered::<Entity, With<BackdropCaptureRoot>>()
        .iter(world)
        .next()
        .expect("backdrop root");

    assert_eq!(
        world
            .get::<ComputedUiTargetCamera>(shell)
            .expect("shell target is computed by the UI layout")
            .get(),
        Some(presentation),
        "the interactive shell must stay on the primary-window camera"
    );
    assert_eq!(
        world
            .get::<ComputedUiRenderTargetInfo>(shell)
            .expect("shell render-target metadata")
            .physical_size(),
        size
    );
    assert_eq!(
        world
            .get::<ComputedUiTargetCamera>(capture_root)
            .expect("capture root target is computed by the UI layout")
            .get(),
        Some(capture),
        "the backdrop tree must stay on the offscreen camera"
    );
    assert_eq!(
        world
            .get::<ComputedUiRenderTargetInfo>(capture_root)
            .expect("capture root render-target metadata")
            .physical_size(),
        size
    );
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
    let standard = ui_palette(&Theme::dark(), UiSize::Standard);
    assert_eq!(
        font.font_size, standard.caption.font_size,
        "the observer stamped the palette caption metrics verbatim"
    );
    assert_eq!(
        font.font_size
            .eval(bevy::math::Vec2::ZERO, RemSize(standard.rem_px)),
        UiSize::Standard.caption_font_size().0,
        "the caption resolves to the Standard caption token (12px floor)"
    );
    assert_eq!(
        ink.0.to_srgba(),
        ui_palette(&Theme::dark(), UiSize::Standard)
            .dim_color
            .to_srgba(),
        "the observer stamped the dim ink token"
    );
}

/// The theme owner's shared override vocabulary selects the demo/capture
/// appearance (the same parser GPUI, Iced and the TUI read).
/// Unset (and invalid) keeps today's GNOME-light reference skin; a valid token
/// resolves the requested skin/mode and repaints the window palette.
#[test]
fn tm_skin_override_resolves_the_demo_theme() {
    let parse = |value: &str, high_contrast: bool| {
        resolve_demo_theme(AppearanceOverride::parse(value, high_contrast))
    };
    let default = resolve_demo_theme(None);
    assert_eq!(default.skin, Skin::Gnome, "unset keeps the GNOME skin");
    assert_eq!(
        default.mode,
        LightDark::Light,
        "unset keeps the light reference mode"
    );
    assert_eq!(
        parse("plasma-dark", false).skin,
        Skin::Gnome,
        "an unknown skin is not an override"
    );

    let dark = parse("gnome-dark", false);
    assert_eq!(dark.skin, Skin::Gnome);
    assert_eq!(dark.mode, LightDark::Dark);
    assert_ne!(
        ui_palette(&default, UiSize::Standard).window_clear,
        ui_palette(&dark, UiSize::Standard).window_clear,
        "gnome-dark must repaint the window backdrop"
    );

    let contrast = parse("KDE-DARK", true);
    assert_eq!(contrast.skin, Skin::Kde, "case-insensitive skin alias");
    assert_eq!(contrast.mode, LightDark::Dark);
    assert!(contrast.hc, "TM_SKIN_HC rides the same override");
}

#[test]
fn the_accessibility_bridge_resources_are_installed_once() {
    // The AccessKit bridge (accesskit_unix on Linux) publishes
    // `AccessibilityNode` components only while these resources exist. The
    // windowed composition must end with them installed exactly once —
    // `DefaultPlugins` (bevy_winit) already adds the plugin when the feature
    // is on, and a second unconditional add panics at startup. This headless
    // composition lacks `DefaultPlugins`, so the plugin must come from the
    // frontend composition itself; the double-add guard is what keeps both
    // compositions alive.
    let (client, _host_requests) = scripted_client();
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime = cache
        .get_or_init(move || Ok(client))
        .expect("scripted runtime starts");
    let mut app = headless_window_app(runtime);
    app.update();
    app.update();

    let world = app.world_mut();
    assert!(
        world
            .get_resource::<bevy::a11y::AccessibilityRequested>()
            .is_some(),
        "the AT-requests resource must exist after composition"
    );
    assert!(
        world
            .get_resource::<bevy::a11y::ManageAccessibilityUpdates>()
            .is_some(),
        "the update-management resource must exist after composition"
    );
}

#[test]
fn timed_feedback_notice_mounts_and_retires_the_performance_overlay() {
    let (client, _host_requests) = scripted_client();
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime = cache
        .get_or_init(move || Ok(client))
        .expect("scripted runtime starts");
    let mut app = headless_window_app(runtime);

    // Initial update to run Startup systems and spawn the app shell.
    app.update();
    app.world_mut().resource_mut::<crate::app::Route>().page = crate::app::Page::Performance;
    app.world_mut().trigger(crate::app::RouteChanged);
    app.update();

    let world = app.world_mut();
    let line = world
        .query_filtered::<&Text, With<FeedbackActivityLine>>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect::<Vec<String>>();
    assert_eq!(
        line.len(),
        1,
        "app shell spawns exactly one footer activity line"
    );
    assert_eq!(
        line[0], "Waiting for telemetry…",
        "footer activity remains the background projection"
    );

    // Report a timed feedback notice with 30ms lifetime.
    app.world_mut()
        .non_send_mut::<crate::app::FrontendTrack>()
        .shell
        .report_notice(
            FeedbackSource::Interaction,
            FeedbackSeverity::Info,
            FeedbackLifecycle::timed(std::time::Duration::from_millis(30)),
            "Screenshot captured",
        );

    // Frame 1: drain_system advances 16ms (leaving 14ms), detects the shell
    // notice and mounts its floating overlay.
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<crate::feedback_overlay::FeedbackToast>>()
            .iter(world)
            .count(),
        1,
        "active shell notice mounts one feedback overlay"
    );

    // Frame 2: drain_system advances another 16ms (32ms total >= 30ms).
    // The shell expires the notice and the overlay retires.
    app.update();

    let world = app.world_mut();
    assert!(
        world
            .non_send::<crate::app::FrontendTrack>()
            .shell
            .feedback_notice()
            .is_none(),
        "timed notice must be cleared from shell after expiring"
    );
    assert_eq!(
        world
            .query_filtered::<Entity, With<crate::feedback_overlay::FeedbackToast>>()
            .iter(world)
            .count(),
        0,
        "expired shell notice removes the floating overlay"
    );
}

/// The shared optical surface's measured physical geometry.
fn optical_surface() -> OpticalGlassSurface {
    let spec = Theme::dark()
        .optical_glass_spec(GlassTier::Card)
        .expect("card optical spec");
    let geometry = spec
        .resolve(OpticalGlassContext::new(1.0, 1.0))
        .expect("valid optical geometry");
    OpticalGlassSurface {
        kind: OpticalGlassMaterialKind::Optical,
        spec,
        geometry,
        raster: OpticalGlassRaster {
            origin_px: Vec2::ZERO,
            size_px: Vec2::new(120.0, 80.0),
            viewport_px: Vec2::new(800.0, 600.0),
            device_scale_factor: 1.0,
        },
        backdrop: Handle::<Image>::default(),
    }
}

/// One attached optical surface: the material the controller actually samples.
fn attached_optical_surface() -> impl bevy::ecs::bundle::Bundle {
    (
        optical_surface(),
        OpticalGlassBinding::Optical(Handle::<OpticalGlassMaterial>::default()),
    )
}

/// The invalidation controller on a minimal app: the offscreen camera, the
/// revision resources and the drive system, with nothing else.
fn backdrop_controller_app() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    super::backdrop_capture::register(&mut app);
    app.add_systems(PostUpdate, drive_backdrop_capture);
    let camera = app
        .world_mut()
        .spawn((
            Camera2d,
            Camera {
                is_active: false,
                ..Camera::default()
            },
            BackdropCaptureCamera,
        ))
        .id();
    (app, camera)
}

#[test]
fn the_backdrop_capture_stays_dormant_without_a_live_optical_surface() {
    let (mut app, camera) = backdrop_controller_app();
    app.world_mut().resource_mut::<BackdropRevision>().bump();

    for _ in 0..3 {
        app.update();
        assert!(
            !app.world()
                .get::<Camera>(camera)
                .expect("capture camera")
                .is_active,
            "an unsupported renderer never renders the offscreen backdrop"
        );
    }
}

#[test]
fn a_live_optical_surface_captures_exactly_one_frame_per_revision() {
    let (mut app, camera) = backdrop_controller_app();
    app.world_mut().spawn(attached_optical_surface());

    app.update();
    assert!(
        app.world()
            .get::<Camera>(camera)
            .expect("capture camera")
            .is_active,
        "the first live optical surface captures the lower composite"
    );
    app.update();
    assert!(
        !app.world()
            .get::<Camera>(camera)
            .expect("capture camera")
            .is_active,
        "an unchanged lower composite is not re-rendered"
    );

    app.world_mut().resource_mut::<BackdropRevision>().bump();
    app.update();
    assert!(
        app.world()
            .get::<Camera>(camera)
            .expect("capture camera")
            .is_active,
        "a new revision captures the lower composite again"
    );
    app.update();
    assert!(
        !app.world()
            .get::<Camera>(camera)
            .expect("capture camera")
            .is_active,
        "the capture stops once the revision is mirrored"
    );
}

#[test]
fn a_fallback_surface_does_not_wake_the_offscreen_optical_camera() {
    let (mut app, camera) = backdrop_controller_app();
    app.world_mut().spawn((
        optical_surface(),
        OpticalGlassBinding::Fallback {
            reason: OpticalGlassUnavailableReason::Headless,
            material: OpticalGlassFallback::Painted,
        },
    ));

    for _ in 0..3 {
        app.update();
        assert!(
            !app.world()
                .get::<Camera>(camera)
                .expect("capture camera")
                .is_active,
            "the shader never samples a fallback surface, so its camera stays dormant"
        );
    }
}

#[test]
fn a_palette_change_rearms_the_capture_for_a_live_surface() {
    let (mut app, camera) = backdrop_controller_app();
    app.world_mut().spawn(attached_optical_surface());
    app.update();
    app.update();
    assert!(
        !app.world()
            .get::<Camera>(camera)
            .expect("capture camera")
            .is_active
    );

    app.world_mut().insert_resource(WindowPalette {
        inner: ui_palette(&Theme::dark(), UiSize::Large),
    });
    app.update();
    assert!(
        app.world()
            .get::<Camera>(camera)
            .expect("capture camera")
            .is_active,
        "a palette swap invalidates the captured lower composite"
    );
    app.update();
    assert!(
        !app.world()
            .get::<Camera>(camera)
            .expect("capture camera")
            .is_active
    );
}

#[test]
fn a_projection_fold_bumps_the_backdrop_revision() {
    let (mut app, _camera) = backdrop_controller_app();
    let before = app.world().resource::<BackdropRevision>().0;

    app.world_mut().trigger(ShellProjectionFolded);

    assert_eq!(
        app.world().resource::<BackdropRevision>().0,
        before + 1,
        "a fold invalidates the captured lower composite"
    );
}
