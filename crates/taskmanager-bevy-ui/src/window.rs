//! The window composition: bsn! app shell + dynamic observers.
//!
//! **bsn! idiom reference for this crate** (docs/BEVY_UI_FRONTEND.md: static
//! structure composes declaratively, dynamic state binds via observers):
//!
//! - static trees are one [`bsn!`] invocation per named scene function;
//!   children nest either as parenthesized entity entries (`( Text(…)
//!   TextRole(Role::Body) )`) or as one expression item per dynamic fan-out
//!   (`{ vec_of_scenes() }`);
//! - marker values ride the same declarative shape — `TextRole(Role::Caption)`
//!   seeds the template then patches the value, so no post-spawn fixups;
//! - dynamic values in field position are plain expressions but
//!   method-call-looking values (`x.clone()`) must be braced (`{ x.clone() }`)
//!   — the macro's loose value parser only accepts calls without receivers
//!   bare;
//! - what changes at runtime NEVER rebuilds the tree: the summary line is
//!   rewritten by the [`CapabilitySummaryChanged`] observer, text styling is
//!   stamped by the [`On<Add<TextRole>>`](style_text_role) observer, and
//!   page content is remounted by the route observers in [`crate::app`].
//!
//! The frontend plugin below owns only the app shell, route, observers, and
//! page scene. The window launcher owns Bevy's `DefaultPlugins`; headless
//! tests add the explicit headless infrastructure composition. Keeping those two
//! compositions separate is important in Bevy 0.19 because `AssetPlugin`,
//! `ScenePlugin`, and input plugins are singleton infrastructure.

use std::process::ExitCode;
use std::time::Duration;

use bevy::DefaultPlugins;
use bevy::app::{App, AppExit, Plugin, PluginGroup, PostUpdate, PreUpdate, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::{Camera2d, ClearColor};
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Changed, Has, Or, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::text::{Font, FontSource, TextColor, TextFont};
use bevy::ui::Pressed;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, UiRect, Val,
    percent,
};
use bevy::ui::widget::Text;
use bevy::window::{Window, WindowPlugin, WindowResolution};
use taskmanager_app_host::NativeAppHost;
use taskmanager_core::core::metrics::{CpuMetrics, ScalarObservation, ScalarObservationGroup};

use taskmanager_assets::product;
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

use crate::app::{AppShellPlugin, ContentSlot, Page, Route, nav_strip_scene};
use crate::drain::{self, CapabilitySummaryChanged};
use crate::pages::history::HistoryProjectionResource;
use crate::pages::performance::{PerformanceLayoutState, sync_performance_layout};
use crate::palette::{self, UiPalette, space_8, space_12};
use crate::runtime::SharedRuntime;
use crate::widgets::controls::{ControlVisual, control_background};

/// The resolved token palette, injected as a resource for spawn systems.
#[derive(Resource)]
pub(crate) struct WindowPalette {
    pub(crate) inner: UiPalette,
}

/// Handle of the registered embedded UI face (ADR-026 fonts policy).
#[derive(Resource, Default)]
struct PlaceholderFonts {
    ui: Option<Handle<Font>>,
    mono: Option<Handle<Font>>,
}

/// Typographic role stamped onto a text node. One component, one observer:
/// pages/widgets emit `TextRole(Role::…)` inside their bsn! trees and never
/// touch font assets or ink literals — the theme adapter owns both.
///
/// The `Default` seed only exists for the bsn! template mechanism; the
/// spawned value always carries an explicit role.
#[derive(Clone, Copy, Component, Debug, Default, PartialEq, Eq)]
pub(crate) struct TextRole(pub(crate) Role);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Role {
    /// Page/panel titles: heading metrics, full ink.
    Heading,
    /// Primary reading text: body metrics, full ink.
    #[default]
    Body,
    /// Labels, headers, summary lines: caption metrics, dim ink.
    Caption,
    /// Aligned telemetry values and diagnostics: bundled Roboto Mono.
    Mono,
}

/// Marker for the one summary text node the drain observer rewrites.
#[derive(Component, Clone, Default)]
pub(crate) struct SummaryLine;

/// Marker for the status/feedback caption under the capability summary. The
/// drain's [`crate::drain::FeedbackChanged`] observer rewrites it from the
/// shell's typed feedback lifecycle (control outcomes, notices).
#[derive(Component, Clone, Default)]
pub(crate) struct FeedbackLine;

/// Marker on the window shell's root node — the mount point the confirmation
/// overlay stacks under, and the semantic root of the window tree.
#[derive(Component, Clone, Default)]
pub(crate) struct AppShellRoot;

#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct DemoMode;

#[derive(Resource, Default)]
struct CaptureMarkerState(bool);

/// Build and run the live windowed frontend to completion.
pub(crate) fn run(shared: &'static SharedRuntime) -> ExitCode {
    run_with_mode(shared, false)
}

/// Build and run the deterministic capture window. The renderer and route
/// composition are identical to production; only the shell input is the
/// explicit no-I/O fixture and the platform drain is omitted.
pub(crate) fn run_demo(shared: &'static SharedRuntime) -> ExitCode {
    run_with_mode(shared, true)
}

fn run_with_mode(shared: &'static SharedRuntime, demo: bool) -> ExitCode {
    // Production keeps the cold-start dark theme until the native appearance
    // seam arrives. Capture uses the light reference skin so the visual gate
    // compares the actual product structure and typography, not a fixture-only
    // color inversion.
    let theme = if demo {
        Theme::build(
            Skin::Gnome,
            LightDark::Light,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        )
    } else {
        Theme::dark()
    };
    let palette = palette::ui_palette(&theme);
    let mut app = App::new();
    if demo {
        app.insert_resource(DemoMode);
    }
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: product::BEVY_NAME.to_owned(),
            name: Some(product::BEVY_APP_ID.to_owned()),
            resolution: capture_window_resolution(),
            // Borderless content, GPUI's chrome grammar: the navigation strip
            // is the window's top product surface and the compositor owns the
            // frame. A server title bar would paint a second identity layer
            // the other frontends do not carry.
            decorations: false,
            ..Window::default()
        }),
        ..WindowPlugin::default()
    }));
    app.insert_resource(ClearColor(palette.window_clear));
    app.add_plugins(FrontendWindowPlugin {
        runtime: shared,
        palette,
    });
    if !demo {
        app.insert_non_send(production_history_runtime());
        app.add_systems(
            PreUpdate,
            crate::pages::history::drain_history_system.before(crate::drain::drain_system),
        );
    }
    if let Some(page) = capture_page() {
        app.insert_resource(Route { page });
    }
    if demo {
        app.init_resource::<CaptureMarkerState>();
        app.add_systems(Update, emit_capture_marker);
    }
    match app.run() {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(_) => ExitCode::FAILURE,
    }
}

fn emit_capture_marker(
    route: Res<Route>,
    contents: Query<&crate::app::PageContent>,
    mut marker: ResMut<CaptureMarkerState>,
) {
    if marker.0 || contents.iter().all(|content| content.page != route.page) {
        return;
    }
    let page = capture_page_name(route.page);
    println!("BEVY_CAPTURE_MARKER event=frame_ready mode=demo page={page}");
    println!("BEVY_CAPTURE_MARKER event=target_ready mode=demo page={page}");
    marker.0 = true;
}

fn capture_page_name(page: crate::app::Page) -> &'static str {
    if capture_wants_service_logs() {
        return "service-logs";
    }
    match page {
        crate::app::Page::Processes => "applications",
        crate::app::Page::Performance => "performance",
        crate::app::Page::Services => "services",
        crate::app::Page::System => "system",
        crate::app::Page::Startup => "startup",
        crate::app::Page::Sessions => "users",
        crate::app::Page::Alerts => "alerts",
        crate::app::Page::Settings => "settings",
        crate::app::Page::AppHistory => "app-history",
    }
}

/// The capture scenario that renders the Services page with the log panel
/// open over a seeded fixture feed. Production routing is untouched: the
/// env var exists only inside the demo capture composition.
fn capture_wants_service_logs() -> bool {
    std::env::var("TM_BEVY_CAPTURE_PAGE").is_ok_and(|value| value.trim() == "service-logs")
}

/// Capture fixture: open the log stream for the first demo service and
/// pre-fill the feed with a bounded, deterministic journal excerpt. The
/// scenario renders the real panel over this state; production never runs it.
fn seed_service_log_fixture(shell: &mut taskmanager_shell::ShellApp) {
    use taskmanager_core::core::services::{
        ServiceLogEntry, ServiceLogLevel, ServiceLogLevelFilter, ServiceLogQuery,
        ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter,
    };
    let Some(service) = shell.sorted_services().first().cloned() else {
        return;
    };
    let service_id = service.id.clone();
    let _ = shell.open_service_log_for(service_id.clone());
    let lines: &[&str] = &[
        "Started Network Manager.",
        "Reached target Network.",
        "wlan0: link becomes ready",
        "Starting Network Manager Script Dispatcher Service...",
        "Started Network Manager Script Dispatcher Service.",
        "dhcp: lease renewed (3600s)",
        "wlan0: Gained IPv6LL",
        "device (wlan0): state change: activated -> deactivating",
        "device (wlan0): state change: deactivating -> disconnected",
        "wlan0: link is not ready",
        "device (wlan0): state change: disconnected -> prepare",
        "device (wlan0): supplicant interface state: scanning -> authenticating",
        "device (wlan0): supplicant interface state: authenticating -> associating",
        "device (wlan0): supplicant interface state: associating -> 4way_handshake",
        "device (wlan0): supplicant interface state: 4way_handshake -> completed",
        "wlan0: link becomes ready",
        "device (wlan0): state change: config -> activated",
        "dhcp: request granted",
        "address added: 192.168.1.42/24",
        "route added: default via 192.168.1.1",
    ];
    let base_micros = crate::drain::unix_now_ms().saturating_sub(60_000) * 1_000;
    let entries: Vec<ServiceLogEntry> = lines
        .iter()
        .enumerate()
        .map(|(index, message)| ServiceLogEntry {
            cursor: format!("demo:{index:04}"),
            realtime_timestamp_micros: Some(base_micros + index as u64 * 1_500_000),
            priority: Some(6),
            level: ServiceLogLevel::Unknown,
            message: (*message).to_owned(),
        })
        .collect();
    let query = ServiceLogQuery {
        service_id: service_id.clone(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    let snapshot = ServiceLogStreamSnapshot {
        query: query.clone(),
        state: ServiceLogStreamState::from_query_entries(&query, entries),
    };
    if let Some(open) = shell.service_log.as_mut() {
        open.feed.apply_at(snapshot, crate::drain::unix_now_ms());
    }
}

/// Compose the history connector at the native edge. The config preference is
/// read once at startup through the bounded app-host client; disabled history
/// does not launch a writer, replay worker, or frontend connector.
fn production_history_runtime() -> crate::pages::history::HistoryRuntime {
    let host = NativeAppHost::production();
    let enabled = host
        .config_client()
        .ok()
        .map(|mut client| {
            client
                .wait_for_initial(Duration::from_millis(250))
                .snapshot()
                .history_persistence
        })
        .unwrap_or(false);
    let mut runtime = crate::pages::history::HistoryRuntime::default();
    runtime.request(enabled);
    if enabled {
        runtime.install_connector(host.history_frontend_connector());
    }
    runtime
}

fn capture_page() -> Option<crate::app::Page> {
    let value = std::env::var("TM_BEVY_CAPTURE_PAGE").ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "applications" | "processes" => Some(crate::app::Page::Processes),
        "performance" => Some(crate::app::Page::Performance),
        "services" => Some(crate::app::Page::Services),
        "service-logs" => Some(crate::app::Page::Services),
        "system" => Some(crate::app::Page::System),
        "startup" => Some(crate::app::Page::Startup),
        "users" | "sessions" => Some(crate::app::Page::Sessions),
        "alerts" => Some(crate::app::Page::Alerts),
        "settings" => Some(crate::app::Page::Settings),
        "app-history" | "history" => Some(crate::app::Page::AppHistory),
        _ => None,
    }
}

fn capture_window_resolution() -> WindowResolution {
    let raw = std::env::var("TM_BEVY_WINDOW_SIZE").unwrap_or_default();
    let (width, height) = raw
        .split_once('x')
        .and_then(|(width, height)| Some((width.parse::<u32>().ok()?, height.parse::<u32>().ok()?)))
        .filter(|(width, height)| *width >= 720 && *height >= 480)
        .unwrap_or((1180, 780));
    WindowResolution::new(width, height)
}

/// Wires the frontend-owned seams and app shell into one bevy `App`.
pub(crate) struct FrontendWindowPlugin {
    pub(crate) runtime: &'static SharedRuntime,
    pub(crate) palette: UiPalette,
}

impl Plugin for FrontendWindowPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(crate::app::SharedRuntimeHandle {
            shared: self.runtime,
        });
        app.insert_non_send(crate::app::FrontendTrack {
            shell: if app.world().contains_resource::<DemoMode>() {
                let mut shell = demo_shell();
                if capture_wants_service_logs() {
                    seed_service_log_fixture(&mut shell);
                }
                shell
            } else {
                taskmanager_shell::ShellApp::new()
            },
            initial_refresh_submitted: app.world().contains_resource::<DemoMode>(),
            process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion::default(),
        });
        app.insert_resource(WindowPalette {
            inner: self.palette.clone(),
        });
        // The route always has an immutable history projection available;
        // production adds the non-send connector runtime below, while
        // headless compositions remain honestly Disabled.
        app.init_resource::<HistoryProjectionResource>();
        app.init_resource::<PerformanceLayoutState>().add_systems(
            PostUpdate,
            (sync_performance_layout, sync_control_visuals).chain(),
        );
        app.init_resource::<PlaceholderFonts>();
        app.init_resource::<crate::drain::FeedbackCache>();
        app.add_observer(rewrite_summary_line);
        app.add_observer(rewrite_feedback_line);
        app.add_observer(style_text_role);
        // Accessibility resource plumbing: the winit AccessKit bridge (the
        // `accesskit_unix` feature) publishes `AccessibilityNode` components
        // to the platform tree through these resources. `DefaultPlugins`
        // already adds the plugin when that feature is on, so guard against
        // the duplicate-add panic instead of assuming a composition order.
        if !app.is_plugin_added::<bevy::a11y::AccessibilityPlugin>() {
            app.add_plugins(bevy::a11y::AccessibilityPlugin);
        }
        app.add_plugins(AppShellPlugin);
        crate::icons::register(app);
        crate::confirmation::register(app);
        crate::menu_modal::register::<crate::pages::services::menu::ServiceMenuCtx>(app);
        crate::menu_modal::register::<crate::pages::startup::menu::StartupMenuCtx>(app);
        crate::menu_modal::register::<crate::pages::sessions::menu::SessionMenuCtx>(app);
        crate::semantic::register(app);
        app.add_systems(
            Startup,
            (
                register_embedded_fonts,
                crate::icons::build_icon_plates,
                spawn_app_shell,
            )
                .chain(),
        );
        if !app.world().contains_resource::<DemoMode>() {
            app.add_systems(PreUpdate, drain::drain_system);
        }
    }
}

/// Build the capture-only shell with a warm, deterministic graph window.
/// `demo_app` intentionally starts with one honest sample for general fixture
/// consumers; the visual contract needs a populated curve window, so the
/// capture shell records a short adjacent sequence without changing
/// production collection or the shared fixture's semantics.
fn demo_shell() -> taskmanager_shell::ShellApp {
    let mut shell = taskmanager_shell::demo_app();
    if let Some(seed) = shell.projection().snapshot.clone() {
        for offset in 1..=24_u64 {
            let mut next = seed.clone();
            next.timestamp_ms = next
                .timestamp_ms
                .saturating_add(offset.saturating_mul(1_000));
            next.cpu = demo_cpu_frame(&next.cpu, next.timestamp_ms, offset);
            taskmanager_shell::fixture::record_demo_history_frame(&mut shell, &next, None, None);
        }
    }
    shell
}

fn demo_cpu_frame(seed: &CpuMetrics, timestamp_ms: u64, offset: u64) -> CpuMetrics {
    let usage = if offset == 24 {
        37.4
    } else {
        24.0 + f32::from((offset.saturating_mul(7) % 26) as u8)
    };
    let mut observations = seed.scalar_observations().clone();
    observations.global_usage_pct = ScalarObservation::available(usage, timestamp_ms);
    let core_values = (0..seed.current_core_usage_len())
        .map(|index| (usage + (index as f32 * 4.0) - (offset % 3) as f32 * 2.0).clamp(0.0, 100.0))
        .collect();
    observations.core_usage_group = ScalarObservationGroup::available(core_values, timestamp_ms);
    let mut frame = CpuMetrics::from_observations(observations);
    frame.brand = seed.brand.clone();
    frame.frequency_source = seed.frequency_source;
    frame.temperature_source = seed.temperature_source;
    frame.physical_cores = seed.physical_cores;
    frame.logical_cores = seed.logical_cores;
    frame.l1d_cache_kb = seed.l1d_cache_kb;
    frame.l1i_cache_kb = seed.l1i_cache_kb;
    frame.l2_cache_kb = seed.l2_cache_kb;
    frame.l3_cache_kb = seed.l3_cache_kb;
    frame.performance_policy = seed.performance_policy.clone();
    frame
}

/// Register the bundled faces every other frontend embeds (ADR-026) into the
/// bevy font store. `embedded_fonts()` yields MiSans VF followed by Roboto
/// Mono, the same UI/metric-role order used by GPUI and Iced.
fn register_embedded_fonts(mut fonts: ResMut<Assets<Font>>, mut handles: ResMut<PlaceholderFonts>) {
    let mut embedded = taskmanager_assets::embedded_fonts().into_iter();
    handles.ui = embedded
        .next()
        .map(|bytes| fonts.add(Font::from_bytes(bytes.into_owned())));
    handles.mono = embedded
        .next()
        .map(|bytes| fonts.add(Font::from_bytes(bytes.into_owned())));
    if handles.ui.is_none() {
        eprintln!(
            "taskforest-b: embedded font table empty (expected the {} face); \
             text falls back to the default font source",
            taskmanager_assets::EMBEDDED_FONT_FAMILIES
                .first()
                .copied()
                .unwrap_or("ui")
        );
    }
    if handles.mono.is_none() {
        eprintln!(
            "taskforest-b: embedded mono face missing (expected the {} face); \
             metric text falls back to the UI face",
            taskmanager_assets::EMBEDDED_FONT_FAMILIES
                .get(1)
                .copied()
                .unwrap_or("mono")
        );
    }
}

/// Resolve a role's registered face. A missing mono registration degrades to
/// the registered UI face; a missing UI registration retains Bevy's honest
/// default source rather than claiming an unavailable handle.
fn role_font_source(fonts: &PlaceholderFonts, role: Role) -> FontSource {
    let handle = match role {
        Role::Mono => fonts.mono.clone().or_else(|| fonts.ui.clone()),
        Role::Heading | Role::Body | Role::Caption => fonts.ui.clone(),
    };
    handle.map(FontSource::Handle).unwrap_or_default()
}

/// Observer: stamp palette metrics + ink onto every text node as its role
/// lands. Runs for the startup shell, every page remount, and every future
/// widget insert — the single place typography becomes bevy values.
fn style_text_role(
    trigger: On<Add, TextRole>,
    mut texts: Query<(&TextRole, &mut TextFont, &mut TextColor)>,
    palette: Res<WindowPalette>,
    fonts: Res<PlaceholderFonts>,
) {
    let Ok((role, mut metrics, mut ink)) = texts.get_mut(trigger.event().entity) else {
        return;
    };
    let (font, color) = match role.0 {
        Role::Heading => (palette.inner.heading.clone(), palette.inner.heading_color),
        Role::Body => (palette.inner.body.clone(), palette.inner.body_color),
        Role::Caption => (palette.inner.caption.clone(), palette.inner.dim_color),
        Role::Mono => (palette.inner.mono.clone(), palette.inner.body_color),
    };
    *metrics = TextFont {
        font: role_font_source(&fonts, role.0),
        ..font
    };
    ink.0 = color;
}

/// Repaint only product-owned interactive controls whose interaction or
/// selected state changed. Bevy's `Button` provides the required
/// `Interaction`/`Pressed` state; this system supplies the shared BSN skin.
#[allow(clippy::type_complexity)]
fn sync_control_visuals(
    palette: Res<WindowPalette>,
    mut controls: Query<
        (
            &ControlVisual,
            Option<&PickingInteraction>,
            Has<Pressed>,
            &mut BackgroundColor,
        ),
        Or<(
            Changed<ControlVisual>,
            Changed<PickingInteraction>,
            Changed<Pressed>,
        )>,
    >,
) {
    for (visual, interaction, pressed, mut fill) in &mut controls {
        fill.0 = control_background(
            visual,
            interaction.copied().unwrap_or_default(),
            pressed,
            &palette.inner,
        );
    }
}

/// Observer: rewrite the summary line when the drain folded a new capability
/// snapshot. Observer idiom per the upstream watch doc — no polling, no
/// change-detection loop.
fn rewrite_summary_line(
    summary: On<CapabilitySummaryChanged>,
    mut lines: Query<&mut Text, With<SummaryLine>>,
) {
    // The shell spawns exactly one summary line; a structurally broken
    // world (zero or many) is skipped rather than guessed at.
    if let Ok(mut line) = lines.single_mut() {
        line.0 = summary.event().0.clone();
    }
}

/// Observer: rewrite the feedback line when the shell's status copy changed.
/// Empty feedback keeps the line blank — never a fabricated status.
fn rewrite_feedback_line(
    feedback: On<crate::drain::FeedbackChanged>,
    mut lines: Query<&mut Text, With<FeedbackLine>>,
) {
    if let Ok(mut line) = lines.single_mut() {
        line.0 = feedback.event().0.clone();
    }
}

/// The full app shell as one declarative scene: the product navigation strip
/// over the routed page's content. One chrome shape for every route — the
/// same strip grammar GPUI renders — with the summary/feedback caption band
/// kept only where no page-local status surface exists.
fn app_shell_scene(palette: &UiPalette, route: Page, summary: String) -> Box<dyn Scene> {
    let strip: Box<dyn Scene> = Box::new(nav_strip_scene(route, palette));
    if route == Page::Performance {
        // GPUI chrome parity: the Performance page fills the whole window
        // under the strip with no extra status band.
        return Box::new(bsn! {
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
            }
            BackgroundColor({ palette.window_clear })
            AppShellRoot
            Children [
                ( { strip } ),
                (
                    Node {
                        width: percent(100),
                        height: percent(100),
                        flex_grow: 1.0,
                        justify_content: JustifyContent::FlexStart,
                        align_items: AlignItems::Stretch,
                        padding: UiRect::all(Val::Px(space_8())),
                    }
                    BackgroundColor({ palette.content_bg })
                    ContentSlot
                ),
            ]
        });
    }
    Box::new(standard_app_shell_scene(palette, summary, strip))
}

fn standard_app_shell_scene(
    palette: &UiPalette,
    summary: String,
    strip: Box<dyn Scene>,
) -> impl Scene + use<> {
    let radius = palette.panel_radius_px;
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
        }
        BackgroundColor({ palette.window_clear })
        AppShellRoot
        Children [
            ( { strip } ),
            (
                // The shell's status band: capability summary and the drain's
                // typed feedback, caption-sized so it informs without adding
                // a second chrome layer.
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexEnd,
                    padding: UiRect::horizontal(Val::Px(space_12())),
                }
                Children [
                    ( Text(summary) SummaryLine TextRole(Role::Caption) ),
                    ( Text("") FeedbackLine TextRole(Role::Caption) ),
                ]
            ),
            (
                Node {
                    width: percent(100),
                    height: percent(100),
                    flex_grow: 1.0,
                    justify_content: JustifyContent::FlexStart,
                    align_items: AlignItems::Stretch,
                    padding: UiRect::all(Val::Px(space_8())),
                    border_radius: BorderRadius::all(Val::Px(radius)),
                }
                BackgroundColor({ palette.content_bg })
                ContentSlot
            ),
        ]
    }
}

/// Startup spawn: one camera for the ui render node to target, then the
/// shell scene. The initial route comes from the (default) route resource —
/// no window-local page state exists.
fn spawn_app_shell(
    palette: Res<WindowPalette>,
    route: Res<Route>,
    demo: Option<Res<DemoMode>>,
    mut commands: Commands,
) {
    commands.spawn(Camera2d);
    let summary = if demo.is_some() {
        "Demo snapshot · no host actions".to_owned()
    } else {
        "waiting for the first capability snapshot…".to_owned()
    };
    commands.spawn_scene(app_shell_scene(&palette.inner, route.page, summary));
}

#[cfg(test)]
#[path = "../tests/headless/window.rs"]
pub(crate) mod tests;
