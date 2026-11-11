//! test-intent: behavior
//!
//! Elastic-layout proof for the Applications details rail and its scroll
//! bounds. The rail is admitted as one whole group, the bottom safety band
//! keeps two control rows, and every DPI scale snaps the lower edge to a
//! complete detail row instead of exposing a partial final row.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::Vec2;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::prelude::{ComputedNode, Display, Node, Val};
use bevy::ui_widgets::ScrollArea;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_application::{AppAction, AppPage};
use taskmanager_shell::{ShellApp, fixture};
use taskmanager_theme::{Theme, tokens::UiSize};
use taskmanager_ui_contract::presentation::vocabulary::COMPACT_CONTENT_WIDTH;

use crate::app::{FrontendTrack, Page, PageContext, Route};
use crate::drain::ShellProjectionFolded;
use crate::pages::processes::card_lane::ProcessViewMode;
use crate::pages::processes::details::scroll::whole_row_scroll_offset;
use crate::pages::processes::details::{
    ProcessDetailsRoot, ProcessDetailsRow, detail_row_slots, details_bottom_inset_px,
};
use crate::pages::processes::{ProcessDetailsContainer, ProcessPageRoot, content};
use crate::palette::{UiPalette, ui_palette};
use crate::window::WindowPalette;

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

fn card_lane_app(shell: ShellApp, palette: UiPalette) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app.add_plugins(crate::input::InputPlugin);
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    let mode = ProcessViewMode::Cards;
    app.insert_resource(mode);
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

fn measured_app_at_scale(width: u32, height: u32, scale_factor: f32) -> App {
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
    app.insert_resource(ProcessViewMode::Cards);
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
    let size = UVec2::new(
        (width as f32 * scale_factor).round() as u32,
        (height as f32 * scale_factor).round() as u32,
    );
    app.world_mut().spawn((
        Window {
            resolution: bevy::window::WindowResolution::new(size.x, size.y)
                .with_scale_factor_override(scale_factor),
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
                    scale_factor,
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

fn seed_measured_process_app(app: &mut App, items: Vec<ProcessItem>) {
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
}

#[test]
fn the_details_rail_is_admitted_as_a_whole_group_by_the_measured_page_slot() {
    let mut app = card_lane_app(shell_with(Vec::new()), palette());
    app.update();

    let page_root = app
        .world_mut()
        .query_filtered::<Entity, With<ProcessPageRoot>>()
        .single(app.world())
        .expect("the page root is mounted");
    let details_root = app
        .world_mut()
        .query_filtered::<Entity, With<ProcessDetailsContainer>>()
        .single(app.world())
        .expect("the details container is mounted");

    let set_slot = |app: &mut App, width: f32| {
        let mut entity = app
            .world_mut()
            .get_entity_mut(page_root)
            .expect("page root entity");
        entity.insert(ComputedNode {
            size: Vec2::new(width, 720.0),
            inverse_scale_factor: 1.0,
            ..ComputedNode::default()
        });
        let _ = app
            .world_mut()
            .run_system_once(crate::pages::processes::sync_processes_responsive_layout);
    };

    set_slot(&mut app, COMPACT_CONTENT_WIDTH - 1.0);
    let display = app
        .world()
        .get::<Node>(details_root)
        .map(|node| node.display);
    assert_eq!(
        display,
        Some(Display::None),
        "one pixel below the shared band hides the whole details rail"
    );

    set_slot(&mut app, COMPACT_CONTENT_WIDTH);
    let display = app
        .world()
        .get::<Node>(details_root)
        .map(|node| node.display);
    assert_eq!(
        display,
        Some(Display::Flex),
        "the shared band admits the details rail beside the card lane"
    );
}

#[test]
fn the_details_scroll_area_reserves_a_control_height_bottom_safety_band() {
    let mut app = card_lane_app(shell_with(Vec::new()), palette());
    app.update();

    let palette = palette();
    let node = app
        .world_mut()
        .query_filtered::<&Node, (With<ProcessDetailsRoot>, With<ScrollArea>)>()
        .single(app.world())
        .expect("the details scroll area is mounted");
    assert_eq!(
        node.padding.bottom,
        Val::Px(details_bottom_inset_px(&palette)),
        "the scroll content keeps two whole control rows after its final detail row"
    );
    assert!(
        details_bottom_inset_px(&palette) >= palette.control_height_px() * 2.0,
        "an intermediate scroll position cannot expose a partial final row through the safety band"
    );
}

#[test]
fn intermediate_details_scroll_snaps_the_lower_edge_to_a_whole_row_boundary() {
    let rows = [(0.0, 28.0), (36.0, 64.0), (72.0, 100.0)];
    let correction = whole_row_scroll_offset(0.0, 0.0, 80.0, 90.0, 4.0, &rows);
    assert_eq!(
        correction, 14.0,
        "scrolling down reveals the complete row and a four-pixel clear gap"
    );
    assert_eq!(
        whole_row_scroll_offset(0.0, 0.0, 80.0, 100.0, 4.0, &rows),
        0.0,
        "an already whole row boundary keeps the official scroll position"
    );
    assert_eq!(
        whole_row_scroll_offset(20.0, 30.0, 80.0, 90.0, 4.0, &rows),
        -12.0,
        "scrolling up excludes the partial row without reversing direction"
    );
    assert_eq!(
        whole_row_scroll_offset(0.0, 5.0, 0.0, 15.0, 4.0, &[(0.0, 28.0)]),
        0.0,
        "a scroll limit that cannot reach either row edge stays clamped"
    );
    assert_eq!(
        whole_row_scroll_offset(80.0, 80.0, 80.0, 90.0, 4.0, &rows),
        -22.0,
        "the maximum scroll boundary selects the reachable whole-row edge"
    );
}

#[test]
fn the_wide_measured_details_viewport_never_exposes_a_partial_final_row() {
    use bevy::ui::{ScrollPosition, UiGlobalTransform};

    for scale_factor in [1.0, 1.25, 1.5, 2.0] {
        let items: Vec<ProcessItem> = (0..24).map(|pid| process(pid, "p", "Running")).collect();
        let mut app = {
            let mut app = measured_app_at_scale(1280, 800, scale_factor);
            seed_measured_process_app(&mut app, items);
            app
        };
        app.world_mut().commands().trigger(ShellProjectionFolded);
        for _ in 0..5 {
            app.update();
        }
        let world = app.world_mut();
        let maximum = world
            .query_filtered::<&ComputedNode, With<ProcessDetailsRoot>>()
            .single(world)
            .map(|viewport| {
                let inverse_scale_factor = viewport.inverse_scale_factor();
                (viewport.content_size().y * inverse_scale_factor
                    - viewport.size().y * inverse_scale_factor)
                    .max(0.0)
            })
            .expect("the wide details scroll owner is measured");
        assert!(
            maximum > 0.0,
            "the wide details viewport really scrolls at {scale_factor}×"
        );

        for fraction in [0.0, 0.17, 0.41, 0.63, 0.37, 0.0, 1.0] {
            let world = app.world_mut();
            world
                .query_filtered::<&mut ScrollPosition, With<ProcessDetailsRoot>>()
                .single_mut(world)
                .expect("the scroll position stays mounted")
                .0
                .y = maximum * fraction;
            app.update();
            let world = app.world_mut();
            let (viewport, transform, scroll) = world
                .query_filtered::<(&ComputedNode, &UiGlobalTransform, &ScrollPosition), With<ProcessDetailsRoot>>()
                .single(world)
                .expect("the wide details scroll owner stays measured");
            let inverse_scale_factor = viewport.inverse_scale_factor();
            assert!(
                (inverse_scale_factor - scale_factor.recip()).abs() < 0.001,
                "the measured UI uses the requested {scale_factor}× scale factor"
            );
            let visible = viewport.size().y * inverse_scale_factor;
            let viewport_bottom = transform.translation.y * inverse_scale_factor + visible * 0.5;
            let applied_scroll = viewport.scroll_position.y * inverse_scale_factor;
            assert!(
                (scroll.0.y - applied_scroll).abs() <= 1.0,
                "Bevy layout applies and clamps the snapped scroll position at {scale_factor}×"
            );
            assert!(
                scroll.0.y >= 0.0 && scroll.0.y <= maximum + 1.0,
                "the snapped offset remains inside the ScrollArea range at {scale_factor}×"
            );
            let cropped = world
                .query_filtered::<(&ComputedNode, &UiGlobalTransform), With<ProcessDetailsRow>>()
                .iter(world)
                .any(|(node, transform)| {
                    let inverse_scale_factor = node.inverse_scale_factor();
                    let half_height = node.size().y * inverse_scale_factor * 0.5;
                    let top = transform.translation.y * inverse_scale_factor - half_height;
                    let bottom = transform.translation.y * inverse_scale_factor + half_height;
                    top < viewport_bottom - 0.5 && bottom > viewport_bottom + 0.5
                });
            assert!(
                !cropped,
                "a partial row is not visible at {scale_factor}× and scroll {fraction:.2}"
            );
        }
    }
}

#[test]
fn a_detail_row_keeps_its_label_and_value_in_separate_bounded_slots() {
    let slots = detail_row_slots();
    assert!(
        slots.label_shrink > 0.0 && slots.label_grow > 0.0,
        "a long localized label may shrink and grow in its own bounded slot"
    );
    assert_eq!(
        slots.value_shrink, 0.0,
        "the value never shrinks out of the right edge"
    );
    assert!(
        slots.value_min_width_rem > 0.0,
        "the value keeps a measured floor in rem"
    );
}
