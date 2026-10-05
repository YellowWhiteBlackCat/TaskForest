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
//! compositions separate is important in Bevy 0.20 because `AssetPlugin`,
//! `ScenePlugin`, and input plugins are singleton infrastructure.

use std::process::ExitCode;
use std::time::Duration;

use bevy::DefaultPlugins;
use bevy::app::{App, AppExit, Plugin, PluginGroup, PostUpdate, PreUpdate, Startup};
use bevy::asset::{Assets, Handle};
use bevy::camera::{Camera2d, ClearColor};
use bevy::ecs::component::Component;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Changed, Has, Or, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{CommandsSceneExt, bsn};
use bevy::text::{Font, FontSource, TextColor, TextFont};
use bevy::ui::Pressed;
use bevy::ui::prelude::BackgroundColor;
use bevy::ui::widget::Text;
use bevy::window::{Window, WindowPlugin};
use taskmanager_app_host::NativeAppHost;

mod capture_marker;
mod capture_state;
use capture_marker::emit_capture_marker;
use capture_state::{initialize_capture_state, is_demo, is_production};

use taskmanager_assets::product;
use taskmanager_theme::Theme;

pub(crate) mod chrome;
use chrome::app_shell_scene;
mod appearance;
use appearance::demo_theme_from_env;

use crate::app::{AppShellPlugin, Route};
use crate::capture::{capture_page, capture_scenario_target, capture_window_resolution};
use crate::drain::{self, CapabilitySummaryChanged};
use crate::pages::history::HistoryProjectionResource;
use crate::pages::performance::{PerformanceLayoutState, sync_performance_layout};
use crate::pages::settings::ThemePreferences;
use crate::pages::system::diagnostic_modal::DiagnosticRuntime;
use crate::palette::{self, UiPalette};
use crate::runtime::SharedRuntime;
use crate::widgets::controls::{ControlVisual, control_background};
use taskmanager_app_host::acquire_single_instance;
use taskmanager_application::i18n::t;
use taskmanager_assets::EMBEDDED_FONT_FAMILIES;
use taskmanager_assets::embedded_fonts;
use taskmanager_platform_contract::InstanceRole;
use taskmanager_shell::ShellApp;

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
struct CaptureMarkerState {
    emitted: bool,
    history_open_requested: bool,
    data_presented: bool,
    hardware_scroll_requested: bool,
}

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
    let _instance_guard = if !demo {
        let (tx, _rx) = std::sync::mpsc::channel();
        match acquire_single_instance(product::BEVY_NAME, tx) {
            Ok(InstanceRole::Primary(guard)) => Some(guard),
            Ok(InstanceRole::Secondary) => {
                eprintln!("taskforest-b: already running, waking existing instance");
                return ExitCode::SUCCESS;
            }
            Err(failure) => {
                eprintln!("taskforest-b: cannot acquire single-instance lock: {failure:?}");
                None
            }
        }
    } else {
        None
    };

    // Production keeps the cold-start dark theme here; the persisted
    // appearance, locale, and cadence/capacity restore in the `Startup`
    // restore system (before the first frame renders) from the same config
    // tokens the settings page writes. Capture uses the light reference skin
    // so the visual gate compares the actual product structure and
    // typography, not a fixture-only color inversion; the shared `TM_SKIN`
    // testing override can select another skin/mode there (unset keeps the
    // light reference skin byte-for-byte).
    let theme = if demo {
        demo_theme_from_env()
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
            // Under visual capture, keep borderless content so pixel receipts match
            // reference frames without compositor window borders; under normal
            // desktop launches, follow the repo-wide SSD direction (decorations: true)
            // so the window has native titlebar, dragging, and minimize/close controls.
            decorations: std::env::var_os("TASKFOREST_CAPTURE_SCENARIO").is_none(),
            ..Window::default()
        }),
        ..WindowPlugin::default()
    }));
    app.insert_resource(ClearColor(palette.window_clear));
    app.add_plugins(FrontendWindowPlugin {
        runtime: shared,
        palette,
    });
    let history_capture = demo
        && capture_scenario_target().is_some_and(|target| {
            matches!(target, "history-replay" | "application-history-replay")
        });
    if !demo || history_capture {
        app.insert_non_send(production_history_runtime());
        app.add_systems(
            PreUpdate,
            crate::pages::history::drain_history_system.before(crate::drain::drain_system),
        );
    }
    if !demo {
        app.add_systems(
            Startup,
            crate::pages::settings::restore_persisted_preferences,
        );
        if let Ok(client) = NativeAppHost::production().diagnostic_bundle_client() {
            let mut diagnostic = DiagnosticRuntime::default();
            diagnostic.install(client);
            app.insert_resource(diagnostic);
        }

        let (tray_controller, tray_rx) = crate::tray::spawn_tray_host(false);
        app.insert_resource(crate::tray::TrayResource::new(tray_controller, tray_rx));
    } else {
        app.insert_resource(crate::tray::TrayResource::empty());
    }
    if let Some(page) = capture_page() {
        app.insert_resource(Route { page });
    }
    if demo {
        app.init_resource::<CaptureMarkerState>();
        app.add_systems(
            PostUpdate,
            emit_capture_marker
                .after(crate::pages::performance::device_curves::paint_curves)
                .after(crate::pages::performance::replay::paint_charts)
                .after(crate::pages::system::dashboard::paint_charts),
        );
    }
    match app.run() {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(_) => ExitCode::FAILURE,
    }
}

/// Compose the history connector at the native edge. The config preference is
/// read once at startup through the bounded app-host client; disabled history
/// does not launch a writer or replay worker. The inert connector permits a
/// later canonical Settings change to enable persistence without restarting.
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
    runtime.install_connector(host.history_frontend_connector());
    runtime
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
            shell: ShellApp::new(),
            initial_refresh_submitted: false,
            process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion::default(),
        });
        app.insert_resource(WindowPalette {
            inner: self.palette.clone(),
        });
        if std::env::var("TM_BEVY_CAPTURE_PAGE").is_ok_and(|v| v.trim() == "sidebar-hidden") {
            app.insert_resource(crate::pages::performance::PerformanceSidebarVisible(false));
        }
        crate::pages::history::control::register(app);
        crate::pages::performance::replay::register(app);
        // The route always has an immutable history projection available;
        // production adds the non-send connector runtime below, while
        // headless compositions remain honestly Disabled.
        app.init_resource::<HistoryProjectionResource>();
        app.init_resource::<PerformanceLayoutState>().add_systems(
            PostUpdate,
            (
                sync_performance_layout,
                sync_control_visuals,
                crate::widgets::table::paint_zero_values,
            )
                .chain(),
        );
        app.init_resource::<PlaceholderFonts>();
        app.init_resource::<crate::drain::FeedbackCache>();
        crate::window_surface::register(app);
        crate::saved_views::register(app);
        crate::about_modal::register(app);
        crate::system_information_modal::register(app);
        crate::first_run_modal::register(app);
        crate::pages::system::diagnostic_modal::register(app);
        crate::pages::system::dashboard::register(app);
        crate::pages::system::register(app);
        crate::pages::services::register(app);
        crate::pages::alerts::editor::register(app);
        crate::pages::startup::register(app);
        crate::pages::sessions::register(app);
        crate::pages::process_tree::register(app);
        crate::pages::performance::register(app);
        crate::pages::history::scene::register(app);
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
        crate::pages::processes::affinity::register(app);
        crate::pages::processes::properties_modal::register(app);
        crate::pages::processes::columns_modal::register(app);
        crate::pages::services::details_modal::register(app);
        crate::menu_modal::register::<crate::pages::processes::menu::ProcessMenuCtx>(app);
        crate::menu_modal::register::<crate::pages::services::menu::ServiceMenuCtx>(app);
        crate::menu_modal::register::<crate::pages::startup::menu::StartupMenuCtx>(app);
        crate::menu_modal::register::<crate::pages::sessions::menu::SessionMenuCtx>(app);
        crate::semantic::register(app);
        crate::tooltip::register(app);
        crate::text_selection::register(app);
        crate::focus_visible::register(app);
        app.add_systems(
            Startup,
            (
                initialize_capture_state,
                register_embedded_fonts,
                crate::icons::build_icon_plates,
                spawn_app_shell,
            )
                .chain()
                .before(crate::confirmation::init_capture_confirmation),
        );
        app.add_systems(PreUpdate, drain::drain_system.run_if(is_production));
        app.add_systems(PreUpdate, drain::drain_demo_effects.run_if(is_demo));
    }
}

/// Register the bundled faces every other frontend embeds (ADR-026) into the
/// bevy font store. `embedded_fonts()` yields MiSans VF followed by Roboto
/// Mono, the same UI/metric-role order used by GPUI and Iced.
fn register_embedded_fonts(mut fonts: ResMut<Assets<Font>>, mut handles: ResMut<PlaceholderFonts>) {
    let mut embedded = embedded_fonts().into_iter();
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
            EMBEDDED_FONT_FAMILIES.first().copied().unwrap_or("ui")
        );
    }
    if handles.mono.is_none() {
        eprintln!(
            "taskforest-b: embedded mono face missing (expected the {} face); \
             metric text falls back to the UI face",
            EMBEDDED_FONT_FAMILIES.get(1).copied().unwrap_or("mono")
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
    trigger: On<Add<TextRole>>,
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

/// Interactive controls whose interaction or selected state changed this
/// frame. A named alias keeps the skin system's signature readable; the raw
/// tuple would trip `clippy::type_complexity`.
type ChangedControlVisuals<'w, 's> = Query<
    'w,
    's,
    (
        &'static ControlVisual,
        Option<&'static PickingInteraction>,
        Has<Pressed>,
        &'static mut BackgroundColor,
    ),
    Or<(
        Changed<ControlVisual>,
        Changed<PickingInteraction>,
        Changed<Pressed>,
    )>,
>;

/// Repaint only product-owned interactive controls whose interaction or
/// selected state changed. Bevy's `Button` provides the required
/// `Interaction`/`Pressed` state; this system supplies the shared BSN skin.
fn sync_control_visuals(palette: Res<WindowPalette>, mut controls: ChangedControlVisuals<'_, '_>) {
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
    mut lines: Query<(&mut Text, Option<&FeedbackLine>, Option<&SummaryLine>)>,
) {
    let summary = lines
        .iter()
        .find_map(|(line, _, marker)| marker.map(|_| line.0.clone()));
    for (mut line, feedback_marker, _) in &mut lines {
        if feedback_marker.is_some() {
            // The demo shell seeds its informational feedback with the same
            // text used by the summary line. Keep one visual owner instead of
            // painting the sentence twice in the standard (non-Performance)
            // shell.
            line.0 = if summary.as_deref() == Some(feedback.event().0.as_str()) {
                String::new()
            } else {
                feedback.event().0.clone()
            };
        }
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
    commands.spawn_scene(bsn! { Camera2d });
    let summary = if demo.is_some() {
        t("status.demo_snapshot").to_owned()
    } else {
        t("status.waiting_for_snapshot").to_owned()
    };
    commands.spawn_scene(app_shell_scene(&palette.inner, route.page, summary));
}

#[cfg(test)]
#[path = "../tests/headless/window.rs"]
pub(crate) mod tests;
