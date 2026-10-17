//! Application startup and persisted shell configuration.

use super::graph_options::normalize_graph_data_points;
use super::{
    ConfigClient, Duration, PlatformClient, RefreshRequest, RootView, TelemetryStore, Theme,
    TopPage, apply_process_config, config_from_view, i18n, platform_submission_time_ms,
};
use crate::gpui_app::theme::{detect, forced_skin_from_env};
use gpui::{AnyWindowHandle, App, AsyncApp, SharedString, WeakEntity, WindowDecorations};
use taskmanager_app_host::DiagnosticBundleClient;
use taskmanager_app_host::HistoryFrontendConnector;
use taskmanager_app_host::HistoryFrontendConnectorStartError;
use taskmanager_app_host::NativeAppHost;
use taskmanager_app_host::SnapshotExportClient;
use taskmanager_app_host::WindowCaptureClient;
use taskmanager_app_host::WindowPresentation;
use taskmanager_app_host::acquire_single_instance;
use taskmanager_application::ConfigBootstrap;
use taskmanager_application::ConfigPublicationOutcome;
use taskmanager_application::ConfigRevision;
use taskmanager_application::DEFAULT_CONFIG_INITIAL_WAIT;
use taskmanager_application::TelemetryInterval;
use taskmanager_application::TelemetryRefreshPolicy;
use taskmanager_assets::product;
use taskmanager_core::core::config::Config;
use taskmanager_core::core::time::LocalTimeRulesObservation;
use taskmanager_ipc::SubscriptionMask;
use taskmanager_platform_contract::InstanceEvent;
use taskmanager_platform_contract::InstanceGuard;
use taskmanager_platform_contract::InstanceRole;
use taskmanager_shell::FeedbackLifecycle;
use taskmanager_shell::FeedbackSeverity;
use taskmanager_shell::FeedbackSource;
use taskmanager_shell::SortCol;
use taskmanager_shell::SortDir;
use taskmanager_telemetry_store::HistoryRetention;
use taskmanager_theme::FontAvailability;
use taskmanager_theme::FontPreference;
use taskmanager_theme::Skin;
use taskmanager_theme::{HighContrast, resolve_fonts};
use taskmanager_ui::theme_binding::detect_font_availability;
use tracing::warn;

mod appearance;
use appearance::observe_startup_appearance;

mod capture_systems;
mod demo;
pub(crate) use demo::init_demo;

mod config_tokens;
use crate::gpui_app::chrome::WindowDecorationsPreference;
pub(super) use config_tokens::{
    FONT_TOKEN_SYSTEM, color_scheme_from_token, font_pref_from_config, page_token,
    skin_preference_from_config, text_rendering_mode_from_token,
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

mod window;
use window::{RootWindowInputs, make_root_builder, open_main_window, prepare_window_options};

mod ui_bootstrap;
use ui_bootstrap::bootstrap_owned_ui;

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
        guard: Option<Box<dyn InstanceGuard>>,
        events: std::sync::mpsc::Receiver<InstanceEvent>,
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
    match acquire_single_instance(product::GPUI_NAME, events_tx) {
        Ok(InstanceRole::Primary(guard)) => InstanceStartup::Continue {
            guard: Some(guard),
            events,
        },
        Ok(InstanceRole::Secondary) => InstanceStartup::Secondary,
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
    history_connector: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
    appearance: appearance::StartupAppearanceObservation,
    font_pref: FontPreference,
    skin_preference: Option<Skin>,
    language_preference: Option<i18n::Language>,
    font_availability: FontAvailability,
    local_time_rules: LocalTimeRulesObservation,
}

struct InitialConfig {
    config: Config,
    revision: Option<ConfigRevision>,
    notice: Option<String>,
}

type StartupInstance = (
    Option<Box<dyn InstanceGuard>>,
    std::sync::mpsc::Receiver<InstanceEvent>,
);

fn read_initial_config(config_client: &mut ConfigClient) -> InitialConfig {
    match config_client.wait_for_initial(DEFAULT_CONFIG_INITIAL_WAIT) {
        ConfigBootstrap::Published(publication) => {
            let notice = match publication.outcome() {
                ConfigPublicationOutcome::Loaded(recovery) => {
                    initial_config_recovery_message(*recovery)
                }
                _ => None,
            };
            InitialConfig {
                config: publication.snapshot().as_ref().clone(),
                revision: Some(publication.revision()),
                notice,
            }
        }
        ConfigBootstrap::Fallback { snapshot, source } => InitialConfig {
            config: snapshot.as_ref().clone(),
            revision: None,
            notice: Some(
                i18n::t("settings.config_fallback").replace("{source}", &format!("{source:?}")),
            ),
        },
    }
}

fn capture_mode_enabled() -> bool {
    std::env::var("TM_CAPTURE_EVIDENCE")
        .ok()
        .is_some_and(|value| !value.is_empty() && value != "0")
        || std::env::var_os("TM_CAPTURE_SCENARIO").is_some()
}

fn continue_startup(cx: &mut App, capture_mode: bool) -> Option<StartupInstance> {
    match acquire_instance(capture_mode) {
        InstanceStartup::Continue { guard, events } => Some((guard, events)),
        InstanceStartup::Secondary => {
            cx.quit();
            None
        }
    }
}

fn report_font_fallback(availability: &FontAvailability) {
    if !availability.embedded_fonts_ready() {
        warn!(
            bundled_ui = availability.bundled_ui_available(),
            bundled_mono = availability.bundled_mono_available(),
            catalog_truncated = availability.catalog_truncated(),
            "embedded font registration is incomplete; using verified system fallbacks"
        );
    }
}

fn apply_root_startup_config(
    view: &mut RootView,
    cfg: &Config,
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
            FeedbackSource::Persistence,
            FeedbackSeverity::Warning,
            FeedbackLifecycle::UntilReplaced,
            i18n::t("perf.replay.startup_failure_notice").replace("{kind}", reason.stable_code()),
        );
    }
    view.desktop_appearance = facts.appearance.value;
    view.desktop_appearance_sources = facts.appearance.sources;
    view.platform_failures = facts.appearance.failures;
    view.font_availability = facts.font_availability;
    view.local_time_rules = facts.local_time_rules;
    apply_root_persisted_projection(view, cfg, cx);
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
        view.set_process_sort(SortCol::Name, SortDir::Asc);
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
    mut applied_config_revision: Option<ConfigRevision>,
) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        let mut tick = 0u32;
        let mut submitted_presentation = None;
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(200))
                .await;
            tick = tick.wrapping_add(1);
            if weak.upgrade().is_none() {
                cx.update(|app| app.quit());
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
                if view.drain_daemon_telemetry() {
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
    let dynamic_history_ready = dynamic_ready
        && view.capture_evidence.seed_dynamic_capture_history(
            &view.telemetry.system_history,
            &view.telemetry_ingestor,
            view.system_snapshot().timestamp_ms,
        );
    if dynamic_history_ready {
        view.select_device(super::SelectedDevice::Battery(0));
    }
    if view.poll_process_insights()
        | view.poll_service_details()
        | health_ready
        | dynamic_ready
        | dynamic_history_ready
    {
        cx.notify();
    }
}

/// Open the main window and start the telemetry + process polling loops.
pub struct StartupRuntime {
    pub config_client: ConfigClient,
    pub snapshot_export_client: SnapshotExportClient,
    pub window_capture_client: WindowCaptureClient,
    pub diagnostic_bundle_client: DiagnosticBundleClient,
    pub service_log_export_client: DiagnosticBundleClient,
    pub history_connector: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
}

pub struct StartupEnvironment {
    pub native_locale_name: Option<String>,
    pub local_time_rules: LocalTimeRulesObservation,
    pub custom_app_id: Option<String>,
    pub presentation: WindowPresentation,
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
    let capture_mode = capture_mode_enabled();
    let Some((instance_guard, instance_rx)) = continue_startup(cx, capture_mode) else {
        return Ok(());
    };
    // Own UI-layer bootstrap: focus registry, input/dialog/popup/table/tree
    // keymaps (P6: replaces the old `gpui_component::init`). Idempotent; runs
    // once before the window opens.
    bootstrap_owned_ui(cx);
    // Register the bundled fonts (MiSans VF + Roboto Mono) BEFORE any
    // text shapes. The subsequent text-system probe verifies that the family
    // names actually became resolvable; an asset existing in the binary is not
    // treated as proof that toolkit registration succeeded.
    // Receive the initial immutable config publication from the background
    // coordinator. The bounded fallback remains typed and later ticks can
    // converge through the same publication stream.
    // Applied to the theme BEFORE WindowOptions consume it (the Mica/vibrancy
    // material depends on the skin) and to the frontend refresh interval + RootView
    // toggles/page inside the open_window closure below.
    let InitialConfig {
        config: cfg,
        revision: applied_config_revision,
        notice: config_bootstrap_notice,
    } = read_initial_config(&mut config_client);

    let language_preference = cfg.language.as_deref().and_then(i18n::Language::from_code);
    let initial_language = language_preference.or_else(|| {
        native_locale_name
            .as_deref()
            .map(i18n::language_from_locale)
    });
    i18n::set_language(initial_language.unwrap_or_else(i18n::detect_language));

    // Native appearance observation belongs to the platform adapter. It runs
    // before window creation on its own bounded lane, so shared GPUI code never
    // launches gsettings/defaults/reg or reads platform configuration
    // files.
    let initial_interval = TelemetryInterval::clamped(Duration::from_millis(cfg.refresh_ms));
    let telemetry_refresh_policy = TelemetryRefreshPolicy::new(initial_interval);
    // Physical retention is a product bound, not a renderer preference. GPUI
    // dashboard windows and device pages project the same canonical rings;
    // shrinking a graph tail must never discard history for another surface.
    let (telemetry, telemetry_ingestor) =
        TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::PRODUCT);
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
    report_font_fallback(&font_availability);
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
    let app_id = resolve_app_id(custom_app_id);
    let Some((options, fallback_options)) =
        prepare_window_options(cx, &cfg, &theme, &app_id, &presentation)
    else {
        return Ok(());
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
    let (daemon_worker, daemon_rx) =
        if !capture_mode && NativeAppHost::production().is_daemon_connected() {
            match NativeAppHost::production().subscribe_daemon_stream(SubscriptionMask::ALL, 1000) {
                Some((w, r)) => (Some(w), Some(r)),
                None => (None, None),
            }
        } else {
            (None, None)
        };
    let root_builder = make_root_builder(RootWindowInputs {
        theme,
        telemetry,
        telemetry_ingestor,
        telemetry_refresh_policy,
        platform,
        cfg,
        root_startup_facts,
        has_explicit_page_override,
        snapshot_export_client,
        window_capture_client,
        diagnostic_bundle_client,
        service_log_export_client,
        instance_guard,
        instance_rx,
        config_bootstrap_notice,
        config_client,
        applied_config_revision,
        capture_mode,
        surface_role,
        daemon_worker,
        daemon_rx,
    });
    open_main_window(
        cx,
        options,
        fallback_options,
        &presentation,
        theme,
        root_builder,
    );
    Ok(())
}
