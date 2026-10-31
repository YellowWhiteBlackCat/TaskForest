//! test-intent: behavior
//!
//! Headless behavior tests for the System page's cross-frontend parity
//! surface: the adaptive overview grid, the SMBIOS slot topology, the
//! hot-spot sensor rule, the GPU die group, the diagnostic failure guidance
//! and the hardware serial masking toggle.
//!
//! Every assertion measures what the page actually laid out or states: a
//! grid is proven by the measured card geometry, a rule by the projection it
//! produces, and a control by the state it leaves behind.

use super::*;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::Assets;
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::With;
use bevy::image::TextureAtlasLayout;
use bevy::math::{UVec2, Vec2};
use bevy::picking::DefaultPickingPlugins;
use bevy::text::{Font, TextPlugin};
use bevy::transform::TransformPlugin;
use bevy::ui::prelude::BackgroundColor;
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, UiGlobalTransform, UiPlugin};
use bevy::ui_widgets::Activate;
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskforest_contract::{CapabilityId, EventSequence, RequestId};
use taskforest_core::core::hardware::HardwareInfo;
use taskforest_core::core::metrics::{DmiIdentityFacts, SmbiosMemorySnapshot, SmbiosModuleRow};
use taskforest_core::core::sensors::{SensorCenterSnapshot, SensorReading};
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::i18n::t;
use taskmanager_application::{
    CorrelatedEvent, PlatformEventBatch, PlatformEventContext, SmbiosMemoryEvent,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture;
use taskmanager_test_support::{pin_english, smbios_memory};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use crate::app::{FrontendTrack, Page, Route};
use crate::palette::ui_palette;
use crate::runtime::RuntimeCache;
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;
use crate::window_surface::{WindowSurfaceOverlay, WindowSurfaceState};

/// The host inventory the overview states: four fact groups, one card each.
fn fixture_hardware() -> HardwareInfo {
    HardwareInfo {
        hostname: Some("taskforest-workstation".into()),
        os_name: Some("CachyOS".into()),
        os_version: Some("rolling".into()),
        kernel_version: Some("7.2.0".into()),
        kernel_build: Some("ARCH".into()),
        product_name: Some("TiPro 9000".into()),
        firmware_vendor: Some("American Megatrends".into()),
        cpu_brand: Some("Intel Core Ultra 7".into()),
        cpu_cores: Some(22),
        total_memory_mb: Some(32768),
        ..Default::default()
    }
}

/// A shell that states the host inventory and nothing else.
fn hardware_shell() -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_hardware(&mut shell, |slot| *slot = Some(fixture_hardware()));
    shell
}

/// The accepted memory inventory: two populated channels, two free slots and
/// the identity facts a privileged SMBIOS lane supplies.
fn inventory_with_identity() -> SmbiosMemorySnapshot {
    let modules = vec![
        SmbiosModuleRow {
            slot: 0,
            size_mb: Some(16 * 1024),
            locator: Some("ChannelA-DIMM0".into()),
            serial_number: Some("MOD-SERIAL-90210".into()),
            ..Default::default()
        },
        SmbiosModuleRow {
            slot: 2,
            size_mb: Some(16 * 1024),
            locator: Some("ChannelB-DIMM0".into()),
            serial_number: Some("MOD-SERIAL-90211".into()),
            ..Default::default()
        },
    ];
    SmbiosMemorySnapshot::success(
        4,
        2,
        modules,
        Some(DmiIdentityFacts {
            system_serial: Some("SN-REAL-4711".into()),
            board_serial: Some("BOARD-SERIAL-0815".into()),
            system_uuid: Some("4c4c4544-0038-3510-8054-b7c04f433333".into()),
            ..Default::default()
        }),
    )
}

/// Accept one memory inventory through the shell's own request session.
fn seed_memory_inventory(shell: &mut ShellApp, snapshot: SmbiosMemorySnapshot) {
    let attempt = shell.begin_smbios_memory_request();
    if !shell.accept_smbios_memory_request(attempt, RequestId::MIN) {
        return;
    }
    let batch = PlatformEventBatch {
        smbios_memory_events: vec![CorrelatedEvent::new(
            PlatformEventContext {
                request_id: RequestId::MIN,
                capability: CapabilityId::TELEMETRY_MEMORY_SMBIOS,
                provider: None,
                sequence: EventSequence::new(1),
                observed_at_ms: 0,
            },
            SmbiosMemoryEvent::Update(snapshot),
        )],
        ..PlatformEventBatch::default()
    };
    shell.apply_platform_batch(batch);
}

/// One temperature reading of the sensor center.
fn zone_reading(label: &str, temperature_c: f64) -> SensorReading {
    use taskforest_core::core::sensors::{
        SensorDescriptor, SensorMagnitude, SensorMeasurementObservation, SensorScale,
    };

    let descriptor = SensorDescriptor::temperature(SensorScale::IDENTITY);
    let observation = SensorMeasurementObservation::available(
        descriptor,
        SensorMagnitude::Decimal(temperature_c),
        1_000,
    )
    .expect("valid thermal-zone fixture");
    SensorReading::from_measurement_observation(
        format!("thermal:{label}:zone:0:temperature").into(),
        label.to_owned(),
        label.to_owned(),
        observation,
    )
}

/// The headless System page over a measured viewport.
fn system_app(width: u32, height: u32) -> App {
    let (client, _recorder) = smbios_memory::platform(SmbiosMemorySnapshot::default());
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime = cache
        .get_or_init_with_config(move || Ok((client, None)))
        .expect("quiet runtime");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(HeadlessFrontendPlugins)
        .add_plugins(FrontendWindowPlugin {
            runtime,
            palette: ui_palette(&Theme::dark(), UiSize::Standard),
        });
    app.init_resource::<Assets<Font>>();
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
    app.world_mut().resource_mut::<Route>().page = Page::System;
    app
}

/// The number of distinct measured positions, within half a pixel.
fn distinct(values: impl Iterator<Item = f32>) -> usize {
    let mut sorted: Vec<f32> = values.collect();
    sorted.sort_by(f32::total_cmp);
    sorted.dedup_by(|left, right| (*left - *right).abs() < 1.0);
    sorted.len()
}

/// The text the page states, in mount order.
fn stated_texts(app: &mut App) -> Vec<String> {
    let world = app.world_mut();
    let mut stated: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    let mut readouts = world.query::<&bevy::text::EditableText>();
    stated.extend(readouts.iter(world).map(|text| text.value().to_string()));
    stated
}

/// The masking toggle the page's control strip mounts.
///
/// The control strip is painted with the page, so the control is a fresh
/// entity after every repaint: the caller resolves it again.
fn masking_toggle(app: &mut App) -> Entity {
    app.world_mut()
        .query::<(Entity, &super::privacy::SerialMaskToggle)>()
        .iter(app.world())
        .next()
        .map(|(entity, _)| entity)
        .expect("the masking toggle is mounted")
}

#[test]
fn the_system_cards_fold_into_a_two_by_two_grid_at_720x480() {
    pin_english();
    let mut app = system_app(720, 480);
    app.world_mut().non_send_mut::<FrontendTrack>().shell = hardware_shell();
    for _ in 0..8 {
        app.update();
    }
    let world = app.world_mut();
    let cards: Vec<(Vec2, Vec2)> = world
        .query_filtered::<(&ComputedNode, &UiGlobalTransform), With<SystemFactCard>>()
        .iter(world)
        .map(|(node, transform)| (node.size(), transform.translation))
        .collect();
    assert_eq!(
        cards.len(),
        4,
        "the host inventory states four hardware cards: {cards:?}"
    );
    assert_eq!(
        distinct(cards.iter().map(|(_, center)| center.x)),
        2,
        "the four cards fold into two columns at 720x480: {cards:?}"
    );
    assert_eq!(
        distinct(cards.iter().map(|(_, center)| center.y)),
        2,
        "the four cards fold into two rows at 720x480: {cards:?}"
    );
    let (body, body_transform) = world
        .query_filtered::<(&ComputedNode, &UiGlobalTransform), With<SystemBody>>()
        .iter(world)
        .next()
        .expect("the System body");
    let left = body_transform.translation.x - body.size().x / 2.0;
    let right = body_transform.translation.x + body.size().x / 2.0;
    for (size, center) in &cards {
        assert!(
            center.x - size.x / 2.0 >= left - 0.5,
            "a card leaves the left edge of the body: {cards:?}"
        );
        assert!(
            center.x + size.x / 2.0 <= right + 0.5,
            "a card leaves the right edge of the body: {cards:?}"
        );
    }
}

#[test]
fn a_wide_window_keeps_the_four_cards_on_one_row() {
    pin_english();
    let mut app = system_app(1280, 720);
    app.world_mut().non_send_mut::<FrontendTrack>().shell = hardware_shell();
    for _ in 0..8 {
        app.update();
    }
    let world = app.world_mut();
    let cards: Vec<(Vec2, Vec2)> = world
        .query_filtered::<(&ComputedNode, &UiGlobalTransform), With<SystemFactCard>>()
        .iter(world)
        .map(|(node, transform)| (node.size(), transform.translation))
        .collect();
    assert_eq!(cards.len(), 4, "the host inventory states four cards");
    assert_eq!(
        distinct(cards.iter().map(|(_, center)| center.y)),
        1,
        "the four cards share one row at 1280x720: {cards:?}"
    );
    assert_eq!(
        distinct(cards.iter().map(|(_, center)| center.x)),
        4,
        "the four cards keep their own column at 1280x720: {cards:?}"
    );
}

#[test]
fn the_smbios_slots_render_a_graphical_channel_topology() {
    pin_english();
    let mut app = system_app(1280, 720);
    let mut shell = hardware_shell();
    seed_memory_inventory(&mut shell, inventory_with_identity());
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    for _ in 0..8 {
        app.update();
    }
    let world = app.world_mut();
    let cells: Vec<bool> = world
        .query::<&super::topology::MemorySlotCell>()
        .iter(world)
        .map(|cell| cell.populated)
        .collect();
    assert_eq!(
        cells.len(),
        4,
        "the board map draws one cell per type-17 slot: {cells:?}"
    );
    assert_eq!(
        cells.iter().filter(|filled| **filled).count(),
        2,
        "the populated slots are the filled cells: {cells:?}"
    );
    let texts = stated_texts(&mut app);
    assert!(
        texts.iter().any(|text| text == t("system.memory_slots")),
        "the topology card states the slot map: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "ChannelA"),
        "the topology names the first memory channel: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "ChannelB"),
        "the topology names the second memory channel: {texts:?}"
    );
}

#[test]
fn a_hot_sensor_is_pinned_to_the_top_and_highlighted() {
    pin_english();
    let sensors = SensorCenterSnapshot {
        readings: vec![
            zone_reading("acpitz", 54.5),
            zone_reading("x86_pkg_temp", 91.0),
        ],
        ..Default::default()
    };
    let projection = super::thermal::sensor_temperature_projection(&sensors);
    assert!(projection.zones.pinned, "the hot reading is pinned");
    assert_eq!(
        projection.zones.rows[0].label, "x86_pkg_temp",
        "the reading above the hot threshold leads the group"
    );

    let mut app = system_app(1280, 720);
    let mut shell = hardware_shell();
    fixture::seed_projection_fact(
        &mut shell,
        fixture::ProjectionSeedFact::Sensors(Some(sensors)),
    );
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    for _ in 0..8 {
        app.update();
    }
    let world = app.world_mut();
    let hot: Vec<Entity> = world
        .query_filtered::<Entity, With<super::thermal::HotSensorRow>>()
        .iter(world)
        .collect();
    assert_eq!(hot.len(), 1, "exactly one row wears the hot-spot wash");
    let fill = world
        .get::<BackgroundColor>(hot[0])
        .expect("the hot row carries the theme's danger wash");
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    assert_eq!(
        fill.0.to_srgba().red,
        palette.danger_color.to_srgba().red,
        "the hot row is washed with the theme's danger ink"
    );
    assert_eq!(
        fill.0.to_srgba().green,
        palette.danger_color.to_srgba().green,
        "the hot row is washed with the theme's danger ink"
    );
    // The pinned row is painted above the cool one.
    let hot_y = world
        .get::<UiGlobalTransform>(hot[0])
        .expect("the hot row is placed")
        .translation
        .y;
    let cool = world
        .query::<(&Text, &ChildOf)>()
        .iter(world)
        .find(|(text, _)| text.0 == "acpitz")
        .and_then(|(_, parent)| world.get::<UiGlobalTransform>(parent.parent()))
        .expect("the cool reading is placed")
        .translation
        .y;
    assert!(
        hot_y < cool,
        "the pinned hot reading is painted above the cool one: {hot_y} vs {cool}"
    );
}

#[test]
fn a_snapshot_without_a_hot_sensor_pins_nothing() {
    pin_english();
    let sensors = SensorCenterSnapshot {
        readings: vec![
            zone_reading("acpitz", 54.5),
            zone_reading("x86_pkg_temp", 71.0),
        ],
        ..Default::default()
    };
    let projection = super::thermal::sensor_temperature_projection(&sensors);
    assert!(
        !projection.zones.pinned,
        "a reading below the hot threshold pins nothing"
    );
    let labels: Vec<&str> = projection
        .zones
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect();
    assert_eq!(
        labels,
        ["acpitz", "x86_pkg_temp"],
        "the projection order survives when no reading is hot"
    );

    let mut app = system_app(1280, 720);
    let mut shell = hardware_shell();
    fixture::seed_projection_fact(
        &mut shell,
        fixture::ProjectionSeedFact::Sensors(Some(sensors)),
    );
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    for _ in 0..8 {
        app.update();
    }
    let world = app.world_mut();
    let hot = world
        .query_filtered::<Entity, With<super::thermal::HotSensorRow>>()
        .iter(world)
        .count();
    assert_eq!(hot, 0, "no row is washed when no reading is hot");
}

#[test]
fn gpu_die_temperatures_get_their_own_group_card() {
    pin_english();
    let sensors = SensorCenterSnapshot {
        readings: vec![
            zone_reading("acpitz", 54.5),
            zone_reading("GPU Hot Spot", 72.0),
        ],
        ..Default::default()
    };
    let projection = super::thermal::sensor_temperature_projection(&sensors);
    assert_eq!(
        projection.gpu.rows.len(),
        1,
        "the GPU die channel is grouped apart: {projection:?}"
    );
    assert_eq!(projection.gpu.rows[0].label, "GPU Hot Spot");
    assert_eq!(
        projection.zones.rows.len(),
        1,
        "the host zone stays with the host zones: {projection:?}"
    );
    assert_eq!(projection.zones.rows[0].label, "acpitz");

    let mut app = system_app(1280, 720);
    let mut shell = hardware_shell();
    fixture::seed_projection_fact(
        &mut shell,
        fixture::ProjectionSeedFact::Sensors(Some(sensors)),
    );
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    for _ in 0..8 {
        app.update();
    }
    let texts = stated_texts(&mut app);
    let gpu_title = format!("{} · {}", t("common.gpu"), t("common.temperature"));
    assert!(
        texts.iter().any(|text| text == &gpu_title),
        "the GPU die channels get their own group card: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == t("common.thermal_zones")),
        "the host thermal zones keep their own card: {texts:?}"
    );
}

#[test]
fn a_failed_diagnostic_export_shows_a_centred_troubleshooting_surface() {
    use taskforest_core::core::diagnostics::{DiagnosticBundleError, DiagnosticBundleErrorKind};

    pin_english();
    let mut app = crate::window::tests::scripted_frontend_app();
    app.world_mut().resource_mut::<Route>().page = Page::System;
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
    let size = UVec2::new(720, 480);
    app.world_mut().spawn((
        Window {
            resolution: (720, 480).into(),
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
    app.world_mut().non_send_mut::<FrontendTrack>().shell = fixture::demo_app();
    app.world_mut()
        .trigger(super::diagnostic_modal::DiagnosticCommand::Failure(
            DiagnosticBundleError::with_detail(
                DiagnosticBundleErrorKind::Io,
                "the report directory is not writable",
            ),
        ));
    for _ in 0..6 {
        app.update();
    }
    let world = app.world_mut();
    let guidance = world
        .query_filtered::<Entity, With<super::diagnostic_modal::DiagnosticFailureGuidance>>()
        .iter(world)
        .next()
        .expect("the failed export states a troubleshooting surface");
    let overlay = world
        .query_filtered::<Entity, With<WindowSurfaceOverlay>>()
        .iter(world)
        .next()
        .expect("the diagnostic overlay is mounted");
    let guidance_center = world
        .get::<UiGlobalTransform>(guidance)
        .expect("the guidance surface is placed")
        .translation
        .x;
    let overlay_center = world
        .get::<UiGlobalTransform>(overlay)
        .expect("the overlay is placed")
        .translation
        .x;
    assert!(
        (guidance_center - overlay_center).abs() < 2.0,
        "the troubleshooting surface is centred in the modal: {guidance_center} vs {overlay_center}"
    );
    let texts: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    assert!(
        texts.iter().any(|text| text == t("diagnostics.failed")),
        "the troubleshooting surface states the failure: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text == DiagnosticBundleErrorKind::Io.stable_code()),
        "the troubleshooting surface states the stable failure code: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == t("first_run.retry")),
        "the retry action stays on the surface: {texts:?}"
    );
    // The typed flow still works: a retry re-prepares the review.
    app.world_mut()
        .trigger(super::diagnostic_modal::DiagnosticCommand::Retry);
    app.update();
    assert!(
        matches!(
            app.world().resource::<WindowSurfaceState>().diagnostic(),
            Some(DiagnosticBundleUiState::Preview(_))
        ),
        "the retry re-prepares the review through the shared typed path"
    );
}

#[test]
fn the_serial_masking_toggle_masks_the_real_serial() {
    pin_english();
    let mut app = system_app(1280, 720);
    let mut shell = hardware_shell();
    seed_memory_inventory(&mut shell, inventory_with_identity());
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    for _ in 0..8 {
        app.update();
    }
    let texts = stated_texts(&mut app);
    assert!(
        texts.iter().any(|text| text == "SN-REAL-4711"),
        "the observed serial is stated before masking: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "MOD-SERIAL-90210"),
        "the observed module serial is stated before masking: {texts:?}"
    );

    let toggle = masking_toggle(&mut app);
    app.world_mut().trigger(Activate { entity: toggle });
    for _ in 0..4 {
        app.update();
    }
    let texts = stated_texts(&mut app);
    assert!(
        !texts.iter().any(|text| text == "SN-REAL-4711"),
        "the real serial is gone once masking is on: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text == "MOD-SERIAL-90210"),
        "the real module serial is gone once masking is on: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text == super::privacy::MASKED_SERIAL),
        "the serial is replaced by the mask: {texts:?}"
    );

    let toggle = masking_toggle(&mut app);
    app.world_mut().trigger(Activate { entity: toggle });
    for _ in 0..4 {
        app.update();
    }
    let texts = stated_texts(&mut app);
    assert!(
        texts.iter().any(|text| text == "SN-REAL-4711"),
        "the real serial returns when masking is off again: {texts:?}"
    );
}
