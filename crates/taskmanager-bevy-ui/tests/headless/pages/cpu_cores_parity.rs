//! test-intent: behavior
//!
//! Headless behavior tests for the Bevy Performance page's per-core grid:
//! the heterogeneous class grouping, the socket/NUMA envelopes, the
//! utilization warning bands, the responsive column fold, the independent
//! label/value slots, the per-core frequency/temperature subtitle, and the
//! collapse/expand reveal.
//!
//! Every layout claim is a MEASURED fact: the assertions read the laid-out
//! geometry (`ComputedNode` + `UiGlobalTransform`) of the mounted grid, not
//! the constants that produced it.

use bevy::app::App;
use bevy::asset::Assets;
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::world::World;
use bevy::image::TextureAtlasLayout;
use bevy::math::UVec2;
use bevy::picking::DefaultPickingPlugins;
use bevy::scene::WorldSceneExt;
use bevy::text::TextPlugin;
use bevy::transform::TransformPlugin;
use bevy::ui::prelude::{BackgroundColor, ComputedNode, UiGlobalTransform};
use bevy::ui::widget::Text;
use bevy::ui::{IsDefaultUiCamera, UiPlugin};
use bevy::ui_widgets::Activate;
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskmanager_core::core::hardware::{CpuType, HardwareInfo};
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuPackageMetrics, CpuScalarObservations, ScalarObservation, ScalarObservationGroup,
};
use taskmanager_shell::fixture;
use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::*;
use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;
use crate::pages::performance::tests::{
    Folded, GIB, cpu_outcome, headless_perf_app, memory_metrics, projection,
};
use crate::pages::performance::{DynField, DynText};
use crate::palette::ui_palette;

/// One headless composition with a real viewport: the frontend's own
/// window plugin (the palette, the shell track, the text roles) plus the
/// UI layout pass that measures the grid.
fn core_grid_app(width: u32, height: u32) -> App {
    let mut app = headless_perf_app();
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
        IsDefaultUiCamera,
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
    app
}

/// A shell with the observed per-core usage, the given hardware class
/// inventory, and the given package (socket) inventory.
fn folded_cores(usage: &[f32], classes: Vec<CpuType>, packages: Vec<CpuPackageMetrics>) -> Folded {
    let mut folded = Folded::new();
    let at = 1_000;
    let observations = CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(20.0, at),
        core_usage_group: ScalarObservationGroup::available(usage.to_vec(), at),
        ..CpuScalarObservations::default()
    };
    let mut cpu = CpuMetrics::from_observations(observations);
    cpu.packages = packages;
    let memory = memory_metrics(at, 4 * GIB, 16 * GIB, 12 * GIB, (GIB, 4 * GIB));
    folded.apply(
        vec![cpu_outcome(1, at, cpu.clone())],
        projection(1, cpu, memory, Vec::new()),
    );
    let hardware = HardwareInfo {
        cpu_types: classes,
        ..HardwareInfo::default()
    };
    fixture::edit_hardware(&mut folded.shell, |slot| *slot = Some(hardware));
    folded
}

/// A shell whose cores also report a per-core frequency and temperature.
fn folded_cores_with_telemetry(usage: &[f32], frequency: &[u64], temperature: &[f32]) -> Folded {
    let mut folded = Folded::new();
    let at = 1_000;
    let observations = CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(20.0, at),
        core_usage_group: ScalarObservationGroup::available(usage.to_vec(), at),
        per_core_frequency_group: ScalarObservationGroup::available(frequency.to_vec(), at),
        per_core_temperature_group: ScalarObservationGroup::available(temperature.to_vec(), at),
        ..CpuScalarObservations::default()
    };
    let cpu = CpuMetrics::from_observations(observations);
    let memory = memory_metrics(at, 4 * GIB, 16 * GIB, 12 * GIB, (GIB, 4 * GIB));
    folded.apply(
        vec![cpu_outcome(1, at, cpu.clone())],
        projection(1, cpu, memory, Vec::new()),
    );
    folded
}

/// Mount the per-core grid card with the given shell and return the root
/// entity.
fn mount_grid(app: &mut App, shell: &ShellApp) -> Entity {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    app.world_mut()
        .spawn_scene(super::super::cpu::cpu_core_grid_scene(shell, &palette))
        .expect("the per-core grid scene resolves without assets")
        .id()
}

/// Every entity below `root`, breadth first.
fn descendants(world: &World, root: Entity) -> Vec<Entity> {
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        let children = world
            .get::<Children>(entity)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        for child in children {
            found.push(child);
            pending.push(child);
        }
    }
    found
}

/// The per-core cells of the mounted grid, in mount order.
fn cell_indices(world: &mut World) -> Vec<usize> {
    let mut cells = world.query::<&CoreCell>();
    let mut indices: Vec<usize> = cells.iter(world).map(|cell| cell.0).collect();
    indices.sort_unstable();
    indices
}

/// The measured top edge of every core cell, ordered by core index.
fn cell_tops(world: &mut World) -> Vec<f32> {
    let mut cells = world.query::<(&CoreCell, &ComputedNode, &UiGlobalTransform)>();
    let mut measured: Vec<(usize, f32)> = cells
        .iter(world)
        .map(|(cell, _, transform)| (cell.0, transform.translation.y))
        .collect();
    measured.sort_unstable_by_key(|(index, _)| *index);
    measured.into_iter().map(|(_, top)| top).collect()
}

/// The text of the marked dynamic slot.
fn dyn_text(world: &mut World, field: &DynField) -> Option<String> {
    let mut texts = world.query::<(&DynText, &Text)>();
    texts
        .iter(world)
        .find(|(marker, _)| marker.0 == *field)
        .map(|(_, text)| text.0.clone())
}

/// The presentation rank of a core class: the shared matrix order.
fn class_rank(class: CpuType) -> u8 {
    match class {
        CpuType::Performance => 0,
        CpuType::Efficient => 1,
        CpuType::LowPower => 2,
        CpuType::Unknown => 3,
    }
}

/// A host that measured no core yet renders the shared dash, never a
/// fabricated grid of zero-width cores.
#[test]
fn an_unmeasured_core_grid_reads_as_the_shared_dash() {
    let folded = Folded::new();
    let mut app = core_grid_app(1280, 960);
    mount_grid(&mut app, &folded.shell);
    app.update();

    let world = app.world_mut();
    assert!(
        cell_indices(world).is_empty(),
        "an unmeasured host mounts no per-core cell"
    );
    let captions: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    assert!(
        captions.iter().any(|caption| caption == MISSING_VALUE),
        "the empty grid reads as the shared dash"
    );
}

/// BEVY-109: the heterogeneous class inventory groups the grid under the
/// shared typed captions, and every core lands in its observed class.
#[test]
fn heterogeneous_core_classes_group_the_grid_under_typed_captions() {
    let folded = folded_cores(
        &[10.0, 20.0, 30.0, 40.0],
        vec![
            CpuType::Performance,
            CpuType::Performance,
            CpuType::Efficient,
            CpuType::LowPower,
        ],
        Vec::new(),
    );
    let mut app = core_grid_app(1280, 960);
    mount_grid(&mut app, &folded.shell);
    app.update();

    let world = app.world_mut();
    let sections: Vec<(CpuType, usize, Entity)> = {
        let mut query = world.query::<(&CoreClassSection, Entity)>();
        query
            .iter(world)
            .map(|(section, entity)| (section.class, section.count, entity))
            .collect()
    };
    let mut grouped: Vec<(CpuType, usize, Vec<usize>)> = Vec::new();
    for (class, count, entity) in sections {
        let mut indices: Vec<usize> = Vec::new();
        for row in descendants(world, entity) {
            if world.get::<CoreGridRow>(row).is_none() {
                continue;
            }
            for cell in descendants(world, row) {
                if let Some(marker) = world.get::<CoreCell>(cell) {
                    indices.push(marker.0);
                }
            }
        }
        grouped.push((class, count, indices));
    }
    grouped.sort_by_key(|(class, _, _)| class_rank(*class));
    assert_eq!(
        grouped,
        vec![
            (CpuType::Performance, 2, vec![0, 1]),
            (CpuType::Efficient, 1, vec![2]),
            (CpuType::LowPower, 1, vec![3]),
        ],
        "each core class owns exactly its observed cores"
    );
    let captions: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    for label in [
        t("cpu.performance_cores"),
        t("cpu.efficiency_cores"),
        t("cpu.low_power_cores"),
    ] {
        assert!(
            captions.iter().any(|caption| caption == label),
            "the class caption {label} must be rendered"
        );
    }
    assert_eq!(cell_indices(world), vec![0, 1, 2, 3]);
}

/// BEVY-109: a host without a class inventory keeps one flat group — the
/// class is never guessed from missing data.
#[test]
fn a_host_without_a_class_inventory_keeps_one_flat_group() {
    let folded = folded_cores(&[10.0, 20.0], Vec::new(), Vec::new());
    let mut app = core_grid_app(1280, 960);
    mount_grid(&mut app, &folded.shell);
    app.update();

    let world = app.world_mut();
    let mut sections = world.query::<&CoreClassSection>();
    let classes: Vec<CpuType> = sections.iter(world).map(|section| section.class).collect();
    assert_eq!(
        classes,
        vec![CpuType::Unknown],
        "an absent inventory renders one flat group, never a guessed class"
    );
    assert_eq!(cell_indices(world), vec![0, 1]);
}

/// BEVY-113: the package inventory draws one envelope per socket, carries
/// the NUMA node the platform reported, and keeps each core in its own
/// package.
#[test]
fn socket_envelopes_group_the_cores_of_each_package() {
    let mut first = CpuPackageMetrics::new(0);
    first.logical_core_ids = vec![0, 1];
    first.numa_node_id = Some(0);
    let mut second = CpuPackageMetrics::new(1);
    second.logical_core_ids = vec![2, 3];
    second.numa_node_id = Some(1);
    let folded = folded_cores(
        &[10.0, 20.0, 30.0, 40.0],
        vec![
            CpuType::Performance,
            CpuType::Performance,
            CpuType::Efficient,
            CpuType::Efficient,
        ],
        vec![first, second],
    );
    let mut app = core_grid_app(1280, 960);
    mount_grid(&mut app, &folded.shell);
    app.update();

    let world = app.world_mut();
    let mut envelopes = world.query::<&CoreSocketEnvelope>();
    let mut sockets: Vec<(u32, Option<u32>)> = envelopes
        .iter(world)
        .map(|envelope| (envelope.socket_id, envelope.numa_node))
        .collect();
    sockets.sort_unstable();
    assert_eq!(
        sockets,
        vec![(0, Some(0)), (1, Some(1))],
        "each observed package draws its own envelope with its NUMA node"
    );
    let captions: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    assert!(
        captions.iter().any(|caption| caption == "NUMA 0"),
        "the first socket carries its observed NUMA node"
    );
    // The envelope that owns socket 0 holds exactly the cores the package
    // inventory claims for it.
    let mut envelope_roots = world.query::<(&CoreSocketEnvelope, Entity)>();
    let root = envelope_roots
        .iter(world)
        .find(|(envelope, _)| envelope.socket_id == 0)
        .map(|(_, entity)| entity)
        .expect("the first socket envelope");
    let mut owned: Vec<usize> = descendants(world, root)
        .into_iter()
        .filter_map(|entity| world.get::<CoreCell>(entity).map(|cell| cell.0))
        .collect();
    owned.sort_unstable();
    assert_eq!(
        owned,
        vec![0, 1],
        "the socket envelope owns exactly its package's cores"
    );
}

/// BEVY-111: the bar ink follows the utilization bands — calm, amber above
/// the warn band, red above the alarm band.
#[test]
fn the_core_bar_ink_follows_the_utilization_bands() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    assert_eq!(core_bar_ink(Some(10.0), &palette), palette.accent);
    assert_eq!(core_bar_ink(Some(80.0), &palette), palette.accent);
    assert_eq!(core_bar_ink(Some(85.0), &palette), palette.warning_color);
    assert_eq!(core_bar_ink(Some(95.0), &palette), palette.warning_color);
    assert_eq!(core_bar_ink(Some(96.0), &palette), palette.danger_color);
    assert_eq!(
        core_bar_ink(None, &palette),
        palette.accent,
        "an unobserved core never paints a warning"
    );
}

/// BEVY-111: the mounted bar's ink is rewritten from the same folded
/// observation its width shows.
#[test]
fn the_folded_core_bar_paints_the_observed_utilization_band() {
    let folded = folded_cores(&[96.0, 85.0, 10.0], Vec::new(), Vec::new());
    let mut app = core_grid_app(1280, 960);
    app.world_mut().non_send_mut::<FrontendTrack>().shell = folded.shell.clone();
    mount_grid(&mut app, &folded.shell);
    app.update();
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();

    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let world = app.world_mut();
    let mut fills = world.query::<(&CoreBarFill, &BackgroundColor)>();
    let mut inks: Vec<(usize, bevy::color::Color)> = fills
        .iter(world)
        .map(|(fill, color)| (fill.0, color.0))
        .collect();
    inks.sort_unstable_by_key(|(index, _)| *index);
    assert_eq!(
        inks,
        vec![
            (0, palette.danger_color),
            (1, palette.warning_color),
            (2, palette.accent),
        ],
        "each bar's ink follows its own folded utilization"
    );
}

/// Fold the projection once and let the page's fold observer repaint the
/// grid: the bar inks, the measured column policy, and the reveal easing.
fn fold_once(app: &mut App) {
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    app.update();
}

/// BEVY-110: the measured row width decides the column count — the wide
/// four-column grid folds to two columns in a narrow viewport, and the
/// cells land on the measured rows.
#[test]
fn the_core_grid_folds_to_the_compact_columns_in_a_narrow_viewport() {
    let usage = vec![10.0; 8];
    let folded = folded_cores(&usage, Vec::new(), Vec::new());

    let mut narrow = core_grid_app(720, 480);
    mount_grid(&mut narrow, &folded.shell);
    narrow.update();
    fold_once(&mut narrow);
    let narrow_tops = {
        let world = narrow.world_mut();
        cell_tops(world)
    };
    assert_eq!(narrow_tops.len(), 8, "every core keeps a cell");
    assert!(
        (narrow_tops[0] - narrow_tops[1]).abs() < 0.5,
        "the first two cells share the narrow row"
    );
    assert!(
        narrow_tops[2] > narrow_tops[1] + 1.0,
        "the third cell wraps to the next row in the narrow viewport"
    );
    assert!(
        (narrow_tops[2] - narrow_tops[3]).abs() < 0.5,
        "the narrow viewport lays two cells per row"
    );

    let mut wide = core_grid_app(1920, 1080);
    mount_grid(&mut wide, &folded.shell);
    wide.update();
    fold_once(&mut wide);
    let wide_tops = {
        let world = wide.world_mut();
        cell_tops(world)
    };
    assert!(
        (wide_tops[0] - wide_tops[3]).abs() < 0.5,
        "the wide viewport lays four cells per row, measured on the row tops"
    );
    assert!(
        wide_tops[4] > wide_tops[3] + 1.0,
        "the fifth cell starts the next row in the wide viewport"
    );
}

/// BEVY-112: the identity slot and the value slot own independent widths,
/// so a narrow cell never squeezes the core number into the percentage.
#[test]
fn the_core_number_and_percentage_keep_independent_slots() {
    let usage = vec![10.0; 4];
    let folded = folded_cores(&usage, Vec::new(), Vec::new());
    let mut app = core_grid_app(720, 480);
    mount_grid(&mut app, &folded.shell);
    for _ in 0..8 {
        app.update();
    }
    fold_once(&mut app);

    let world = app.world_mut();
    let mut labels = world.query::<(&CoreLabelSlot, &ComputedNode, &UiGlobalTransform)>();
    let mut values = world.query::<(&CoreValueSlot, &ComputedNode, &UiGlobalTransform)>();
    let label = labels
        .iter(world)
        .next()
        .map(|(_, node, transform)| (transform.translation, node.size()))
        .expect("the identity slot");
    let value = values
        .iter(world)
        .next()
        .map(|(_, node, transform)| (transform.translation, node.size()))
        .expect("the value slot");
    assert!(
        value.1.x > 0.0,
        "the value slot keeps its own width in a narrow cell"
    );
    assert!(
        label.1.x > 0.0,
        "the identity slot keeps its own width in a narrow cell"
    );
    assert!(
        label.0.x + label.1.x / 2.0 <= value.0.x - value.1.x / 2.0 + 0.5,
        "the identity slot and the value slot never overlap"
    );
    // The cell keeps room for both slots: the percentage is never pushed
    // out of its own cell by the identity slot.
    let cell = cell_width(world);
    assert!(
        cell >= label.1.x + value.1.x,
        "the cell keeps room for the identity slot and the value slot, measured"
    );
    // Each slot keeps its own declared width: a narrow cell shrinks the bar,
    // never the identity or the percentage.
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let label_width = palette.control_height_rem * 2.0 * palette.rem_px;
    let value_width = palette.control_height_rem * 1.6 * palette.rem_px;
    assert!(
        label.1.x + 0.5 >= label_width,
        "the identity slot keeps its declared width, measured"
    );
    assert!(
        value.1.x + 0.5 >= value_width,
        "the value slot keeps its declared width, measured"
    );
}

/// The measured width of the first per-core cell.
fn cell_width(world: &mut World) -> f32 {
    let mut cells = world.query_filtered::<Entity, With<CoreCell>>();
    let cell = cells.iter(world).next().expect("a per-core cell");
    world
        .get::<ComputedNode>(cell)
        .map(|node| node.size().x)
        .unwrap_or(0.0)
}

/// BEVY-114: the core cell carries the core's frequency and temperature in
/// its micro subtitle, and the percentage in its own value slot.
#[test]
fn the_core_cell_carries_the_frequency_and_temperature_subtitle() {
    let folded = folded_cores_with_telemetry(&[12.0, 40.0], &[3600, 2400], &[45.0, 60.0]);
    let mut app = core_grid_app(1280, 960);
    app.world_mut().non_send_mut::<FrontendTrack>().shell = folded.shell.clone();
    mount_grid(&mut app, &folded.shell);
    app.update();
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();

    let world = app.world_mut();
    assert_eq!(
        dyn_text(world, &DynField::Cpu(CpuField::CoreTelemetry(0))),
        Some("3.6 GHz · 45 °C".to_owned())
    );
    assert_eq!(
        dyn_text(world, &DynField::Cpu(CpuField::CoreTelemetry(1))),
        Some("2.4 GHz · 60 °C".to_owned())
    );
    assert_eq!(
        dyn_text(world, &DynField::Cpu(CpuField::CoreUsage(0))),
        Some("12.0%".to_owned())
    );
}

/// BEVY-114: a core whose secondary facts were never observed reads as the
/// shared dash, never a fabricated zero.
#[test]
fn an_unobserved_core_subtitle_reads_as_the_shared_dash() {
    let folded = folded_cores(&[12.0], Vec::new(), Vec::new());
    let mut app = core_grid_app(1280, 960);
    app.world_mut().non_send_mut::<FrontendTrack>().shell = folded.shell.clone();
    mount_grid(&mut app, &folded.shell);
    app.update();
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();

    let world = app.world_mut();
    assert_eq!(
        dyn_text(world, &DynField::Cpu(CpuField::CoreTelemetry(0))),
        Some(MISSING_VALUE.to_owned())
    );
}

/// BEVY-115: the reveal eases from the open height to the collapsed height
/// and back, and it settles exactly on the content's own height.
#[test]
fn the_core_grid_reveal_eases_between_the_collapsed_and_open_heights() {
    let transition = CoreGridTransition {
        from: 0.0,
        to: 4.0,
        started_at: 10.0,
    };
    assert_eq!(transition.progress(10.0), 0.0);
    let mut previous = transition.value(10.0);
    for step in 1..=6 {
        let advanced = CoreGridTransition {
            from: 0.0,
            to: 4.0,
            started_at: 10.0,
        };
        let value = advanced.value(10.0 + CORE_GRID_REVEAL_SECONDS * step as f32 / 6.0);
        assert!(
            value > previous,
            "the eased height grows monotonically while the grid opens"
        );
        assert!(value <= 4.0, "the eased height never overshoots its target");
        previous = value;
    }
    let settled = CoreGridTransition {
        from: 0.0,
        to: 4.0,
        started_at: 10.0,
    };
    assert_eq!(
        settled.value(10.0 + CORE_GRID_REVEAL_SECONDS),
        4.0,
        "the transition settles on its target"
    );

    let folded = folded_cores(&[10.0; 4], Vec::new(), Vec::new());
    let mut app = core_grid_app(1280, 960);
    mount_grid(&mut app, &folded.shell);
    app.update();
    fold_once(&mut app);
    let open_height = {
        let world = app.world_mut();
        let mut contents = world.query::<(&CoreGridContent, &ComputedNode)>();
        contents
            .iter(world)
            .next()
            .map(|(_, node)| node.size().y)
            .unwrap_or(0.0)
    };
    assert!(open_height > 0.0, "the open grid has a measured height");

    let toggle = {
        let world = app.world_mut();
        world
            .query_filtered::<Entity, With<CoreGridToggle>>()
            .single(world)
            .expect("the logical-core toggle")
    };
    app.world_mut().trigger(Activate { entity: toggle });
    fold_once(&mut app);
    let collapsed = {
        let world = app.world_mut();
        world
            .query::<&CoreGridReveal>()
            .iter(world)
            .next()
            .map(|reveal| reveal.transition.value(f32::MAX))
            .unwrap_or(-1.0)
    };
    assert!(
        collapsed.abs() < 0.001,
        "the collapsed grid settles at zero height, measured on the reveal state"
    );

    app.world_mut().trigger(Activate { entity: toggle });
    fold_once(&mut app);
    let reopened = {
        let world = app.world_mut();
        world
            .query::<&CoreGridReveal>()
            .iter(world)
            .next()
            .map(|reveal| reveal.transition.value(f32::MAX))
            .unwrap_or(-1.0)
    };
    assert!(
        (reopened * 16.0 - open_height).abs() < 1.0,
        "the reopened grid settles on the content's own measured height:          reopened={reopened} (={}px) open_height={open_height}",
        reopened * 16.0
    );
}
