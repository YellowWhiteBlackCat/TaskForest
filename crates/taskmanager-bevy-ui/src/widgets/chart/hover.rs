//! The chart's pointer seam: the crosshair position, the hovered sample
//! slot, and the floating value pill.
//!
//! The pointer arrives through Bevy's picking (`PointerMove`), whose hit data
//! is normalized to the node's own box. That keeps the seam scale-free: the
//! crosshair lands on the sample the pointer is really over, whatever the
//! window's pixel ratio is.

use bevy::ecs::component::Component;
use bevy::ecs::observer::On;
use bevy::ecs::system::Query;
use bevy::math::Vec2;
use bevy::picking::events::{PointerMove, PointerOut};

/// The pointer's position inside the canvas, normalized to the node's box
/// (the origin at the center, the corners at ±0.5).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ChartPointer {
    pub(crate) normalized: Option<Vec2>,
}

/// The floating value pill's marker: the node that carries the hovered
/// sample's readout.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ChartHoverLabel;

/// Observer: remember where the pointer is over the canvas.
pub(crate) fn on_chart_pointer_move(
    pointer_move: On<PointerMove>,
    mut pointers: Query<&mut ChartPointer>,
) {
    if let Ok(mut pointer) = pointers.get_mut(pointer_move.event().entity) {
        pointer.normalized = pointer_move
            .event()
            .hit
            .position
            .map(|position| Vec2::new(position.x, position.y));
    }
}

/// Observer: forget the pointer when it leaves the canvas, so the crosshair
/// and the pill do not linger over a chart nobody points at.
pub(crate) fn on_chart_pointer_out(
    pointer_out: On<PointerOut>,
    mut pointers: Query<&mut ChartPointer>,
) {
    if let Ok(mut pointer) = pointers.get_mut(pointer_out.event().entity) {
        pointer.normalized = None;
    }
}

/// The pointer's position inside a canvas of the given size, in px.
#[must_use]
pub(crate) fn hover_position(normalized: Option<Vec2>, size: Vec2) -> Option<Vec2> {
    let normalized = normalized?;
    let position = (normalized + Vec2::splat(0.5)) * size;
    if position.x < 0.0 || position.y < 0.0 || position.x > size.x || position.y > size.y {
        return None;
    }
    Some(position)
}

/// The sample slot under the pointer.
///
/// The newest sample owns the right edge, so the slot is resolved from the
/// right exactly like the reference surface's slot grid: a sparse window can
/// never report a value from the opposite side.
#[must_use]
pub(crate) fn hover_slot(points: usize, width: f32, x: f32) -> Option<usize> {
    if points == 0 || width <= 0.0 {
        return None;
    }
    if points == 1 {
        return Some(0);
    }
    let relative = (x / width).clamp(0.0, 1.0);
    Some((((points - 1) as f32) * relative).round() as usize)
}

/// The pill's readout: the hovered value, plus how many samples the slot sits
/// behind the newest one.
#[must_use]
pub(crate) fn hover_text(format: Option<fn(f32) -> String>, value: f32, behind: usize) -> String {
    let value = format.map_or_else(|| format!("{value:.0}"), |format| format(value));
    if behind == 0 {
        value
    } else {
        format!("{value} · -{behind}")
    }
}
