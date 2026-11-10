//! test-intent: behavior
//!
//! Measured bounds for the Performance page: the shared typed rung admits or
//! hides whole groups, and no fixed (non-scroll-owned) band breaches the
//! bottom edge. Scroll-area descendants are classified separately.

use super::*;
use crate::app::{ContentSlot, FrontendTrack, Page, Route, RouteChanged};
use crate::pages::performance::scene::blocks::memory::PerformanceOptionalMemoryLegend;
use crate::pages::performance::scene::sidebar::cores::{CoreGridContent, CoreGridReveal};
use crate::pages::performance::{PerformanceDeviceBody, PerformancePageRoot};
use crate::widgets::layout_budget::{
    PerformanceBudgetSlot, PerformanceFrameBudget, PerformanceLowerBand,
};
use bevy::app::App;
use bevy::asset::Assets;
use bevy::camera::{Camera, Camera2d, ComputedCameraValues, RenderTargetInfo, Viewport};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::world::World;
use bevy::image::TextureAtlasLayout;
use bevy::math::UVec2;
use bevy::picking::DefaultPickingPlugins;
use bevy::text::TextPlugin;
use bevy::transform::TransformPlugin;
use bevy::ui::prelude::{ComputedNode, UiGlobalTransform};
use bevy::ui::{Display, IsDefaultUiCamera, Node, UiPlugin};
use bevy::window::{ExitCondition, PrimaryWindow, Window, WindowPlugin};
use taskmanager_shell::demo_app;
use taskmanager_ui_contract::presentation::vocabulary::VerticalSpace;

fn probe_app(width: u32, height: u32) -> App {
    probe_app_on_page(width, height, Page::Performance)
}

/// One composed window that mounts `page` exactly once, the way the product
/// and the capture route do: the `Route` resource is moved BEFORE the first
/// `RouteChanged`, so no page is ever mounted, unmounted, and mounted again.
/// A double mount is not a harmless detail here — a reveal body that locks
/// shut on its first mount measures correctly open on its second, so a
/// harness that re-routes hides exactly the state the census exists to find.
fn probe_app_on_page(width: u32, height: u32, page: Page) -> App {
    let mut app = crate::pages::performance::tests::headless_perf_app();
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
    app.world_mut().non_send_mut::<FrontendTrack>().shell = demo_app();
    app.world_mut().resource_mut::<Route>().page = page;
    app.world_mut().commands().trigger(RouteChanged);
    for _ in 0..8 {
        app.update();
    }
    app
}

fn descendants(world: &World, root: Entity) -> Vec<Entity> {
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        if let Some(children) = world.get::<Children>(entity) {
            for child in children.iter() {
                found.push(*child);
                pending.push(*child);
            }
        }
    }
    found
}

#[test]
fn the_shared_rung_admits_whole_groups_and_fixed_bands_keep_bottom_bounds() {
    for (width, height) in [(720_u32, 480_u32), (1280, 720), (1280, 920), (2400, 520)] {
        let mut app = probe_app(width, height);
        let world = app.world_mut();

        let budget = *world.resource::<PerformanceFrameBudget>();
        let mut slot_query = world.query_filtered::<&ComputedNode, With<PerformanceBudgetSlot>>();
        let slot_size = slot_query.iter(world).next().map(|node| node.size());
        let mut lower = world.query_filtered::<&Node, With<PerformanceLowerBand>>();
        let lower_displays: Vec<Display> = lower.iter(world).map(|node| node.display).collect();

        let mut roots =
            world.query_filtered::<(Entity, &ComputedNode), With<PerformancePageRoot>>();
        let page_root = roots.iter(world).next().map(|(entity, _)| entity);
        let mut slots =
            world.query_filtered::<(&ComputedNode, &UiGlobalTransform), With<ContentSlot>>();
        let content_slot = slots.iter(world).next().map(|(node, transform)| {
            (
                node.size(),
                node.padding.min_inset,
                node.padding.max_inset,
                node.inverse_scale_factor,
                transform.translation,
            )
        });

        let mut scrolls = world.query_filtered::<
            (Entity, &ComputedNode, &UiGlobalTransform),
            With<PerformanceDeviceBody>,
        >();
        let scroll = scrolls
            .iter(world)
            .next()
            .map(|(entity, node, transform)| (entity, node.size(), transform.translation));
        let scroll_ancestors: Vec<Entity> = scroll
            .map(|(entity, _, _)| {
                let mut found = Vec::new();
                let mut cursor = Some(entity);
                while let Some(current) = cursor {
                    found.push(current);
                    cursor = world.get::<ChildOf>(current).map(|parent| parent.parent());
                }
                found
            })
            .unwrap_or_default();

        let mut legend = world.query_filtered::<&Node, With<PerformanceOptionalMemoryLegend>>();
        let legend_displays: Vec<Display> = legend.iter(world).map(|node| node.display).collect();
        let mut grid =
            world.query_filtered::<(&ComputedNode, &UiGlobalTransform), With<CoreGridContent>>();
        let grid_geometry: Vec<(f32, f32)> = grid
            .iter(world)
            .map(|(node, transform)| (node.size().y, transform.translation.y + node.size().y / 2.0))
            .collect();
        let mut reveal = world.query_filtered::<&Node, With<CoreGridReveal>>();
        let reveal_displays: Vec<Display> = reveal.iter(world).map(|node| node.display).collect();

        let mut outside_fixed = 0_usize;
        let mut inside_scroll = 0_usize;
        let mut worst_fixed_bottom = 0_f32;
        if let Some(root) = page_root {
            for entity in std::iter::once(root).chain(descendants(world, root)) {
                if world
                    .get::<Node>(entity)
                    .is_some_and(|node| node.display == Display::None)
                {
                    continue;
                }
                let mut cursor = world.get::<ChildOf>(entity).map(|parent| parent.parent());
                let mut hidden_ancestor = false;
                while let Some(current) = cursor {
                    if world
                        .get::<Node>(current)
                        .is_some_and(|node| node.display == Display::None)
                    {
                        hidden_ancestor = true;
                        break;
                    }
                    cursor = world.get::<ChildOf>(current).map(|parent| parent.parent());
                }
                if hidden_ancestor {
                    continue;
                }
                let Some(node) = world.get::<ComputedNode>(entity) else {
                    continue;
                };
                let Some(transform) = world.get::<UiGlobalTransform>(entity) else {
                    continue;
                };
                if node.size().y <= 0.0 {
                    continue;
                }
                let bottom = transform.translation.y + node.size().y / 2.0;
                if bottom > height as f32 + 0.5 {
                    let mut cursor = world.get::<ChildOf>(entity).map(|parent| parent.parent());
                    let mut scroll_owned = false;
                    while let Some(current) = cursor {
                        let clipped_ancestor = world
                            .get::<Node>(current)
                            .is_some_and(|node| !node.overflow.y.is_visible());
                        if clipped_ancestor || scroll_ancestors.contains(&current) {
                            scroll_owned = true;
                            break;
                        }
                        cursor = world.get::<ChildOf>(current).map(|parent| parent.parent());
                    }
                    if scroll_owned {
                        inside_scroll += 1;
                    } else {
                        outside_fixed += 1;
                        if bottom > worst_fixed_bottom {
                            worst_fixed_bottom = bottom;
                        }
                    }
                }
            }
        }
        assert!(content_slot.is_some(), "the content slot is measured");
        assert!(slot_size.is_some(), "the budget slot is measured");
        // The core grid is only admissible evidence for this page's bounds
        // while it is actually measured: a reveal body that collapses to zero
        // hides the grid inside the card, and the bottom-edge classification
        // below would then be arguing about pixels that were never painted.
        // A group the shared rung does not admit owns no pixels, so it is
        // exempt by construction.
        let admitted = lower_displays[0] != Display::None;
        assert_eq!(
            grid_geometry
                .iter()
                .filter(|(height, _)| *height > 0.0)
                .count(),
            usize::from(admitted),
            "the core grid measures exactly one body while admitted and none while the group is hidden at {width}x{height}: geometry={grid_geometry:?}, lower_displays={lower_displays:?}"
        );

        assert_eq!(
            lower_displays.len(),
            1,
            "one lower-band group is mounted at {width}x{height}"
        );
        assert_eq!(
            legend_displays.len(),
            1,
            "one vertical group is mounted at {width}x{height}"
        );
        let admits_full = budget.admits_full_inventory();
        assert_eq!(
            lower_displays[0],
            if admits_full {
                Display::Flex
            } else {
                Display::None
            },
            "the lower band follows the shared rung at {width}x{height}"
        );
        let generous = budget.page.vertical_space == VerticalSpace::Generous;
        assert_eq!(
            legend_displays[0],
            if generous {
                Display::Flex
            } else {
                Display::None
            },
            "the vertical group follows the shared capacity bucket at {width}x{height}"
        );
        assert_eq!(
            outside_fixed, 0,
            "no fixed band breaches the bottom edge at {width}x{height}: worst={worst_fixed_bottom}, clipped={inside_scroll}, slot={content_slot:?}, scroll={scroll:?}, grid_geometry={grid_geometry:?}, reveal={reveal_displays:?}"
        );
        assert!(
            worst_fixed_bottom <= height as f32 + 0.5,
            "bottom safety at {width}x{height}"
        );
        assert!(page_root.is_some(), "the Performance page mounts");
    }
}

/// The clipped-group paint invariant. A group that clips its own content
/// (`overflow.y` not visible) and measures ZERO height while its subtree
/// still lays out content is the signature of a surface that paints nothing:
/// the clip hides every pixel its children computed. R132 found exactly this
/// in the per-core grid's reveal body, where the motion collapsed a height it
/// had never measured and every gate stayed green because the grid was only
/// ever mounted outside the page's scroll viewport in tests.
///
/// The census runs over the REAL composed window — the same composition the
/// capture drives — for every page in `Page::ALL` at the capture matrix
/// sizes, so a collapsed clip anywhere in the product surfaces here rather
/// than in a screenshot nobody compares. A group the shared rung legitimately
/// hides (`Display::None` in its ancestry) owns no pixels and is exempt.
#[test]
fn no_clipped_group_hides_painted_content_across_the_page_matrix() {
    for (width, height) in [(1280_u32, 800_u32), (1180, 780), (720, 480)] {
        for page in Page::ALL {
            let mut app = probe_app_on_page(width, height, *page);

            let world = app.world_mut();
            let mut offenders: Vec<(Entity, usize)> = Vec::new();
            let mut suspects: Vec<(Entity, Vec<Entity>)> = Vec::new();
            {
                let mut nodes = world.query::<(Entity, &Children, &Node, &ComputedNode)>();
                for (entity, children, node, computed) in nodes.iter(world) {
                    if node.display == Display::None
                        || node.overflow.y.is_visible()
                        || computed.size().y > 0.0
                    {
                        continue;
                    }
                    let mut cursor = Some(entity);
                    let mut hidden_by_ancestor = false;
                    while let Some(current) = cursor {
                        if world
                            .get::<Node>(current)
                            .is_some_and(|parent| parent.display == Display::None)
                        {
                            hidden_by_ancestor = true;
                            break;
                        }
                        cursor = world.get::<ChildOf>(current).map(|parent| parent.parent());
                    }
                    if !hidden_by_ancestor {
                        suspects.push((entity, children.iter().copied().collect()));
                    }
                }
            }
            for (clip, top) in suspects {
                let mut pending = top;
                let mut painted = 0_usize;
                let mut walked = 0_usize;
                while let Some(child) = pending.pop() {
                    walked += 1;
                    if walked > 4_000 {
                        break;
                    }
                    if world
                        .get::<Node>(child)
                        .is_some_and(|node| node.display == Display::None)
                    {
                        continue;
                    }
                    if world
                        .get::<ComputedNode>(child)
                        .is_some_and(|node| node.size().y > 0.0)
                    {
                        painted += 1;
                    }
                    if let Some(children) = world.get::<Children>(child) {
                        pending.extend(children.iter().copied());
                    }
                }
                if painted > 0 {
                    offenders.push((clip, painted));
                }
            }
            assert!(
                offenders.is_empty(),
                "no clipped group hides laid-out content at {width}x{height} on page {page:?}: {offenders:?}"
            );
        }
    }
}
