//! test-intent: behavior
//!
//! Settings-page tests:
//!
//! - the discrete capacity ladder projects its live authority exactly (never
//!   the nearest step), and continuous refresh/capacity writes round-trip
//!   through the shell's public entries so a mutation in the projection or
//!   the write path fails here without a world;
//! - the two switchable theme modes resolve and read back from the palette
//!   authority, and a foreign mode combination is never claimed as one of
//!   them;
//! - the wired activation observer on the real plugin composition: a widget
//!   activation writes its authority (shell preferences, the palette
//!   resources, the shared i18n bundle) and remounts the page, whose fresh
//!   tree mirrors the new values — "changes take effect immediately, no
//!   local copy state".

use std::time::Duration;
use taskmanager_theme::tokens::UiSize;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::Assets;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::{Has, With};
use bevy::ecs::world::World;
use bevy::text::Font;
use bevy::ui::Checked;
use bevy::ui::Val;
use bevy::ui::widget::Text;
use taskforest_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, RequestPort, SubmissionError,
};
use taskmanager_application::i18n::{Language, current_language, set_language};
use taskmanager_application::{
    HostTelemetryRequest, PlatformClient, PlatformEffect, PlatformEvent, PlatformFacets,
    PlatformHandle, SystemFacets, TelemetryInterval,
};

use taskmanager_shell::ShellApp;
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

use super::{
    CAPACITY_CHOICES, RefreshSlider, RefreshSliderValue, SettingsChoice, SettingsField,
    capacity_choice_index, interval_from_seconds, palette_mode, refresh_readout, refresh_seconds,
    theme_for_mode,
};
use crate::app::{FrontendTrack, Page, PageContent, Route};
use crate::input::PendingEffects;
use crate::pages::settings::privilege_center::PrivilegeControl;
use crate::pages::settings::{SettingsBody, SettingsFooter, SettingsHeading};
use crate::palette::ui_palette;
use crate::widgets::slider::{SliderPart, SliderPartOwner};
use crate::window::tests::HeadlessFrontendPlugins;
use crate::window::{FrontendWindowPlugin, WindowPalette};
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::image::TextureAtlasLayout;
use bevy::math::UVec2;
use bevy::picking::DefaultPickingPlugins;
use bevy::text::TextPlugin;
use bevy::transform::TransformPlugin;
use bevy::ui::UiPlugin;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskforest_core::core::appearance::DesktopAppearance;
use taskmanager_shell::presentation::privilege_center::PrivilegeAction;

#[test]
fn settings_scroll_body_preserves_header_and_footer_bounds_in_short_frames() {
    for (width, height) in [(720, 360), (720, 480), (1280, 720), (1600, 480), (720, 960)] {
        let mut app = headless_shell_app();
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
        let size = UVec2::new(width, height);
        app.world_mut().spawn((
            Window {
                resolution: (width, height).into(),
                ..Default::default()
            },
            PrimaryWindow,
        ));
        app.world_mut().spawn((
            Camera2d,
            Camera {
                computed: ComputedCameraValues {
                    target_info: Some(RenderTargetInfo {
                        physical_size: size,
                        scale_factor: 1.0,
                    }),
                    ..Default::default()
                },
                viewport: Some(Viewport {
                    physical_size: size,
                    ..Default::default()
                }),
                ..Default::default()
            },
        ));
        mount_settings(&mut app);
        app.update();
        let world = app.world_mut();
        let mut query = world.query::<(
            &ComputedNode,
            &UiGlobalTransform,
            Has<SettingsHeading>,
            Has<SettingsBody>,
            Has<SettingsFooter>,
        )>();
        let mut found = 0;
        for (node, transform, heading, body, footer) in query.iter(world) {
            if !(heading || body || footer) {
                continue;
            }
            found += 1;
            let half = node.size() / 2.0;
            assert!(
                node.size().y > 0.0,
                "every settings slot must retain height at {width}x{height}"
            );
            assert!(
                transform.translation.x + half.x <= width as f32 + 0.5,
                "right edge must fit at {width}x{height}: center={:?}, size={:?}, slots=({heading},{body},{footer})",
                transform.translation,
                node.size()
            );
            assert!(
                transform.translation.y + half.y <= height as f32 + 0.5,
                "bottom edge must fit at {width}x{height}: center={:?}, size={:?}, slots=({heading},{body},{footer})",
                transform.translation,
                node.size()
            );
        }
        assert_eq!(
            found, 3,
            "all mandatory slots and the scroll owner must mount"
        );
    }
}

#[test]
fn permission_center_button_freezes_the_lane_and_rejects_a_stale_offer() {
    let mut app = headless_shell_app();
    let snapshot = CapabilitySnapshot::from_descriptors([CapabilityDescriptor {
        id: CapabilityId::TELEMETRY_CPU_PACKAGE_POWER,
        status: CapabilityStatus::RequiresEscalation,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }]);
    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .apply_capability_snapshot(snapshot);
    mount_settings(&mut app);
    let entity = app
        .world_mut()
        .query::<(Entity, &PrivilegeControl)>()
        .iter(app.world())
        .find(|(_, control)| control.0 == Some(PrivilegeAction::RaplPower))
        .map(|(entity, _)| entity)
        .expect("the offered RAPL action must mount");
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity });
    assert!(matches!(
        app.world().resource::<PendingEffects>().0.as_slice(),
        [PlatformEffect::RaplPower(_)]
    ));
    app.world_mut().resource_mut::<PendingEffects>().0.clear();
    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .apply_capability_snapshot(CapabilitySnapshot::default());
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity });
    assert!(
        app.world().resource::<PendingEffects>().0.is_empty(),
        "a stale rendered button cannot request a removed capability"
    );
}
use taskforest_contract::RequestEnvelope;
use taskforest_core::core::appearance::DesktopFamily;
use taskforest_core::core::appearance::PreferredColorScheme;

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

    fn try_submit(&self, _request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        Ok(())
    }
}

fn scripted_runtime() -> &'static crate::runtime::SharedRuntime {
    let snapshot = CapabilitySnapshot::from_descriptors([
        CapabilityDescriptor {
            id: CapabilityId::TELEMETRY_HOST,
            status: CapabilityStatus::Available,
            providers: Vec::new(),
            observed_at_ms: 1,
            last_success_at_ms: None,
        },
        CapabilityDescriptor {
            id: CapabilityId::TELEMETRY_CPU_PACKAGE_POWER,
            status: CapabilityStatus::RequiresEscalation,
            providers: Vec::new(),
            observed_at_ms: 1,
            last_success_at_ms: None,
        },
    ]);
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
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
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

fn refresh_slider_entity(world: &mut World) -> Entity {
    world
        .query_filtered::<Entity, With<RefreshSlider>>()
        .single(world)
        .expect("the settings page mounts one continuous refresh slider")
}

fn set_refresh_seconds(app: &mut App, seconds: f32, is_final: bool) {
    let entity = refresh_slider_entity(app.world_mut());
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::ValueChange::<f32> {
            source: entity,
            value: seconds,
            is_final,
        });
    app.update();
}

fn refresh_texts(app: &mut App) -> Vec<String> {
    app.world_mut()
        .query_filtered::<&Text, With<RefreshSliderValue>>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect()
}

fn refresh_texts_from_world(world: &mut World) -> Vec<String> {
    world
        .query_filtered::<&Text, With<RefreshSliderValue>>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect()
}

#[test]
fn zero_value_checkbox_changes_real_cells_without_dimming_missing_values() {
    use crate::pages::settings::ThemePreferences;
    use crate::widgets::table::{row_scene, visible_columns};
    use bevy::scene::CommandsSceneExt;
    use bevy::text::TextColor;
    use bevy::ui_widgets::Checkbox;

    let mut app = headless_shell_app();
    mount_settings(&mut app);
    let columns = visible_columns(&[])
        .into_iter()
        .filter(|column| matches!(column.id, "CPU" | "Memory" | "Network"))
        .collect::<Vec<_>>();
    let cells = vec!["0.0%".to_owned(), "—".to_owned(), "8 B/s".to_owned()];
    app.world_mut()
        .commands()
        .spawn_scene(row_scene(&cells, &columns));
    app.update();
    let colors = |app: &mut App| {
        app.world_mut()
            .query::<(&Text, &TextColor)>()
            .iter(app.world())
            .filter(|(text, _)| cells.contains(&text.0))
            .map(|(text, color)| (text.0.clone(), color.0))
            .collect::<std::collections::HashMap<_, _>>()
    };
    let body = app.world().resource::<WindowPalette>().inner.body_color;
    let dim = app.world().resource::<WindowPalette>().inner.dim_color;
    assert_eq!(colors(&mut app)["0.0%"], body);
    for enabled in [true, false] {
        let entity = choice_entity(app.world_mut(), &SettingsField::GrayZeroValues(enabled));
        assert!(app.world().get::<Checkbox>(entity).is_some());
        app.world_mut()
            .trigger(bevy::ui_widgets::ValueChange::<bool> {
                source: entity,
                value: enabled,
                is_final: true,
            });
        app.update();
        assert_eq!(
            app.world().resource::<ThemePreferences>().gray_zero_values,
            enabled
        );
        let current = colors(&mut app);
        assert_eq!(current["0.0%"], if enabled { dim } else { body });
        assert_eq!(current["—"], body);
        assert_eq!(current["8 B/s"], body);
        let next = choice_entity(app.world_mut(), &SettingsField::GrayZeroValues(!enabled));
        assert_eq!(is_checked(app.world_mut(), next), enabled);
    }
}

// ---- pure projections of the authorities ----

#[test]
fn continuous_refresh_and_capacity_writes_round_trip_through_the_shell_entries() {
    let mut shell = ShellApp::new();
    let two_seconds = interval_from_seconds(2.0);
    shell.set_telemetry_interval(two_seconds);
    assert_eq!(shell.telemetry_interval(), two_seconds);
    assert_eq!(
        refresh_readout(shell.telemetry_interval()),
        "2.0 s",
        "the continuous readout follows the accepted cadence"
    );
    let between = TelemetryInterval::clamped(Duration::from_millis(750));
    assert_eq!(
        refresh_seconds(between),
        0.75,
        "an off-step cadence keeps its measured value"
    );
    assert_eq!(
        interval_from_seconds(-4.0),
        TelemetryInterval::clamped(Duration::from_millis(100)),
        "the shared policy clamps a non-positive slider request"
    );
    assert_eq!(
        interval_from_seconds(f32::NAN),
        TelemetryInterval::clamped(Duration::from_millis(100)),
        "an unmeasurable slider request stays typed and bounded"
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
    let light = ui_palette(&theme_for_mode(LightDark::Light), UiSize::Standard);
    let dark = ui_palette(&theme_for_mode(LightDark::Dark), UiSize::Standard);
    assert_ne!(
        light.content_bg.to_srgba(),
        dark.content_bg.to_srgba(),
        "the two switchable modes must be distinguishable on the view surface"
    );
    assert_eq!(palette_mode(&light), Some(LightDark::Light));
    assert_eq!(palette_mode(&dark), Some(LightDark::Dark));
    assert_eq!(
        palette_mode(&ui_palette(&Theme::dark(), UiSize::Standard)),
        Some(LightDark::Dark),
        "the cold-start palette reads back as Dark by construction"
    );
    // A mode combination the switch does not own is never claimed.
    let eye_forest = ui_palette(
        &Theme::build(
            Skin::Gnome,
            LightDark::EyeForest,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        ),
        UiSize::Standard,
    );
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
    for (en, zh) in [
        ("Settings", "设置"),
        ("Mode", "模式"),
        ("Language", "语言"),
        ("High contrast", "高对比度"),
        ("Interface size", "界面大小"),
        ("Refresh interval", "刷新间隔"),
        ("History capacity", "历史容量"),
        ("Telemetry updates", "遥测更新"),
    ] {
        assert!(
            texts.iter().any(|text| text == en || text == zh),
            "the {en}/{zh} row renders: {texts:?}"
        );
    }
    // The switch state words are catalog-owned too: a row reports the shared
    // Enabled/Disabled/LIVE vocabulary rather than a frontend-private casing.
    for (en, zh) in [("Disabled", "已禁用"), ("LIVE", "实时")] {
        assert!(
            texts.iter().any(|text| text == en || text == zh),
            "the {en}/{zh} state renders: {texts:?}"
        );
    }
    assert!(
        texts.iter().any(|text| text == "1.0 s"),
        "the live cadence value renders: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "64 samples"),
        "the live capacity value renders: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text.contains("apply live") && text.contains("persist across sessions")),
        "the footer is honest: choices apply live and persist across sessions"
    );
    // The default cadence is the continuous slider's exact value; the store
    // default capacity (64) still selects no discrete step at all.
    assert_eq!(
        refresh_texts_from_world(world),
        vec!["1.0 s".to_owned()],
        "the live cadence renders through the slider readout"
    );
    assert!(
        world
            .query_filtered::<&bevy::ui_widgets::SliderValue, With<RefreshSlider>>()
            .single(world)
            .is_ok(),
        "the settings page mounts the official continuous slider"
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

    set_refresh_seconds(&mut app, 2.0, true);
    assert_eq!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .telemetry_interval(),
        TelemetryInterval::clamped(Duration::from_millis(2000)),
        "the slider activation wrote the shell entry"
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
    // The remounted tree mirrors the accepted value in the live readout.
    assert_eq!(refresh_texts(&mut app), vec!["2.0 s".to_owned()]);
}

#[test]
fn an_intermediate_refresh_drag_updates_the_readout_without_remounting() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);
    let before = mounted_page_entity(app.world_mut());
    let slider = refresh_slider_entity(app.world_mut());

    set_refresh_seconds(&mut app, 3.7, false);
    assert_eq!(refresh_texts(&mut app), vec!["3.7 s".to_owned()]);
    assert_eq!(
        mounted_page_entity(app.world_mut()),
        before,
        "a held drag does not rebuild the page under the pointer"
    );
    assert_eq!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .telemetry_interval(),
        TelemetryInterval::clamped(Duration::from_millis(3700)),
        "the intermediate value reaches the shell authority"
    );
    for (part, owner, node) in app
        .world_mut()
        .query::<(&SliderPart, &SliderPartOwner, &bevy::ui::Node)>()
        .iter(app.world())
    {
        assert_eq!(owner.0, "settings.refresh");
        let expected = Val::Percent((3.7 - 0.5) / (5.0 - 0.5) * 100.0);
        assert_eq!(
            match part {
                SliderPart::Fill => node.width,
                SliderPart::Thumb => node.left,
            },
            expected,
            "the fill and thumb follow the accepted value"
        );
    }
    assert_eq!(refresh_slider_entity(app.world_mut()), slider);

    set_refresh_seconds(&mut app, 4.0, true);
    assert_ne!(
        mounted_page_entity(app.world_mut()),
        before,
        "the final drag value remounts the fresh projection"
    );
    assert_eq!(refresh_texts(&mut app), vec!["4.0 s".to_owned()]);
}

#[test]
fn theme_activation_swaps_the_palette_authority_and_restyles_the_rail() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    let light_choice = choice_entity(app.world_mut(), &SettingsField::Theme(LightDark::Light));
    activate(&mut app, light_choice);
    let light = ui_palette(&theme_for_mode(LightDark::Light), UiSize::Standard);
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
    let dark = ui_palette(&theme_for_mode(LightDark::Dark), UiSize::Standard);
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

#[test]
fn high_contrast_activation_toggles_high_contrast_on_palette() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);

    assert!(!app.world().resource::<WindowPalette>().inner.high_contrast);

    let hc_choice = choice_entity(app.world_mut(), &SettingsField::HighContrast(true));
    activate(&mut app, hc_choice);
    assert!(
        app.world().resource::<WindowPalette>().inner.high_contrast,
        "high-contrast activation enables high contrast on the palette"
    );

    let hc_off_choice = choice_entity(app.world_mut(), &SettingsField::HighContrast(false));
    activate(&mut app, hc_off_choice);
    assert!(
        !app.world().resource::<WindowPalette>().inner.high_contrast,
        "high-contrast activation disables high contrast on the palette"
    );
}

#[test]
fn system_mode_and_theme_preferences_follow_observed_appearance() {
    use super::ThemePreferences;
    let mut prefs = ThemePreferences::default();
    assert_eq!(prefs.mode, None, "default mode is System (None)");
    assert_eq!(
        prefs.effective_mode(),
        LightDark::Dark,
        "defaults to Dark before observation"
    );

    let light_app = DesktopAppearance {
        family: DesktopFamily::Gnome,
        color_scheme: PreferredColorScheme::Light,
        high_contrast: Some(false),
    };
    prefs.observed_appearance = Some(light_app);
    assert_eq!(
        prefs.effective_mode(),
        LightDark::Light,
        "follows light appearance in System mode"
    );

    let hc_app = DesktopAppearance {
        family: DesktopFamily::Gnome,
        color_scheme: PreferredColorScheme::Dark,
        high_contrast: Some(true),
    };
    prefs.observed_appearance = Some(hc_app);
    assert_eq!(
        prefs.effective_contrast(),
        HighContrast::On,
        "follows high contrast appearance"
    );

    // Explicit choice overrides system observation
    prefs.mode = Some(LightDark::Dark);
    prefs.observed_appearance = Some(light_app);
    assert_eq!(
        prefs.effective_mode(),
        LightDark::Dark,
        "explicit choice is not overridden"
    );
}
