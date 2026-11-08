//! test-intent: behavior
//!
//! The spatial focus seam is pure geometry over typed entity candidates, so
//! these tests pin the shared navigation mesh and rectangle projection without
//! constructing a UI hierarchy or a live World.

use bevy::ecs::entity::{Entity, EntityIndex};
use zhuge_ui_core::navigation::{Direction, Rect};

use crate::spatial_focus::spatial_focus_next;

#[test]
fn spatial_navigation_uses_the_shared_navigation_mesh() {
    let origin = Rect::from_origin_size(0.0, 0.0, 10.0, 10.0);
    let right = Rect::from_origin_size(40.0, 0.0, 10.0, 10.0);
    let down = Rect::from_origin_size(0.0, 40.0, 10.0, 10.0);

    let first = Entity::from_index(EntityIndex::from_raw_u32(1).expect("valid test entity index"));
    let second = Entity::from_index(EntityIndex::from_raw_u32(2).expect("valid test entity index"));
    let candidates = vec![(first, right), (second, down)];

    assert_eq!(
        spatial_focus_next(origin, Direction::Right, candidates.clone()),
        Some(first)
    );
    assert_eq!(
        spatial_focus_next(origin, Direction::Down, candidates),
        Some(second)
    );
}

#[test]
fn node_bounds_preserve_transform_center_and_size() {
    let rect = Rect::from_origin_size(2.0, 4.0, 10.0, 20.0);
    assert_eq!(rect.width(), 10.0);
    assert_eq!(rect.height(), 20.0);
    assert!(rect.contains_point(7.0, 14.0));
}
