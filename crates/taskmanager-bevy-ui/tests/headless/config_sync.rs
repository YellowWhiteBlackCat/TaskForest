//! test-intent: behavior
//!
//! Headless behavior tests verifying that Bevy UI settings edits round-trip through
//! [`taskmanager_application::ConfigClient`] and persist across sessions via
//! [`taskmanager_application::ConfigCoordinator`].

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::Assets;
use bevy::camera::ClearColor;
use bevy::ecs::entity::Entity;
use bevy::text::Font;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::{
    ConfigBootstrap, ConfigClient, ConfigCoordinator, ConfigStore, HostTelemetryRequest,
    PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle, SystemFacets, TelemetryInterval,
};
use taskmanager_core::config::Config;
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, RequestPort, SubmissionError,
};
use taskmanager_theme::{LightDark, Skin, Theme};

use super::{
    SettingsChoice, SettingsField, ThemePreferences, apply_preferences, patch_persisted_config,
};
use crate::app::{FrontendTrack, Page, PageContent, Route, SharedRuntimeHandle};
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use crate::window::tests::HeadlessFrontendPlugins;
use crate::window::{FrontendWindowPlugin, WindowPalette};
use taskmanager_application::DEFAULT_CONFIG_INITIAL_WAIT;
use taskmanager_platform_contract::RequestEnvelope;

/// Apply a persisted [`Config`] snapshot to the live theme preferences and
/// shell. The windowed composition does not yet restore persisted config at
/// startup, so this lives with the config-sync tests that exercise it.
fn apply_persisted_config(
    config: &Config,
    prefs: Option<&mut ThemePreferences>,
    palette: &mut WindowPalette,
    clear: Option<&mut ClearColor>,
    track: &mut FrontendTrack,
) {
    if let Some(prefs) = prefs {
        if config.mode.eq_ignore_ascii_case("System") || config.mode.is_empty() {
            prefs.mode = None;
        } else if config.mode.eq_ignore_ascii_case("Light") {
            prefs.mode = Some(LightDark::Light);
        } else if config.mode.eq_ignore_ascii_case("Dark") {
            prefs.mode = Some(LightDark::Dark);
        } else if config.mode.eq_ignore_ascii_case("EyeForest") {
            prefs.mode = Some(LightDark::EyeForest);
        }

        if !config.skin.is_empty() {
            prefs.skin = match config.skin.to_ascii_lowercase().as_str() {
                "kde" => Some(Skin::Kde),
                "windows" => Some(Skin::Windows),
                "macos" => Some(Skin::Macos),
                _ => Some(Skin::Gnome),
            };
        }

        prefs.hc = config.hc;
        apply_preferences(prefs, palette, clear);
    }

    if let Some(lang) = config.language.as_deref().and_then(Language::from_code) {
        set_language(lang);
    }

    if config.refresh_ms > 0 {
        track
            .shell
            .set_telemetry_interval(TelemetryInterval::clamped(Duration::from_millis(
                config.refresh_ms,
            )));
    }

    if config.graph_data_points > 0 {
        track
            .shell
            .set_history_capacity(usize::try_from(config.graph_data_points).unwrap_or(60));
    }
}

/// Sync preferences from the coordinator via [`SharedRuntimeHandle`].
fn sync_preferences_from_config(
    runtime: Option<&SharedRuntimeHandle>,
    prefs: Option<&mut ThemePreferences>,
    palette: &mut WindowPalette,
    clear: Option<&mut ClearColor>,
    track: &mut FrontendTrack,
) {
    let Some(runtime) = runtime else {
        return;
    };
    let mut config_guard = runtime.shared.lock_config();
    let Some(client) = config_guard.as_mut() else {
        return;
    };

    if client.snapshot().is_none() {
        let _ = client.wait_for_initial(DEFAULT_CONFIG_INITIAL_WAIT);
    } else {
        let _ = client.drain();
    }

    if let Some(snapshot) = client.snapshot().cloned() {
        apply_persisted_config(&snapshot, prefs, palette, clear, track);
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

struct QuietRequests;

impl RequestPort for QuietRequests {
    type Request = HostTelemetryRequest;

    fn try_submit(&self, _request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        Ok(())
    }
}

fn fake_client() -> PlatformClient {
    let snapshot = CapabilitySnapshot::from_descriptors([CapabilityDescriptor {
        id: CapabilityId::TELEMETRY_HOST,
        status: CapabilityStatus::Available,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }]);
    PlatformClient::new(PlatformHandle::new(
        std::sync::Arc::new(FixedCapabilities(snapshot)),
        std::sync::Arc::new(QuietEvents),
        PlatformFacets::default()
            .with_system(SystemFacets::default().with_host(std::sync::Arc::new(QuietRequests))),
    ))
}

fn test_path(label: &str) -> PathBuf {
    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);
    let sequence = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp/bevy-config-sync")
        .join(format!("{label}-{}-{sequence}", std::process::id()))
        .join("config.json")
}

fn start_coordinator(path: &PathBuf) -> (ConfigCoordinator, ConfigClient) {
    let coordinator = ConfigCoordinator::start(ConfigStore::new(path)).expect("start runtime");
    let mut client = coordinator.client();
    assert!(matches!(
        client.wait_for_initial(Duration::from_secs(2)),
        ConfigBootstrap::Published(_)
    ));
    (coordinator, client)
}

fn scripted_runtime_with_config(config_client: ConfigClient) -> &'static SharedRuntime {
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    cache
        .get_or_init_with_config(move || Ok((fake_client(), Some(config_client))))
        .expect("scripted runtime with config starts")
}

fn headless_settings_app(runtime: &'static SharedRuntime) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime,
        palette: ui_palette(&Theme::dark()),
    });
    app.init_resource::<Assets<Font>>();
    app.world_mut().resource_mut::<Route>().page = Page::Settings;
    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<&PageContent, ()>()
            .single(app.world())
            .expect("exactly one page content mounts")
            .page,
        Page::Settings,
        "the settings page is mounted"
    );
    app
}

fn choice_entity(world: &mut bevy::ecs::world::World, wanted: &SettingsField) -> Entity {
    let mut choices = world.query_filtered::<(Entity, &SettingsChoice), ()>();
    choices
        .iter(world)
        .find(|(_, choice)| &choice.0 == wanted)
        .map(|(entity, _)| entity)
        .unwrap_or_else(|| panic!("the {wanted:?} choice must be mounted"))
}

fn activate(app: &mut App, entity: Entity) {
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::ValueChange::<bool> {
            source: entity,
            value: true,
            is_final: true,
        });
    app.update();
}

fn wait_for_config_sync(runtime: &'static SharedRuntime) {
    let guard = runtime.lock_config();
    if let Some(client) = guard.as_ref() {
        client
            .synchronize(Duration::from_secs(2))
            .expect("config client sync within 2s");
    }
}

#[test]
fn theme_mode_edits_round_trip_and_persist() {
    let path = test_path("theme-mode");
    let (coordinator, client) = start_coordinator(&path);
    let runtime = scripted_runtime_with_config(client);
    let mut app = headless_settings_app(runtime);

    // 1. Activate Light mode
    let light_entity = choice_entity(app.world_mut(), &SettingsField::Theme(LightDark::Light));
    activate(&mut app, light_entity);
    wait_for_config_sync(runtime);

    let store = ConfigStore::new(&path);
    assert_eq!(store.load_or_default().mode, "Light");

    // 2. Activate Dark mode
    let dark_entity = choice_entity(app.world_mut(), &SettingsField::Theme(LightDark::Dark));
    activate(&mut app, dark_entity);
    wait_for_config_sync(runtime);
    assert_eq!(store.load_or_default().mode, "Dark");

    // 3. Activate System mode
    let system_entity = choice_entity(app.world_mut(), &SettingsField::SystemMode);
    activate(&mut app, system_entity);
    wait_for_config_sync(runtime);
    assert_eq!(store.load_or_default().mode, "System");

    drop(coordinator);
    let _ = std::fs::remove_dir_all(path.parent().expect("config parent"));
}

#[test]
fn high_contrast_and_language_edits_round_trip_and_persist() {
    let path = test_path("hc-lang");
    let (coordinator, client) = start_coordinator(&path);
    let runtime = scripted_runtime_with_config(client);
    let mut app = headless_settings_app(runtime);

    // 1. Activate High Contrast
    let hc_entity = choice_entity(app.world_mut(), &SettingsField::HighContrast(true));
    activate(&mut app, hc_entity);
    wait_for_config_sync(runtime);

    let store = ConfigStore::new(&path);
    assert!(
        store.load_or_default().hc,
        "high contrast should be enabled in config"
    );

    // 2. Activate Language Zh
    let zh_entity = choice_entity(app.world_mut(), &SettingsField::Language(Language::Zh));
    activate(&mut app, zh_entity);
    wait_for_config_sync(runtime);
    assert_eq!(store.load_or_default().language.as_deref(), Some("zh"));

    // 3. Revert High Contrast
    let hc_off = choice_entity(app.world_mut(), &SettingsField::HighContrast(false));
    activate(&mut app, hc_off);
    wait_for_config_sync(runtime);
    assert!(
        !store.load_or_default().hc,
        "high contrast should be disabled in config"
    );

    // Clean up language
    set_language(Language::En);
    drop(coordinator);
    let _ = std::fs::remove_dir_all(path.parent().expect("config parent"));
}

#[test]
fn telemetry_cadence_and_history_capacity_persist() {
    let path = test_path("cadence-capacity");
    let (coordinator, client) = start_coordinator(&path);
    let runtime = scripted_runtime_with_config(client);
    let mut app = headless_settings_app(runtime);

    // 1. Set refresh interval to 2000 ms
    let two_sec = choice_entity(
        app.world_mut(),
        &SettingsField::Refresh(TelemetryInterval::clamped(Duration::from_millis(2000))),
    );
    activate(&mut app, two_sec);
    wait_for_config_sync(runtime);

    let store = ConfigStore::new(&path);
    assert_eq!(store.load_or_default().refresh_ms, 2000);

    // 2. Set capacity to 300
    let cap_300 = choice_entity(app.world_mut(), &SettingsField::HistoryCapacity(300));
    activate(&mut app, cap_300);
    wait_for_config_sync(runtime);
    assert_eq!(store.load_or_default().graph_data_points, 300);

    drop(coordinator);
    let _ = std::fs::remove_dir_all(path.parent().expect("config parent"));
}

#[test]
fn skin_patch_persists_to_coordinator() {
    let path = test_path("skin");
    let (coordinator, client) = start_coordinator(&path);
    let runtime = scripted_runtime_with_config(client);

    let handle = SharedRuntimeHandle { shared: runtime };
    let submission = patch_persisted_config(Some(&handle), |cfg| {
        cfg.skin = Skin::Kde.label().to_string();
    });
    assert!(submission.is_some(), "submission should be queued");
    wait_for_config_sync(runtime);

    let store = ConfigStore::new(&path);
    assert_eq!(store.load_or_default().skin, "KDE");

    drop(coordinator);
    let _ = std::fs::remove_dir_all(path.parent().expect("config parent"));
}

#[test]
fn cross_session_configuration_persistence_round_trip() {
    let path = test_path("cross-session");

    // Session 1: write preferences through Bevy settings app
    {
        let (coordinator, client) = start_coordinator(&path);
        let runtime = scripted_runtime_with_config(client);
        let mut app = headless_settings_app(runtime);

        // Modify theme to Light
        let light_entity = choice_entity(app.world_mut(), &SettingsField::Theme(LightDark::Light));
        activate(&mut app, light_entity);

        // Modify high contrast to true
        let hc_entity = choice_entity(app.world_mut(), &SettingsField::HighContrast(true));
        activate(&mut app, hc_entity);

        // Modify refresh interval to 500ms
        let half_sec = choice_entity(
            app.world_mut(),
            &SettingsField::Refresh(TelemetryInterval::clamped(Duration::from_millis(500))),
        );
        activate(&mut app, half_sec);

        // Modify capacity to 120
        let cap_120 = choice_entity(app.world_mut(), &SettingsField::HistoryCapacity(120));
        activate(&mut app, cap_120);

        wait_for_config_sync(runtime);
        drop(app);
        drop(coordinator);
    }

    // Session 2: open new coordinator and client on the same storage path
    {
        let (coordinator, client) = start_coordinator(&path);
        let runtime = scripted_runtime_with_config(client);

        let mut app = headless_settings_app(runtime);

        // Synchronize preferences from the persisted snapshot
        use bevy::ecs::system::{NonSendMut, Res, ResMut, RunSystemOnce};
        app.world_mut()
            .run_system_once(
                |runtime: Res<SharedRuntimeHandle>,
                 mut track: NonSendMut<FrontendTrack>,
                 mut palette: ResMut<WindowPalette>,
                 mut prefs: Option<ResMut<ThemePreferences>>| {
                    sync_preferences_from_config(
                        Some(&runtime),
                        prefs.as_deref_mut(),
                        &mut palette,
                        None,
                        &mut track,
                    );
                },
            )
            .expect("system runs once");

        let prefs_val = app.world().resource::<ThemePreferences>();
        assert_eq!(prefs_val.mode, Some(LightDark::Light));
        assert!(prefs_val.hc);

        let track_val = app.world().non_send::<FrontendTrack>();
        assert_eq!(
            track_val.shell.telemetry_interval(),
            TelemetryInterval::clamped(Duration::from_millis(500))
        );
        assert_eq!(track_val.shell.history.capacity(), 120);

        drop(coordinator);
    }

    let _ = std::fs::remove_dir_all(path.parent().expect("config parent"));
}
