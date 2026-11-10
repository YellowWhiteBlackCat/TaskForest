//! test-intent: behavior
//!
//! The optical nesting depth is a MEASURED fact of the composed UI: it is the
//! number of optical surfaces painted under a surface that share its
//! rectangle. These tests prove the derivation and the published depths, so a
//! page cannot opt a surface into a second optical layer by assertion alone —
//! and a surface that really floats over a glass card really reports the
//! depth the shared theme's fail-closed decision consumes.

use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::{Rect, Vec2};
use bevy::prelude::MinimalPlugins;
use bevy::ui::{ComputedNode, ComputedStackIndex, UiGlobalTransform};
use taskmanager_theme::GlassTier;

use super::{OpticalNestingDepths, OpticalSurfacePlacement, optical_nesting_depths};
use crate::glass::optical::OpticalGlassCard;
use crate::window::AppShellRoot;

fn placement(index: u32, stack_index: u32, rect: Rect) -> OpticalSurfacePlacement {
    OpticalSurfacePlacement {
        entity: Entity::from_raw_u32(index).expect("a valid raw entity index"),
        stack_index,
        rect,
    }
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::from_corners(Vec2::new(x, y), Vec2::new(x + width, y + height))
}

#[test]
fn a_surface_under_nothing_is_a_top_level_surface() {
    let only = placement(1, 7, rect(0.0, 0.0, 100.0, 100.0));
    let depths = optical_nesting_depths(&[only]);
    assert_eq!(depths, vec![(only.entity, 0)]);
}

#[test]
fn a_surface_that_shares_no_rectangle_with_a_lower_surface_stays_top_level() {
    let lower = placement(1, 3, rect(0.0, 0.0, 100.0, 100.0));
    let beside = placement(2, 9, rect(400.0, 0.0, 100.0, 100.0));
    let depths = optical_nesting_depths(&[beside, lower]);
    assert_eq!(depths, vec![(lower.entity, 0), (beside.entity, 0)]);
}

#[test]
fn a_surface_painted_over_a_shared_rectangle_reports_the_nesting_depth() {
    let lower = placement(1, 3, rect(0.0, 0.0, 100.0, 100.0));
    let upper = placement(2, 9, rect(50.0, 50.0, 100.0, 100.0));
    let depths = optical_nesting_depths(&[upper, lower]);
    assert_eq!(
        depths,
        vec![(lower.entity, 0), (upper.entity, 1)],
        "the upper surface shares its rectangle with one lower optical surface"
    );
}

#[test]
fn three_stacked_surfaces_report_their_chain_depth() {
    let lowest = placement(1, 2, rect(0.0, 0.0, 100.0, 100.0));
    let middle = placement(2, 5, rect(10.0, 10.0, 100.0, 100.0));
    let highest = placement(3, 8, rect(20.0, 20.0, 100.0, 100.0));
    let depths = optical_nesting_depths(&[highest, lowest, middle]);
    assert_eq!(
        depths,
        vec![(lowest.entity, 0), (middle.entity, 1), (highest.entity, 2)],
        "each surface counts the optical surfaces painted under it"
    );
}

fn computed(size: Vec2, inverse_scale_factor: f32) -> ComputedNode {
    ComputedNode {
        size,
        unrounded_size: size,
        inverse_scale_factor,
        ..ComputedNode::default()
    }
}

fn optical_card(app: &mut App, stack_index: u32, size: Vec2, center: Vec2) -> Entity {
    app.world_mut()
        .spawn((
            OpticalGlassCard {
                tier: GlassTier::Card,
                enabled: true,
            },
            computed(size, 0.5),
            UiGlobalTransform::from_xy(center.x, center.y),
            ComputedStackIndex(stack_index),
        ))
        .id()
}

#[test]
fn the_published_depths_follow_the_measured_composition() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.init_resource::<OpticalNestingDepths>();
    app.world_mut().spawn((
        AppShellRoot,
        computed(Vec2::new(800.0, 600.0), 0.5),
        UiGlobalTransform::from_xy(400.0, 300.0),
    ));

    let card = optical_card(
        &mut app,
        3,
        Vec2::new(200.0, 100.0),
        Vec2::new(300.0, 150.0),
    );
    let overlay = optical_card(
        &mut app,
        9,
        Vec2::new(200.0, 100.0),
        Vec2::new(350.0, 160.0),
    );
    let beside = optical_card(
        &mut app,
        11,
        Vec2::new(200.0, 100.0),
        Vec2::new(700.0, 500.0),
    );

    assert!(
        app.world_mut()
            .run_system_once(super::sync_optical_nesting_depths)
            .is_ok()
    );

    let depths = app.world().resource::<OpticalNestingDepths>();
    assert_eq!(depths.depth_of(card), 0, "the card has nothing under it");
    assert_eq!(
        depths.depth_of(overlay),
        1,
        "the overlay shares its rectangle with the card below it"
    );
    assert_eq!(
        depths.depth_of(beside),
        0,
        "a surface beside the card is not stacked over it"
    );
    assert!(
        depths.has_nested_surface(),
        "the composition really carries a second optical layer"
    );
}

#[test]
fn a_disabled_card_is_not_an_optical_layer() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.init_resource::<OpticalNestingDepths>();
    app.world_mut().spawn((
        AppShellRoot,
        computed(Vec2::new(800.0, 600.0), 0.5),
        UiGlobalTransform::from_xy(400.0, 300.0),
    ));
    let card = optical_card(
        &mut app,
        3,
        Vec2::new(200.0, 100.0),
        Vec2::new(300.0, 150.0),
    );
    let overlay = optical_card(
        &mut app,
        9,
        Vec2::new(200.0, 100.0),
        Vec2::new(350.0, 160.0),
    );
    app.world_mut().entity_mut(card).insert(OpticalGlassCard {
        tier: GlassTier::Card,
        enabled: false,
    });

    assert!(
        app.world_mut()
            .run_system_once(super::sync_optical_nesting_depths)
            .is_ok()
    );

    let depths = app.world().resource::<OpticalNestingDepths>();
    assert_eq!(
        depths.depth_of(overlay),
        0,
        "a painted card is not an optical layer to stack over"
    );
    assert!(!depths.has_nested_surface());
}
