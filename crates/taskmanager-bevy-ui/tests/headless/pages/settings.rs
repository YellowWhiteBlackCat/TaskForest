//! test-intent: behavior
//!
//! Settings-page tests:
//!
//! - the choice ladders project the live authorities exactly (never the
//!   nearest step), and writes round-trip through the shell's public
//!   entries — interval, clamped capacity — so a mutation in the projection
//!   or the write path fails here without a world;
//! - the two switchable theme modes resolve and read back from the palette
//!   authority, and a foreign mode combination is never claimed as one of
//!   them;
//! - the wired activation observer on the real plugin composition: a widget
//!   activation writes its authority (shell preferences, the palette
//!   resources, the shared i18n bundle) and remounts the page, whose fresh
//!   tree mirrors the new values — "changes take effect immediately, no
//!   local copy state".

use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::Assets;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::{Has, With};
use bevy::ecs::world::World;
use bevy::text::Font;
use bevy::ui::Checked;
use bevy::ui::widget::Text;
use taskmanager_application::i18n::{Language, current_language, set_language};
use taskmanager_application::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, HostTelemetryRequest, PlatformClient, PlatformEvent,
    PlatformFacets, PlatformHandle, RequestPort, SubmissionError, SystemFacets, TelemetryInterval,
};
use taskmanager_shell::ShellApp;
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

use super::{
    CAPACITY_CHOICES, REFRESH_CHOICES_MS, SettingsChoice, SettingsField, capacity_choice_index,
    palette_mode, refresh_choice_index, theme_for_mode,
};
use crate::app::{FrontendTrack, Page, PageContent, Route};
use crate::palette::ui_palette;
use crate::window::tests::HeadlessFrontendPlugins;
use crate::window::{FrontendWindowPlugin, WindowPalette};

// ---- scripted platform client (the headless shell-app composition) ----

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

    fn try_submit(
        &self,
        _request: taskmanager_application::RequestEnvelope<Self::Request>,
    ) -> Result<(), SubmissionError> {
        Ok(())
    }
}

fn scripted_runtime() -> &'static crate::runtime::SharedRuntime {
    let snapshot = CapabilitySnapshot::from_descriptors([CapabilityDescriptor {
        id: CapabilityId::TELEMETRY_HOST,
        status: CapabilityStatus::Available,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }]);
    let client = PlatformClient::new(PlatformHandle::new(
        std::sync::Arc::new(FixedCapabilities(snapshot)),
        std::sync::Arc::new(QuietEvents),
        PlatformFacets::default()
            .with_system(SystemFacets::default().with_host(std::sync::Arc::new(QuietRequests))),
    ));
    let cache: &'static crate::runtime::RuntimeCache =
        Box::leak(Box::new(crate::runtime::RuntimeCache::new()));
    cache
        .get_or_init(move || Ok(client))
        .expect("the scripted runtime always starts")
}

fn headless_shell_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime: scripted_runtime(),
        palette: ui_palette(&Theme::dark()),
    });
    app.init_resource::<Assets<Font>>();
    app
}

fn mount_settings(app: &mut App) {
    // Set the route BEFORE the first update so the page mounts directly —
    // no intermediate page ever mounts in this fixture.
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
}

fn mounted_page_entity(world: &mut World) -> Entity {
    world
        .query_filtered::<Entity, With<PageContent>>()
        .single(world)
        .expect("exactly one page content mounts")
}

fn choice_entity(world: &mut World, wanted: &SettingsField) -> Entity {
    let mut choices = world.query_filtered::<(Entity, &SettingsChoice), ()>();
    choices
        .iter(world)
        .find(|(_, choice)| &choice.0 == wanted)
        .map(|(entity, _)| entity)
        .unwrap_or_else(|| panic!("the {wanted:?} choice must be mounted"))
}

fn is_checked(world: &mut World, entity: Entity) -> bool {
    let mut checked = world.query_filtered::<Has<Checked>, ()>();
    checked.get(world, entity).unwrap_or(false)
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

// ---- pure projections of the authorities ----

#[test]
fn refresh_choices_project_the_live_interval_exactly() {
    let shell = ShellApp::new();
    assert_eq!(
        refresh_choice_index(shell.telemetry_interval()),
        Some(1),
        "the default cadence (1 s) is one of the offered steps"
    );
    // A cadence between steps is never reported as a nearby step.
    let between = TelemetryInterval::clamped(Duration::from_millis(750));
    assert_eq!(
        refresh_choice_index(between),
        None,
        "no fabricated selection for a cadence the ladder does not offer"
    );
}

#[test]
fn refresh_and_capacity_writes_round_trip_through_the_shell_entries() {
    let mut shell = ShellApp::new();
    let two_seconds = TelemetryInterval::clamped(Duration::from_millis(REFRESH_CHOICES_MS[2]));
    shell.set_telemetry_interval(two_seconds);
    assert_eq!(shell.telemetry_interval(), two_seconds);
    assert_eq!(
        refresh_choice_index(shell.telemetry_interval()),
        Some(2),
        "the written cadence projects back onto its step"
    );

    let capacity = CAPACITY_CHOICES[2];
    shell.set_history_capacity(capacity);
    assert_eq!(shell.history.capacity(), capacity);
    assert_eq!(capacity_choice_index(shell.history.capacity()), Some(2));

    // The store clamps; the projection follows the effective value.
    shell.set_history_capacity(99_999);
    assert_eq!(
        shell.history.capacity(),
        600,
        "the shell entry clamps beyond-range writes"
    );
    assert_eq!(capacity_choice_index(shell.history.capacity()), Some(3));
    assert_eq!(
        capacity_choice_index(64),
        None,
        "the store default (64) is honestly not one of the offered steps"
    );
}

#[test]
fn theme_modes_resolve_pairwise_and_read_back_from_the_palette() {
    let light = ui_palette(&theme_for_mode(LightDark::Light));
    let dark = ui_palette(&theme_for_mode(LightDark::Dark));
    assert_ne!(
        light.content_bg.to_srgba(),
        dark.content_bg.to_srgba(),
        "the two switchable modes must be distinguishable on the view surface"
    );
    assert_eq!(palette_mode(&light), Some(LightDark::Light));
    assert_eq!(palette_mode(&dark), Some(LightDark::Dark));
    assert_eq!(
        palette_mode(&ui_palette(&Theme::dark())),
        Some(LightDark::Dark),
        "the cold-start palette reads back as Dark by construction"
    );
    // A mode combination the switch does not own is never claimed.
    let eye_forest = ui_palette(&Theme::build(
        Skin::Gnome,
        LightDark::EyeForest,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    ));
    assert_eq!(
        palette_mode(&eye_forest),
        None,
        "an unresolvable palette renders no fabricated selection"
    );
}

// ---- wired activation chain ----

#[test]
fn settings_page_projects_the_live_authorities_into_rows() {
    set_language(Language::En); // deterministic baseline for the row census
    let mut app = headless_shell_app();
    mount_settings(&mut app);
    let world = app.world_mut();
    let texts = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect::<Vec<String>>();
    for expected in [
        "Settings",
        "Theme",
        "Language",
        "Refresh interval",
        "History capacity",
        "Telemetry updates",
    ] {
        assert!(
            texts.iter().any(|text| text == expected),
            "the {expected} row renders: {texts:?}"
        );
    }
    assert!(
        texts.iter().any(|text| text == "1000 ms"),
        "the live cadence value renders: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "64 samples"),
        "the live capacity value renders: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text.contains("in incubation") && text.contains("Settings")),
        "the remaining-fields placeholder is honest about its state"
    );
    // The default selections mirror the authorities: 1 s cadence checked,
    // the other steps unchecked; the store default capacity (64) selects no
    // step at all.
    let one_second = choice_entity(
        world,
        &SettingsField::Refresh(TelemetryInterval::clamped(Duration::from_millis(1000))),
    );
    assert!(
        is_checked(world, one_second),
        "the live cadence is selected"
    );
    let half_second = choice_entity(
        world,
        &SettingsField::Refresh(TelemetryInterval::clamped(Duration::from_millis(500))),
    );
    assert!(
        !is_checked(world, half_second),
        "an unselected step is clear"
    );
    for capacity in CAPACITY_CHOICES {
        let entity = choice_entity(world, &SettingsField::HistoryCapacity(capacity));
        assert!(
            !is_checked(world, entity),
            "capacity {capacity} must not claim the unoffered default"
        );
    }
    let pause = choice_entity(world, &SettingsField::PauseTelemetry);
    assert!(!is_checked(world, pause), "telemetry starts live");
}

#[test]
fn shell_preference_activations_write_the_shell_and_remount_fresh_rows() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);
    let before = mounted_page_entity(app.world_mut());

    let two_seconds =
        SettingsField::Refresh(TelemetryInterval::clamped(Duration::from_millis(2000)));
    let two_second_radio = choice_entity(app.world_mut(), &two_seconds);
    activate(&mut app, two_second_radio);
    assert_eq!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .telemetry_interval(),
        TelemetryInterval::clamped(Duration::from_millis(2000)),
        "the cadence activation wrote the shell entry"
    );
    assert_ne!(
        mounted_page_entity(app.world_mut()),
        before,
        "the page remounted with the fresh projection"
    );

    let capacity_radio = choice_entity(app.world_mut(), &SettingsField::HistoryCapacity(300));
    activate(&mut app, capacity_radio);
    assert_eq!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .history
            .capacity(),
        300,
        "the capacity activation wrote the (clamping) shell entry"
    );

    // The pause toggle goes through the shared reducer, guarded against
    // double flips by the activation observer.
    let pause = choice_entity(app.world_mut(), &SettingsField::PauseTelemetry);
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::ValueChange::<bool> {
            source: pause,
            value: true,
            is_final: true,
        });
    app.update();
    assert!(
        app.world().non_send::<FrontendTrack>().shell.paused(),
        "the pause activation ran the shared TogglePause reducer"
    );
    // The remounted tree mirrors the new values: the 2 s radio is checked.
    let two_second_radio = choice_entity(app.world_mut(), &two_seconds);
    assert!(
        is_checked(app.world_mut(), two_second_radio),
        "the fresh tree mirrors the applied cadence"
    );
}

#[test]
fn theme_activation_swaps_the_palette_authority_and_restyles_the_rail() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    let light_choice = choice_entity(app.world_mut(), &SettingsField::Theme(LightDark::Light));
    activate(&mut app, light_choice);
    let light = ui_palette(&theme_for_mode(LightDark::Light));
    assert_eq!(
        app.world()
            .resource::<WindowPalette>()
            .inner
            .content_bg
            .to_srgba(),
        light.content_bg.to_srgba(),
        "the activation re-resolved the palette resource"
    );
    // The route observer restyles the nav rail from the same resource on
    // every remount the activation requested.
    let rail = app
        .world_mut()
        .query_filtered::<(&crate::app::NavTarget, &bevy::ui::BackgroundColor), ()>()
        .iter(app.world())
        .map(|(target, fill)| (target.0, fill.0.to_srgba()))
        .collect::<Vec<_>>();
    assert_eq!(rail.len(), Page::ALL.len());
    for (page, fill) in rail {
        let expected = crate::app::nav_item_background(page == Page::Settings, &light).to_srgba();
        assert_eq!(fill, expected, "the rail follows the swapped palette");
    }

    // Switching back re-resolves the dark pair.
    let dark_choice = choice_entity(app.world_mut(), &SettingsField::Theme(LightDark::Dark));
    activate(&mut app, dark_choice);
    let dark = ui_palette(&theme_for_mode(LightDark::Dark));
    assert_eq!(
        app.world()
            .resource::<WindowPalette>()
            .inner
            .content_bg
            .to_srgba(),
        dark.content_bg.to_srgba()
    );
}

#[test]
fn language_activation_switches_the_shared_i18n_bundle() {
    set_language(Language::En); // deterministic baseline
    let mut app = headless_shell_app();
    mount_settings(&mut app);
    let zh_choice = choice_entity(app.world_mut(), &SettingsField::Language(Language::Zh));
    activate(&mut app, zh_choice);
    assert_eq!(
        current_language(),
        Language::Zh,
        "the language activation wrote the process-global bundle entry"
    );
    // Hygiene: the bundle is process-global, so restore the baseline even
    // though nextest isolates each test in its own process.
    set_language(Language::En);
}
