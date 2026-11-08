//! Alt+Arrow spatial focus navigation over live UI bounds using the shared
//! `zhuge-ui-core` navigation kernel.
//!
//! The product retains `InputFocus` as its single focus authority. This module
//! only adds a typed bridge from that authority to `zhuge-ui-core::navigation`:
//! it never invents a second focus ring or a second geometry source.

use bevy::app::PostUpdate;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::message::MessageReader;
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::ButtonState;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::window::PrimaryWindow;
use zhuge_ui_core::navigation::{Direction, FocusCandidate, Rect, find_best_candidate};

/// Marker on every control that takes part in spatial focus.
///
/// The product owns it: it is the component this frontend's decorator inserts and
/// this module's query reads, and no other product's Bevy UI registers a spatial
/// focus target.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusTarget;

/// Convert a live Bevy UI node's local center/size into the shared layout
/// rectangle consumed by `zhuge-ui-core`'s spatial navigation kernel.
#[must_use]
pub(crate) fn node_bounds(node: &ComputedNode, transform: &UiGlobalTransform) -> Rect {
    Rect::from_origin_size(
        transform.translation.x - node.size().x * 0.5,
        transform.translation.y - node.size().y * 0.5,
        node.size().x,
        node.size().y,
    )
}

/// Resolve the next keyboard focus entity from the live bounds of all
/// spatially registered controls. This is a headless-testable seam and the
/// single route into the shared `zhuge-ui-core` navigation authority.
#[must_use]
pub(crate) fn spatial_focus_next(
    current: Rect,
    direction: Direction,
    candidates: impl IntoIterator<Item = (Entity, Rect)>,
) -> Option<Entity> {
    find_best_candidate(
        current,
        direction,
        candidates
            .into_iter()
            .map(|(entity, rect)| FocusCandidate { key: entity, rect }),
    )
}

type SpatialQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ComputedNode,
        &'static UiGlobalTransform,
        &'static FocusTarget,
    ),
>;

/// Alt+Arrow moves keyboard focus through spatially registered controls.
/// The system deliberately requires the Alt modifier so it cannot steal the
/// existing page arrow-key navigation contracts.
pub(crate) fn handle_spatial_focus_navigation(
    mut focus: ResMut<InputFocus>,
    modality: Res<crate::focus_visible::InputModalityState>,
    mut events: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<Entity, With<PrimaryWindow>>,
    controls: SpatialQuery,
) {
    if !modality.shows_focus_ring() {
        return;
    }
    let Some(window) = windows.iter().next() else {
        return;
    };
    if !keys.pressed(KeyCode::AltLeft) && !keys.pressed(KeyCode::AltRight) {
        return;
    }

    let mut direction = None;
    for event in events.read() {
        if event.window != window || event.state != ButtonState::Pressed {
            continue;
        }
        direction = Some(match event.key_code {
            KeyCode::ArrowLeft => Direction::Left,
            KeyCode::ArrowRight => Direction::Right,
            KeyCode::ArrowUp => Direction::Up,
            KeyCode::ArrowDown => Direction::Down,
            _ => continue,
        });
        break;
    }
    let Some(direction) = direction else {
        return;
    };

    let mut current_rect = None;
    let candidates: Vec<_> = controls
        .iter()
        .filter(|(_, node, _, _)| node.size().x > 0.0 && node.size().y > 0.0)
        .map(|(entity, node, transform, _)| {
            let rect = node_bounds(node, transform);
            if focus.get() == Some(entity) {
                current_rect = Some(rect);
            }
            (entity, rect)
        })
        .collect();

    if let Some(current) = current_rect
        && let Some(next) = spatial_focus_next(current, direction, candidates.iter().copied())
    {
        focus.set(next, FocusCause::Navigated);
    } else if focus.get().is_none()
        && let Some((first, _)) = candidates
            .into_iter()
            .min_by_key(|(_, rect)| (rect.min_y.round() as i32, rect.min_x.round() as i32))
    {
        focus.set(first, FocusCause::Navigated);
    }
}

pub(crate) fn register(app: &mut bevy::app::App) {
    app.add_systems(PostUpdate, handle_spatial_focus_navigation);
}

#[cfg(test)]
#[path = "../tests/headless/spatial_focus.rs"]
mod tests;
