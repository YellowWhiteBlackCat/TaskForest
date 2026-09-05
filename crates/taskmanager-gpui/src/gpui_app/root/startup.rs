//! Application startup and persisted shell configuration.

use super::graph_options::normalize_graph_data_points;
use super::sidebar_preferences::normalize_sidebar_preferences;
use super::{
    ConfigClient, Duration, PlatformClient, RefreshRequest, RootView, TelemetryStore, Theme,
    TopPage, apply_process_config, config_from_view, i18n, platform_submission_time_ms, responsive,
};
use crate::gpui_app::theme::{detect, forced_skin_from_env};
use gpui::{
    AnyWindowHandle, App, AppContext, AsyncApp, Bounds, Entity, SharedString, Timer,
    TitlebarOptions, WeakEntity, WindowBounds, WindowDecorations, WindowOptions, point, px, size,
};
use taskmanager_assets::product;
use taskmanager_telemetry_store::CorrelatedTelemetryStamp;
use taskmanager_theme::{HighContrast, resolve_fonts};
use taskmanager_ui::theme_binding::{background_appearance, detect_font_availability};
use tracing::{error, warn};

mod appearance;
use appearance::observe_startup_appearance;

mod capture_systems;
mod demo;
pub(crate) use demo::init_demo;

mod config_tokens;
use crate::gpui_app::chrome::WindowDecorationsPreference;
pub(super) use config_tokens::{
    FONT_TOKEN_SYSTEM, color_scheme_from_token, font_pref_from_config, page_token,
    skin_preference_from_config,
};
use config_tokens::{
    apply_cfg_to_theme, resolve_app_id, resolve_startup_page, startup_page_from_token,
    text_rendering_from_token,
};

mod config_sync;
use config_sync::{
    apply_root_persisted_projection, drain_config_publications, initial_config_recovery_message,
    persist_config_if_due,
};

/// Window-frame policy: the persisted preference picks the mode we REQUEST.
/// `System` (the default) and `Native` both ask for server-side decorations
/// so Windows/macOS/KDE own caption buttons, hit testing, snap/maximize
/// affordances and accessibility semantics; `Custom` asks for client
/// decorations (the app-drawn titlebar with transparent rounded corners). A
/// compositor may refuse either request; render then follows the live
/// `Decorations` fact and uses the audited CSD fallback rather than mixing
/// two titlebars, and an explicitly refused preference is reported honestly
/// by the render-time outcome check (`chrome::decoration_outcome_notice`).
fn requested_window_decorations(pref: WindowDecorationsPreference) -> WindowDecorations {
    pref.requested_decorations()
}

enum InstanceStartup {
    Continue {
        guard: Option<Box<dyn taskmanager_platform_contract::InstanceGuard>>,
        events: std::sync::mpsc::Receiver<taskmanager_platform_contract::InstanceEvent>,
    },
    Secondary,
}

fn acquire_instance(capture_mode: bool) -> InstanceStartup {
    let (events_tx, events) = std::sync::mpsc::channel();
    if capture_mode {
        return InstanceStartup::Continue {
            guard: None,
            events,
        };
    }
    match taskmanager_app_host::acquire_single_instance(product::GPUI_NAME, events_tx) {
        Ok(taskmanager_platform_contract::InstanceRole::Primary(guard)) => {
            InstanceStartup::Continue {
                guard: Some(guard),
                events,
            }
        }
        Ok(taskmanager_platform_contract::InstanceRole::Secondary) => InstanceStartup::Secondary,
        Err(failure) => {
            warn!(
                ?failure,
                "single-instance unavailable; continuing without it"
            );
            InstanceStartup::Continue {
                guard: None,
                events,
            }
        }
    }
}

struct RootStartupFacts {
    history_connector: Result<
        taskmanager_app_host::HistoryFrontendConnector,
        taskmanager_app_host::HistoryFrontendConnectorStartError,
    >,
    appearance: appearance::StartupAppearanceObservation,
    font_pref: taskmanager_theme::FontPreference,
    skin_preference: Option<taskmanager_theme::Skin>,
    language_preference: Option<i18n::Language>,
    font_availability: taskmanager_theme::FontAvailability,
    local_time_rules: taskmanager_core::core::time::LocalTimeRulesObservation,
}

fn apply_root_startup_config(
    view: &mut RootView,
    cfg: &taskmanager_core::core::config::Config,
    facts: RootStartupFacts,
    has_explicit_page_override: bool,
    cx: &mut gpui::Context<RootView>,
) {
    view.history_runtime.request(cfg.history_persistence);
    view.history_runtime
        .install_connector(facts.history_connector);
    view.sync_history_persistence_sink();
    if let Some(reason) = view.history_runtime.unavailable_reason() {
        view.shell.report_notice(
            taskmanager_shell::FeedbackSource::Persistence,
            taskmanager_shell::FeedbackSeverity::Warning,
            taskmanager_shell::FeedbackLifecycle::UntilReplaced,
            i18n::t("perf.replay.startup_failure_notice").replace("{kind}", reason.stable_code()),
        );
    }
    view.desktop_appearance = facts.appearance.value;
    view.desktop_appearance_sources = facts.appearance.sources;
    view.platform_failures = facts.appearance.failures;
    view.font_availability = facts.font_availability;
    view.local_time_rules = facts.local_time_rules;
    apply_root_persisted_projection(view, cfg);
    let mut presentation = view.presentation_snapshot();
    presentation.appearance.font = facts.font_pref;
    presentation.appearance.skin = facts.skin_preference;
    presentation.appearance.language = facts.language_preference;
    view.replace_presentation(presentation);
    view.page = resolve_startup_page(
        &cfg.startup_page,
        &cfg.last_page,
        view.page,
        has_explicit_page_override,
    );
    view.request_page_data(view.page);
    if view.capture_evidence.apps_zero_gray_enabled() {
        view.page = TopPage::Apps;
        view.set_gray_zero_values(true, cx);
        view.set_process_sort(
            taskmanager_shell::SortCol::Name,
            taskmanager_shell::SortDir::Asc,
        );
        view.set_process_query("capture-");
    }
    if !view.capture_evidence.first_run_fixture_enabled() {
        view.request_first_run_observation(cx);
    }
}

fn spawn_update_loop(
    cx: &mut App,
    weak: WeakEntity<RootView>,
    window_handle: AnyWindowHandle,
    mut config_client: ConfigClient,
    mut applied_config_revision: Option<taskmanager_application::ConfigRevision>,
) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        let mut tick = 0u32;
        let mut submitted_presentation = None;
        loop {
            Timer::after(Duration::from_millis(200)).await;
            tick = tick.wrapping_add(1);
            if weak.upgrade().is_none() {
                let _ = cx.update(|app| app.quit());
                break;
            }
            let _ = weak.update(cx, |view, cx| {
                let capability_snapshot = view
                    .platform
                    .as_ref()
                    .map(|platform| platform.capabilities().snapshot());
                if capability_snapshot
                    .is_some_and(|snapshot| view.shell.apply_capability_snapshot(snapshot))
                {
                    cx.notify();
                }
                view.drain_instance_events(cx, window_handle);
                super::tray::drain_tray_events(view, cx, window_handle);
                view.drain_history_replay_completions(cx);
                view.reconcile_gpu_engines_visibility(cx);
                if view.drain_snapshot_export_completions() {
                    cx.notify();
                }
                if view.drain_window_capture_completions() {
                    cx.notify();
                }
                let notice_before = view.shell.feedback_notice().is_some();
                view.shell.advance_feedback_time(Duration::from_millis(200));
                if notice_before && view.shell.feedback_notice().is_none() {
                    cx.notify();
                }
            });
            let refresh_paused = weak
                .update(cx, |view, _cx| view.telemetry_refresh_policy.is_paused())
                .unwrap_or(true);
            let platform_batch = if refresh_paused {
                None
            } else {
                weak.update(cx, |view, _cx| {
                    view.platform
                        .as_mut()
                        .and_then(|platform| platform.try_drain().ok())
                        .filter(|batch| !batch.is_empty())
                })
                .ok()
                .flatten()
            };
            if let Some(batch) = platform_batch {
                let _ = weak.update(cx, |view, cx| {
                    capture_systems::apply_platform_batch(view, batch, cx);
                });
            }
            if !refresh_paused {
                let _ = weak.update(cx, |view, cx| {
                    if view.telemetry_frame_state.is_collecting() {
                        cx.notify();
                    }
                    poll_window_systems(view, cx);
                });
                let _ = weak.update(cx, |view, _cx| {
                    view.run_scheduled_refresh(platform_submission_time_ms());
                });
            }
            drain_config_publications(&weak, cx, &mut config_client, &mut applied_config_revision);
            persist_config_if_due(tick, &weak, cx, &config_client, &mut submitted_presentation);
        }
    })
    .detach();
}

fn poll_window_systems(view: &mut RootView, cx: &mut gpui::Context<RootView>) {
    let health_capture = view.sync_capture_system_health_system();
    let health_ready = health_capture.ready();
    if let super::capture::SystemHealthCaptureOutcome::ReadyWithConfirmation(confirmation) =
        health_capture
    {
        view.request_system_health_self_test_confirmation(confirmation);
    }
    let dynamic_ready = view.sync_capture_dynamic_device_system();
    let live_dynamic_ready = view.sync_capture_live_dynamic_device_system();
    if dynamic_ready {
        let stamp = CorrelatedTelemetryStamp::from_accepted_event(
            u64::MAX,
            view.power_supplies().timestamp_ms.saturating_add(1),
        );
        if let Some(stamp) = stamp {
            let _ = view
                .telemetry_ingestor
                .ingest_correlated_power_supplies(stamp, view.power_supplies());
            let _ = view
                .telemetry_ingestor
                .ingest_correlated_sensors(stamp, view.sensors());
        }
        view.reconcile_device_selection();
    }
    if view.poll_process_insights()
        | view.poll_service_details()
        | health_ready
        | dynamic_ready
        | live_dynamic_ready
    {
        cx.notify();
    }
}

/// Open the main window and start the telemetry + process polling loops.
pub struct StartupRuntime {
    pub config_client: ConfigClient,
    pub snapshot_export_client: taskmanager_app_host::SnapshotExportClient,
    pub window_capture_client: taskmanager_app_host::WindowCaptureClient,
    pub diagnostic_bundle_client: taskmanager_app_host::DiagnosticBundleClient,
    pub service_log_export_client: taskmanager_app_host::DiagnosticBundleClient,
    pub history_connector: Result<
        taskmanager_app_host::HistoryFrontendConnector,
        taskmanager_app_host::HistoryFrontendConnectorStartError,
    >,
}

pub struct StartupEnvironment {
    pub native_locale_name: Option<String>,
    pub local_time_rules: taskmanager_core::core::time::LocalTimeRulesObservation,
    pub custom_app_id: Option<String>,
    pub presentation: taskmanager_app_host::WindowPresentation,
}

pub fn init<E>(
    cx: &mut App,
    spawn_client: impl FnOnce() -> Result<PlatformClient, E>,
    runtime: StartupRuntime,
    environment: StartupEnvironment,
) -> Result<(), E> {
    let StartupRuntime {
        mut config_client,
        snapshot_export_client,
        window_capture_client,
        diagnostic_bundle_client,
        service_log_export_client,
        history_connector,
    } = runtime;
    let StartupEnvironment {
        native_locale_name,
        local_time_rules,
        custom_app_id,
        presentation,
    } = environment;
    // Single-instance (ADR-032 follow-up): a second launch activates the
    // existing instance's window and exits before any UI is set up. A typed
    // failure degrades gracefully to a normal (possibly duplicated) launch.
    //
    // The compositor capture harness is an explicitly isolated, non-product
    // mode. It must be able to launch a second GPUI process even when the
    // user's desktop already owns the product's D-Bus name; otherwise the
    // harness exits cleanly as `Secondary` before it can produce markers or a
    // window receipt. This branch is reachable only through the opt-in capture
    // environment and does not weaken normal single-instance behavior.
    let capture_mode = std::env::var("TM_CAPTURE_EVIDENCE")
        .ok()
        .is_some_and(|value| !value.is_empty() && value != "0")
        || std::env::var_os("TM_CAPTURE_SCENARIO").is_some();
    let InstanceStartup::Continue {
        guard: instance_guard,
        events: instance_rx,
    } = acquire_instance(capture_mode)
    else {
        cx.quit();
        return Ok(());
    };
    // Own UI-layer bootstrap: focus registry, input/dialog/popup/table/tree
    // keymaps (P6: replaces the old `gpui_component::init`). Idempotent; runs
    // once before the window opens.
    taskmanager_ui::init(cx);
    // Register the bundled fonts (MiSans VF + Roboto Mono) BEFORE any
    // text shapes. The subsequent text-system probe verifies that the family
    // names actually became resolvable; an asset existing in the binary is not
    // treated as proof that toolkit registration succeeded.
    if let Err(font_error) = cx
        .text_system()
        .add_fonts(taskmanager_assets::embedded_fonts())
    {
        error!(%font_error, "embedded font registration failed; falling back to system fonts");
    }
    // Receive the initial immutable config publication from the background
    // coordinator. The bounded fallback remains typed and later ticks can
    // converge through the same publication stream.
    // Applied to the theme BEFORE WindowOptions consume it (the Mica/vibrancy
    // material depends on the skin) and to the frontend refresh interval + RootView
    // toggles/page inside the open_window closure below.
    let (cfg, applied_config_revision, config_bootstrap_notice) = match config_client
        .wait_for_initial(taskmanager_application::DEFAULT_CONFIG_INITIAL_WAIT)
    {
        taskmanager_application::ConfigBootstrap::Published(publication) => {
            let notice = match publication.outcome() {
                taskmanager_application::ConfigPublicationOutcome::Loaded(recovery) => {
                    initial_config_recovery_message(*recovery)
                }
                _ => None,
            };
            (
                publication.snapshot().as_ref().clone(),
                Some(publication.revision()),
                notice,
            )
        }
        taskmanager_application::ConfigBootstrap::Fallback { snapshot, source } => (
            snapshot.as_ref().clone(),
            None,
            Some(i18n::t("settings.config_fallback").replace("{source}", &format!("{source:?}"))),
        ),
    };

    let language_preference = cfg.language.as_deref().and_then(i18n::Language::from_code);
    let initial_language = language_preference.or_else(|| {
        native_locale_name
            .as_deref()
            .map(i18n::language_from_locale)
    });
    i18n::set_language(initial_language.unwrap_or_else(i18n::detect_language));

    // Published GPUI 0.2.2 exposes no text-raster mode API. The startup
    // mapper therefore normalizes legacy subpixel/grayscale config tokens to
    // platform default and the Settings surface marks those modes disabled.

    // Native appearance observation belongs to the platform adapter. It runs
    // before window creation on its own bounded lane, so shared GPUI code never
    // launches gsettings/defaults/reg or reads platform configuration files.
    let initial_interval =
        taskmanager_application::TelemetryInterval::clamped(Duration::from_millis(cfg.refresh_ms));
    let telemetry_refresh_policy =
        taskmanager_application::TelemetryRefreshPolicy::new(initial_interval);
    // Physical retention is a product bound, not a renderer preference. GPUI
    // dashboard windows and device pages project the same canonical rings;
    // shrinking a graph tail must never discard history for another surface.
    let (telemetry, telemetry_ingestor) = TelemetryStore::shared_with_correlated_ingestion(
        taskmanager_telemetry_store::live_graph::MAX_HISTORY_CAPACITY,
    );
    // Continuous history belongs to this frontend process. Enabled persistence
    // starts the paired writer/replay session without blocking the UI thread.
    let mut platform = spawn_client()?;
    platform.set_telemetry_interval(initial_interval);
    let appearance = observe_startup_appearance(&mut platform);
    let mut theme = detect(appearance.value);
    apply_cfg_to_theme(&mut theme, &cfg);
    // Resolve the font stack against this host: system families where the skin
    // names one that is installed, the bundled faces otherwise (and always when
    // the user explicitly chose them). Availability is a one-time snapshot; the
    // RootView re-resolves against it on every skin/font change.
    let font_availability = detect_font_availability(cx);
    if !font_availability.embedded_fonts_ready() {
        warn!(
            bundled_ui = font_availability.bundled_ui_available(),
            bundled_mono = font_availability.bundled_mono_available(),
            catalog_truncated = font_availability.catalog_truncated(),
            "embedded font registration is incomplete; using verified system fallbacks"
        );
    }
    let font_pref = font_pref_from_config(&cfg, &font_availability);
    let skin_preference = skin_preference_from_config(&cfg).or_else(forced_skin_from_env);
    theme = Theme::build(
        theme.skin,
        theme.mode,
        if theme.hc {
            HighContrast::On
        } else {
            HighContrast::Off
        },
        resolve_fonts(font_pref, theme.skin, &font_availability),
    );
    // Linux/Wayland CSD: the compositor (KWin/Niri/…) does NOT round the
    // client-drawn surface, so the window must be transparent and the chrome
    // paints the per-skin corner radius itself (root/titlebar/sidebar/scrim).
    // macOS/Windows keep an opaque surface — their window servers round and
    // shadow natively.
    theme.window_transparent = cfg!(target_os = "linux");
    // gpui-component components (input/table/menu/…) read the global ActiveTheme
    // on their first frame; the RootView renders the window backdrop itself
    // (P4: no gc Root wrapper). Sync the tokens now, before the window opens, so
    // the first frame carries the resolved skin/fonts (this call only pre-warms
    // the color/font tokens; the RootView re-syncs on every skin/font change).
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(product::GPUI_NAME.into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(120.0), px(80.0)),
            size: responsive::initial_window_size(),
        })),
        // Window-frame mode per the persisted preference (`System` when the
        // token is empty/unknown, the historical behavior): ask the compositor
        // for the decoration mode the user picked. KDE/KWin, macOS, and
        // Windows grant Server and draw the native titlebar + corners; the
        // renderer then emits NO app chrome (see `render`). GNOME/Mutter and
        // some tiling WMs force Client (CSD), and the decoration negotiation
        // reflects that — `window_decorations()` read each frame reacts to
        // what was actually granted, so we do NOT sniff XDG_CURRENT_DESKTOP
        // (fragile). `window_transparent` (set below for Linux) keeps the CSD
        // fallback's transparent rounded corners working when a compositor
        // forces Client despite this Server request.
        window_decorations: Some(requested_window_decorations(
            WindowDecorationsPreference::from_config_token(&cfg.window_decorations),
        )),
        // Mica (Windows) / vibrancy (macOS) material when the skin calls for it;
        // GNOME/KDE stay opaque. On Linux the surface is TRANSPARENT so the CSD
        // fallback can paint its own rounded corners (KWin/Niri don't round a
        // CSD surface); when Server is granted the renderer fills the root
        // opaquely and paints no corners, so the transparency is inert there.
        // macOS/Windows keep Opaque and let the window server round natively.
        // `Theme::background_appearance` is the single decision point — the
        // RootView re-applies it on skin/font switches at runtime.
        window_background: background_appearance(&theme),
        // Wayland app_id MUST be the app's reverse-DNS identifier and match the
        // `.desktop` file basename (packaging/linux/io.github...TaskForestG.desktop)
        // so the compositor (KWin/Mutter) can resolve window → desktop entry →
        // Icon= → hicolor theme. gpui 0.2.2 has no window-icon API, so the
        // taskbar/Dock icon comes ENTIRELY from this app_id → .desktop lookup;
        // a bare "taskmanager" app_id matches no installed desktop file and the
        // window falls back to a generic placeholder icon. (X11 compositors use
        // the matching StartupWMClass in the .desktop.) The binary stays
        // `taskmanager`; the config dir (~/.../taskmanager/) is keyed off the
        // binary name, not the app_id, so it is unaffected.
        app_id: Some(resolve_app_id(custom_app_id)),
        // The window-level floor of the three-layer minimum-space doctrine
        // (ADR-039): the compositor may not shrink the surface below the
        // product minimum, so every inner budget (width slots, chart tiers)
        // reasons about a BOUNDED space. Single source: the responsive
        // constants — never a second literal here.
        window_min_size: Some(size(px(responsive::MIN_WIDTH), px(responsive::MIN_HEIGHT))),
        presentation: crate::window_presentation::to_gpui(&presentation),
        ..Default::default()
    };

    let has_explicit_page_override = std::env::var("TM_PAGE").is_ok();
    let root_startup_facts = RootStartupFacts {
        history_connector,
        appearance,
        font_pref,
        skin_preference,
        language_preference,
        font_availability,
        local_time_rules,
    };
    let _ = platform.request_refresh(RefreshRequest::Dashboard, platform_submission_time_ms());
    let surface_role = crate::window_presentation::surface_role(&presentation);
    let window_result = cx.open_window(options, move |window, cx| {
        // gpui 0.2.2 forwards `TitlebarOptions::title` when constructing its
        // Windows and X11 windows, but its Wayland constructor currently drops
        // that initial field. Submit the title through the live platform window
        // as well: on Wayland this reaches `xdg_toplevel::set_title`, while on
        // macOS/Windows it makes the same product identity explicit and is
        // harmlessly idempotent with the creation options above.
        window.set_window_title(product::GPUI_NAME);
        // theme (Copy) carries the cfg-overridden skin/mode/hc; cfg's toggles
        // + last_page are applied to the fresh view right after construction.
        let entity: Entity<RootView> = cx.new(|cx| {
            let mut v = RootView::new_with_platform_and_surface_role(
                theme,
                telemetry,
                telemetry_ingestor,
                telemetry_refresh_policy,
                platform,
                surface_role,
                cx,
            );
            v.snapshot_export.install(snapshot_export_client);
            v.window_capture.install(window_capture_client);
            v.diagnostic_bundle_runtime
                .install(diagnostic_bundle_client);
            v.service_details
                .install_export_client(service_log_export_client);
            apply_root_startup_config(
                &mut v,
                &cfg,
                root_startup_facts,
                has_explicit_page_override,
                cx,
            );
            v
        });
        let weak = entity.downgrade();
        // Single-instance (ADR-032 follow-up): hold the primary guard and
        // pump activation requests via non-blocking try_recv in the tick loop.
        entity.update(cx, |view, _cx| {
            if let Some(guard) = instance_guard {
                view.instance_guard = Some(guard);
            }
            view.instance_rx = Some(instance_rx);
        });
        if let Some(notice) = config_bootstrap_notice {
            entity.update(cx, |view, cx| view.show_local_feedback(notice, cx));
        }
        // System tray (ADR-032) belongs only to the product session. Capture
        // mode is a disposable, private-compositor process: it must not probe
        // or register a StatusNotifierItem on either the host bus or a private
        // watcher, and it must be able to close normally for owned cleanup.
        if !capture_mode {
            let tray_available = super::tray::spawn_tray_host(&entity, cx);
            // A native titlebar close must not destroy the only window while
            // the tray owns the process lifetime. Returning false keeps the
            // GPUI root entity, ECS/runtime workers, single-instance guard,
            // and tray alive; minimizing is the portable hide primitive.
            if tray_available {
                window.on_window_should_close(cx, |window, _cx| {
                    window.minimize_window();
                    false
                });
            }
        }
        let window_handle = window.window_handle();
        spawn_update_loop(
            cx,
            weak,
            window_handle,
            config_client,
            applied_config_revision,
        );
        // P4 consumption switch: the window root is our own RootView directly — no
        // gpui_component::Root wrapper. RootView hosts the LayerStack overlay stack
        // itself (overlays render as child views of the host; see root.rs).
        entity
    });
    if let Err(error) = window_result {
        error!(%error, "main GPUI window failed to open");
        cx.quit();
    }
    cx.activate(true);
    Ok(())
}
