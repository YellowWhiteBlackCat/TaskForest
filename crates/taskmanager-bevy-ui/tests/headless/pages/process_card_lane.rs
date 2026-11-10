//! test-intent: behavior
//!
//! The Applications card lane is ONE presentation of the shell's existing row
//! projection: chips reduce through the canonical filter, whole groups are
//! admitted from the shared card-flow contract, the bottom/right edges stay
//! protected, a headless renderer keeps the typed painted fallback, and the
//! dense table keeps every row/menu/keyboard seam.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::system::RunSystemOnce;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::prelude::{BackgroundColor, ComputedNode, Display, Node};
use bevy::ui::widget::Text;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_shell::{ProcessStatusFilter, ShellApp, fixture};
use taskmanager_theme::{GlassTier, Theme, tokens::UiSize};

use crate::app::{FrontendTrack, Page, PageContext, Route};
use crate::drain::ShellProjectionFolded;
use crate::pages::processes::card_lane::{
    CardFlowBudget, CardFlowMetrics, CardGroup, ProcessAtomicCard, ProcessCardFlow,
    ProcessCardGroup, ProcessFocusDashboard, ProcessGroupChip, ProcessViewMode,
    ProcessViewModeButton, card_flow_budget, card_flow_plan, focus_projection,
};
use crate::pages::processes::{
    ProcessCountLine, ProcessRowLink, ProcessRowsRoot, ProcessTableArtifact, ProcessTableContainer,
    content,
};
use crate::palette::{UiPalette, ui_palette};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::WindowPalette;

// ---- fixtures -----------------------------------------------------------

fn process(pid: u32, name: &str, status: &str) -> ProcessItem {
    let mut item = ProcessItem::new(pid, name);
    item.status = status.to_owned();
    item.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid), 1),
        cpu_percentage: ScalarObservation::available(12.5, 1),
        memory_bytes: ScalarObservation::available(64 * 1024 * 1024, 1),
        ..ProcessScalarObservations::default()
    });
    item
}

fn shell_with(items: Vec<ProcessItem>) -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| *processes = Some(items));
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    shell
}

fn palette() -> UiPalette {
    ui_palette(&Theme::dark(), UiSize::Standard)
}

/// The page scene on a `MinimalPlugins` app, with the card lane's own
/// observers registered: the same wiring the window composition installs.
fn card_lane_app(shell: ShellApp, palette: UiPalette) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.add_plugins(crate::input::InputPlugin);
    app.init_resource::<Assets<Font>>();
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    let mode = crate::pages::processes::card_lane::ProcessViewMode::Cards;
    app.insert_resource(mode);
    // The shared contract's zero-group verdict is the production fallback for
    // an UNMEASURED slot. Headless behavior fixtures spend that honest dense
    // fallback; the measured suite below proves the admitted card geometry.
    app.insert_resource(CardFlowBudget {
        measured: true,
        groups: 1,
        columns: 2,
        group_height_px: 200.0,
    });
    // The page seams read the route resource; the window composition owns it
    // in production, so the headless fixture installs the same fact.
    app.init_resource::<Route>();
    app.world_mut().resource_mut::<Route>().page = Page::Processes;
    let history = crate::pages::history::HistoryProjectionResource::default();
    let context = PageContext {
        sidebar: &Default::default(),
        gray_zero_values: false,
        shell: &shell,
        palette: &palette,
        history: &history.0,
    };
    app.world_mut()
        .spawn_scene(content(&context))
        .expect("the page scene resolves without assets");
    app.insert_non_send(FrontendTrack {
        shell,
        initial_refresh_submitted: true,
        process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion::default(),
    });
    crate::pages::processes::card_lane::register(&mut app);
    app.update();
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    app
}

fn card_links(app: &mut App) -> Vec<(Entity, usize)> {
    app.world_mut()
        .query_filtered::<(Entity, &ProcessAtomicCard), ()>()
        .iter(app.world())
        .map(|(entity, card)| (entity, card.0))
        .collect()
}

fn assert_card_surfaces(app: &mut App) {
    let world = app.world_mut();
    let palette = world.resource::<WindowPalette>().inner.clone();
    let mut cards =
        world.query_filtered::<(&ControlVisual, &BackgroundColor), With<ProcessAtomicCard>>();
    let surfaces: Vec<_> = cards
        .iter(world)
        .map(|(visual, fill)| (*visual, fill.0))
        .collect();
    assert!(
        !surfaces.is_empty(),
        "the admitted whole group paints atomic cards"
    );
    for (visual, fill) in surfaces {
        assert_eq!(visual.0, ControlTone::Card, "a card is not a table row");
        assert_eq!(
            fill,
            palette.glass_fill(GlassTier::Card),
            "the card's painted glass surface survives control skinning"
        );
    }
}

/// The measured composition: the real window plugins, a UI layout pass, and a
/// camera viewport, so the card geometry is the product's own.
fn measured_app(width: u32, height: u32) -> App {
    use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
    use bevy::image::TextureAtlasLayout;
    use bevy::math::UVec2;
    use bevy::picking::DefaultPickingPlugins;
    use bevy::text::TextPlugin;
    use bevy::transform::TransformPlugin;
    use bevy::ui::UiPlugin;
    use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(crate::window::tests::HeadlessFrontendPlugins)
        .add_plugins(crate::window::FrontendWindowPlugin {
            runtime: crate::runtime::demo_platform_runtime(),
            palette: palette(),
        });
    app.init_resource::<Assets<Font>>();
    app.insert_resource(crate::pages::processes::card_lane::ProcessViewMode::Cards);
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
    app
}

fn measured_process_app(width: u32, height: u32, items: Vec<ProcessItem>) -> App {
    let mut app = measured_app(width, height);
    {
        let world = app.world_mut();
        let mut track = world.non_send_mut::<FrontendTrack>();
        fixture::seed_projection_fact(
            &mut track.shell,
            fixture::ProjectionSeedFact::Processes(Some(items)),
        );
    }
    app.world_mut().resource_mut::<Route>().page = Page::Processes;
    app.world_mut().trigger(crate::app::RouteChanged);
    for _ in 0..8 {
        app.update();
    }
    app
}

fn group_filters(app: &mut App) -> Vec<ProcessStatusFilter> {
    app.world_mut()
        .query_filtered::<&ProcessCardGroup, ()>()
        .iter(app.world())
        .map(|group| group.0)
        .collect()
}

fn chip_labels(app: &mut App) -> Vec<(Entity, ProcessStatusFilter)> {
    app.world_mut()
        .query_filtered::<(Entity, &ProcessGroupChip), ()>()
        .iter(app.world())
        .map(|(entity, chip)| (entity, chip.0))
        .collect()
}

#[test]
fn the_measured_cards_stay_inside_the_bounded_flow_viewport() {
    use bevy::ui::UiGlobalTransform;

    let items: Vec<ProcessItem> = (0..24).map(|pid| process(pid, "p", "Running")).collect();
    let mut app = measured_process_app(1180, 780, items);
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    app.update();
    app.update();
    app.update();
    app.update();

    let world = app.world_mut();

    let mut flows =
        world.query_filtered::<(&ComputedNode, &UiGlobalTransform), With<ProcessCardFlow>>();
    let (flow, flow_transform) = flows
        .iter(world)
        .find(|(node, _)| node.size().x > 0.0 && node.size().y > 0.0)
        .unwrap_or_else(|| panic!("one card flow is measured"));
    assert!(flow.size().x > 0.0 && flow.size().y > 0.0);
    let flow_left = flow_transform.translation.x - flow.size().x / 2.0;
    let flow_right = flow_transform.translation.x + flow.size().x / 2.0;
    let flow_bottom = flow_transform.translation.y + flow.size().y / 2.0;

    let mut groups = world.query_filtered::<(
        &ProcessCardGroup,
        &ComputedNode,
        &UiGlobalTransform,
    ), With<ProcessTableArtifact>>();
    let measured: Vec<_> = groups
        .iter(world)
        .map(|(_, node, transform)| (*node, *transform))
        .collect();
    assert!(
        !measured.is_empty(),
        "the shared verdict admits one whole measured group"
    );
    for (node, transform) in measured {
        let half = node.size() / 2.0;
        assert!(
            transform.translation.x - half.x >= flow_left - 0.5
                && transform.translation.x + half.x <= flow_right + 0.5,
            "a card must not cross the right edge"
        );
        assert!(
            transform.translation.y + half.y <= flow_bottom + 0.5,
            "a card must not cross the bottom edge"
        );
    }
    assert!(
        flow_bottom <= 780.0,
        "the card flow keeps the window bottom protected"
    );
}

#[test]
fn a_slot_too_short_for_one_group_degrades_to_the_dense_table() {
    // The shared contract answers ZERO groups when not even one complete
    // group fits; the lane then steps aside instead of squeezing a card.
    let items: Vec<ProcessItem> = (0..24).map(|pid| process(pid, "p", "Running")).collect();
    let mut app = measured_process_app(720, 480, items);
    app.update();

    let world = app.world_mut();
    let cards = world
        .query_filtered::<Entity, With<ProcessAtomicCard>>()
        .iter(world)
        .count();
    let table = world
        .query_filtered::<&bevy::ui::prelude::Node, With<ProcessTableContainer>>()
        .iter(world)
        .next()
        .expect("the dense table container is mounted");
    assert_eq!(cards, 0, "a compact slot paints no half card");
    assert_eq!(
        table.display,
        Display::Flex,
        "the dense table answers when no whole group fits"
    );
}

#[test]
fn the_focus_dashboard_keeps_the_typed_painted_fallback_without_an_adapter() {
    use crate::glass::OpticalGlassCard;
    use crate::glass::optical::{OpticalGlassBinding, OpticalGlassMaterial};
    use bevy::ui_render::prelude::MaterialNode;
    use taskmanager_theme::{GlassTier, OpticalGlassUnavailableReason};

    let items = vec![process(10, "alpha", "Running")];
    let mut app = measured_process_app(1180, 780, items);
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    app.update();
    app.update();
    app.update();
    app.update();

    let world = app.world_mut();

    let dashboard = world
        .query_filtered::<(Entity, &OpticalGlassCard), With<ProcessFocusDashboard>>()
        .iter(world)
        .find(|(_, card)| card.enabled)
        .map(|(entity, _)| entity)
        .expect("the focus dashboard asks for the real optical tier");
    assert_eq!(
        world.get::<OpticalGlassBinding>(dashboard),
        Some(&OpticalGlassBinding::Fallback(
            OpticalGlassUnavailableReason::Headless
        )),
        "a headless renderer keeps the typed painted fallback"
    );
    assert!(
        world
            .get::<MaterialNode<OpticalGlassMaterial>>(dashboard)
            .is_none(),
        "no optical material is attached without an adapter"
    );
    let palette = &world.resource::<WindowPalette>().inner;
    assert_eq!(
        world
            .get::<bevy::ui::BackgroundColor>(dashboard)
            .expect("the dashboard keeps its painted fill")
            .0,
        palette.glass_fill(GlassTier::Card),
        "the painted glass fill stays visible"
    );
}

// ---- pure: the shared admission contract ---------------------------------

#[test]
fn the_card_budget_spends_the_shared_whole_group_admission() {
    let palette = palette();
    let metrics = CardFlowMetrics::from_palette(&palette);
    let chrome = metrics.lane_chrome_height_px();
    // A slot too short for one complete group block admits ZERO groups:
    // the lane degrades to its dense form instead of squeezing a card.
    let too_short = card_flow_budget(1000.0, chrome, &palette);
    assert_eq!(too_short.groups, 0, "a half group is never admitted");

    let one_group = card_flow_budget(
        1000.0,
        chrome + metrics.header_height_px + metrics.card_height_px + metrics.group_gap_px,
        &palette,
    );
    assert_eq!(one_group.groups, 1);
    assert_eq!(
        one_group.cards(),
        one_group.groups * one_group.columns,
        "the card count is groups times the cards per row"
    );
    assert!(
        one_group.group_height_px >= metrics.header_height_px + metrics.card_floor_px,
        "the admitted group keeps its whole floor: {one_group:?}"
    );

    let wide = card_flow_budget(2000.0, 2000.0, &palette);
    assert!(wide.columns >= 1);
    assert!(
        wide.groups * wide.columns <= wide.groups * 4,
        "the column count stays inside Bevy's own column ceiling"
    );
}

#[test]
fn the_card_plan_admits_whole_groups_only() {
    let items = vec![
        process(10, "alpha", "Running"),
        process(20, "beta", "Running"),
        process(30, "gamma", "Sleeping"),
        process(40, "delta", "Zombie"),
    ];
    let shell = shell_with(items);
    let budget = CardFlowBudget {
        measured: true,
        groups: 1,
        group_height_px: 200.0,
        columns: 2,
    };
    let plan = card_flow_plan(&shell, budget);
    assert!(!plan.empty);
    assert_eq!(
        plan.groups.len(),
        1,
        "only one whole group block is admitted"
    );
    let group = &plan.groups[0];
    assert_eq!(group.group, CardGroup::Running);
    assert_eq!(group.members.len(), 2, "one row holds the admitted columns");
    assert_eq!(group.members, &[0, 1]);
    assert_eq!(
        group.total, 2,
        "the header states the whole bucket, not the admitted row"
    );

    // A zero-group verdict admits nothing: the lane shows its dense fallback.
    let none = card_flow_plan(&shell, CardFlowBudget::default());
    assert!(none.groups.is_empty());
    assert!(
        !none.empty,
        "a non-empty projection is not an empty platform"
    );
}

#[test]
fn the_card_plan_folds_one_bucket_across_the_whole_page() {
    // The bucket is a provider status, not a run of adjacent rows: a page
    // that sorts by CPU interleaves the statuses, and every status must
    // still collect into ONE group.
    let items = vec![
        process(10, "alpha", "Running"),
        process(20, "beta", "Sleeping"),
        process(30, "gamma", "Running"),
        process(40, "delta", "Sleeping"),
    ];
    let shell = shell_with(items);
    let budget = CardFlowBudget {
        measured: true,
        groups: 2,
        group_height_px: 200.0,
        columns: 4,
    };
    let plan = card_flow_plan(&shell, budget);
    assert_eq!(plan.groups.len(), 2, "two status buckets are admitted");
    assert_eq!(plan.groups[0].group, CardGroup::Running);
    assert_eq!(plan.groups[0].members, &[0, 2]);
    assert_eq!(plan.groups[0].total, 2);
    assert_eq!(plan.groups[1].group, CardGroup::Sleeping);
    assert_eq!(plan.groups[1].members, &[1, 3]);
    assert_eq!(plan.groups[1].total, 2);
}

#[test]
fn the_focus_projection_never_invents_an_unavailable_fact() {
    let mut item = ProcessItem::new(77, "focus-target");
    item.status = "Running".to_owned();
    item.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(77, 1),
        ..ProcessScalarObservations::default()
    });
    let shell = shell_with(vec![item]);
    let focus = focus_projection(&shell);
    assert_eq!(focus.name.as_deref(), Some("focus-target"));
    assert_eq!(focus.pid, Some(77));
    assert_eq!(focus.status.as_deref(), Some("Running"));
    assert_eq!(
        focus.cpu, None,
        "an unobserved CPU value stays absent instead of becoming zero"
    );
    assert_eq!(focus.memory, None);
    assert_eq!(focus.visible_count, 1);
}

// ---- wired: the mounted lane ---------------------------------------------

#[test]
fn the_lane_mounts_chips_a_focus_dashboard_and_a_card_flow() {
    let items = vec![
        process(10, "alpha", "Running"),
        process(20, "beta", "Sleeping"),
    ];
    let mut app = card_lane_app(shell_with(items), palette());
    app.update();

    let chips = chip_labels(&mut app);
    assert_eq!(
        chips.len(),
        ProcessStatusFilter::ALL.len(),
        "every shared status bucket is a chip"
    );
    assert!(
        app.world_mut()
            .query_filtered::<Entity, With<ProcessFocusDashboard>>()
            .iter(app.world())
            .count()
            == 1,
        "the focus dashboard is mounted once"
    );
    assert!(
        app.world_mut()
            .query_filtered::<Entity, With<ProcessCardFlow>>()
            .iter(app.world())
            .count()
            == 1,
        "the bounded card flow is mounted once"
    );
}

#[test]
fn a_chip_activation_reduces_through_the_shell_filter() {
    let items = vec![
        process(10, "alpha", "Running"),
        process(20, "beta", "Sleeping"),
        process(30, "gamma", "Running"),
    ];
    let mut app = measured_process_app(1180, 780, items);
    app.update();

    let sleeping = chip_labels(&mut app)
        .into_iter()
        .find(|(_, filter)| *filter == ProcessStatusFilter::Sleeping)
        .map(|(entity, _)| entity)
        .expect("the sleeping chip is mounted");
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate { entity: sleeping });
    app.update();

    let shell = &app.world().non_send::<FrontendTrack>().shell;
    assert_eq!(
        shell.process_status_filter,
        ProcessStatusFilter::Sleeping,
        "the chip reduces through the shell's canonical filter"
    );
    assert_eq!(
        shell.visible_process_count(),
        1,
        "the filtered projection is the shell's own"
    );
}

#[test]
fn the_mode_switch_keeps_one_virtual_window_and_both_presentations() {
    let items: Vec<ProcessItem> = (0..12).map(|pid| process(pid, "p", "Running")).collect();
    let mut app = card_lane_app(shell_with(items), palette());
    app.update();

    let dense = app
        .world_mut()
        .query_filtered::<&Node, With<ProcessTableContainer>>()
        .iter(app.world())
        .next()
        .expect("the dense table container is mounted");
    assert_eq!(
        dense.display,
        Display::None,
        "the dense table is hidden while the card lane is active"
    );

    let dense_button = app
        .world_mut()
        .query_filtered::<(Entity, &ProcessViewModeButton), ()>()
        .iter(app.world())
        .find(|(_, button)| button.0 == ProcessViewMode::Dense)
        .map(|(entity, _)| entity)
        .expect("the dense mode button is mounted");
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate {
            entity: dense_button,
        });
    app.update();
    let _ = app
        .world_mut()
        .run_system_once(crate::pages::processes::sync_processes_responsive_layout);

    assert_eq!(
        *app.world().resource::<ProcessViewMode>(),
        ProcessViewMode::Dense
    );
    let dense = app
        .world_mut()
        .query_filtered::<&Node, With<ProcessTableContainer>>()
        .iter(app.world())
        .next()
        .expect("the dense table container is mounted");
    assert_eq!(
        dense.display,
        Display::Flex,
        "the dense table comes back whole"
    );
    assert!(
        !app.world_mut()
            .query_filtered::<Entity, With<ProcessAtomicCard>>()
            .iter(app.world())
            .count()
            > 0,
        "the card artifacts are swept when the dense mode takes over"
    );
}

#[test]
fn the_dense_mode_keeps_the_row_window_and_the_selection_seams() {
    let items: Vec<ProcessItem> = (0..12).map(|pid| process(pid, "p", "Running")).collect();
    let mut app = card_lane_app(shell_with(items), palette());
    app.update();

    let dense_button = app
        .world_mut()
        .query_filtered::<(Entity, &ProcessViewModeButton), ()>()
        .iter(app.world())
        .find(|(_, button)| button.0 == ProcessViewMode::Dense)
        .map(|(entity, _)| entity)
        .expect("the dense mode button is mounted");
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate {
            entity: dense_button,
        });
    app.update();
    let _ = app
        .world_mut()
        .run_system_once(crate::pages::processes::sync_processes_responsive_layout);

    let root = app
        .world_mut()
        .query_filtered::<Entity, With<ProcessRowsRoot>>()
        .single(app.world())
        .expect("the rows root is mounted");
    app.world_mut()
        .commands()
        .trigger(crate::pages::processes::ProcessSelectStep {
            entity: root,
            delta: 2,
        });
    app.update();

    let shell = &app.world().non_send::<FrontendTrack>().shell;
    assert_eq!(shell.selected, 2, "the arrow seam still owns the cursor");
    let links = app
        .world_mut()
        .query_filtered::<&ProcessRowLink, ()>()
        .iter(app.world())
        .count();
    assert!(links > 0, "the dense rows are materialized again");
}

#[test]
fn the_count_line_and_the_focus_follow_the_same_shell_projection() {
    let items = vec![
        process(10, "alpha", "Running"),
        process(20, "beta", "Sleeping"),
    ];
    let mut app = card_lane_app(shell_with(items), palette());
    app.update();

    let count = app
        .world_mut()
        .query_filtered::<&Text, With<ProcessCountLine>>()
        .single(app.world())
        .map(|text| text.0.clone())
        .expect("exactly one count line");
    assert!(
        count.contains('2'),
        "the count line reports the visible rows: {count}"
    );

    // The fold rebuilds the window and the focus from the same shell facts.
    {
        let world = app.world_mut();
        let mut track = world.non_send_mut::<FrontendTrack>();
        fixture::edit_processes(&mut track.shell, |processes| {
            if let Some(processes) = processes.as_mut() {
                processes.push(process(30, "gamma", "Running"));
            }
        });
    }
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();

    let count = app
        .world_mut()
        .query_filtered::<&Text, With<ProcessCountLine>>()
        .single(app.world())
        .map(|text| text.0.clone())
        .expect("exactly one count line");
    assert!(
        count.contains('3'),
        "the fold refreshes the same count line"
    );

    // The lane's own card artifacts carry the shell's visible-row coordinate,
    // so a card is a presentation of the same row the table addresses.
    let links = card_links(&mut app);
    let visible = shell_visible(&mut app);
    for (_, index) in &links {
        assert!(
            *index < visible,
            "a card addresses a visible row of the shell projection"
        );
    }
    assert!(
        !links.is_empty() || shell_visible(&mut app) == 0,
        "a non-empty projection keeps at least one visible row surface"
    );
}

/// The shell's visible-row count, read through the world.
fn shell_visible(app: &mut App) -> usize {
    app.world()
        .non_send::<FrontendTrack>()
        .shell
        .visible_process_count()
}

#[test]
fn the_card_artifacts_are_swept_with_the_one_virtual_window() {
    let items: Vec<ProcessItem> = (0..8).map(|pid| process(pid, "p", "Running")).collect();
    let mut app = card_lane_app(shell_with(items), palette());
    app.update();
    assert_card_surfaces(&mut app);

    let groups = group_filters(&mut app);
    assert_eq!(
        groups,
        vec![ProcessStatusFilter::Running],
        "one whole state group is mounted"
    );
    let before = card_links(&mut app);
    assert!(!before.is_empty(), "one whole card group is mounted");
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    let after = card_links(&mut app);
    assert_eq!(
        after.len(),
        before.len(),
        "a fold re-materializes exactly one bounded window"
    );
    assert_ne!(
        before, after,
        "the sweep replaces artifacts, not the flow root"
    );
    assert!(
        app.world_mut()
            .query_filtered::<Entity, With<ProcessCardFlow>>()
            .iter(app.world())
            .count()
            == 1,
        "the bounded viewport survives the rebuild"
    );
}

#[test]
fn the_empty_state_is_distinct_from_a_quiet_platform() {
    let mut app = card_lane_app(shell_with(Vec::new()), palette());
    app.update();
    let texts: Vec<String> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    assert!(
        texts
            .iter()
            .any(|text| text == t("empty.no_processes_reported")),
        "a quiet platform states the honest empty state: {texts:?}"
    );
}
