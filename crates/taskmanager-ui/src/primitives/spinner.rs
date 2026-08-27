//! Cycling loading spinner (visual primitive, animated via gpui's
//! `AnimationExt` + a canvas-drawn arc; the arc angle advances each frame
//! through a shared cell).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, ElementId, IntoElement, PathBuilder, Pixels, Point, RenderOnce,
    Styled, Window, canvas, point, px,
};
use taskmanager_theme::Palette;

/// A circular loading spinner with an accent-colored arc. Pure visual: no
/// interaction state, so no entity is needed (M2 applies to stateful
/// components; this one only animates).
#[derive(IntoElement)]
pub struct Spinner {
    palette: Palette,
    size: f32,
    speed: Duration,
}

impl Spinner {
    /// Build a spinner with the given palette snapshot.
    pub fn new(palette: Palette) -> Self {
        Self {
            palette,
            size: 16.0,
            speed: Duration::from_millis(800),
        }
    }

    /// Pixel size (square).
    #[must_use]
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Rotation period override.
    #[must_use]
    pub fn speed(mut self, speed: Duration) -> Self {
        self.speed = speed;
        self
    }
}

/// Sample points of a stroked circle arc from `start` sweeping `sweep_frac`
/// of a full turn (fraction of 2π). Reused by the ring + arc painting.
fn arc_points(
    o: Point<Pixels>,
    size: f32,
    radius: f32,
    start: f32,
    sweep_frac: f32,
) -> Vec<Point<Pixels>> {
    let center = point(o.x + px(size / 2.0), o.y + px(size / 2.0));
    let steps = 20usize;
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let a = start + sweep_frac * t * std::f32::consts::TAU;
            point(
                center.x + px(radius * a.cos()),
                center.y + px(radius * a.sin()),
            )
        })
        .collect()
}

impl RenderOnce for Spinner {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let angle = Rc::new(Cell::new(0.0f32));
        let angle_for_paint = angle.clone();
        let size = self.size;
        let accent = self.palette.accent;
        let border = self.palette.border;

        let arc = canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let start = angle_for_paint.get();
                let radius = size / 2.0 - 1.5;
                let stroke = px((size / 6.0).max(1.5));

                // Faint full ring underneath (palette border).
                let mut ring = PathBuilder::stroke(stroke);
                ring.add_polygon(&arc_points(bounds.origin, size, radius, 0.0, 1.0), false);
                if let Ok(path) = ring.build() {
                    window.paint_path(path, border);
                }

                // Accent arc sweeping ~70% of a turn, starting at `start`.
                let mut arc = PathBuilder::stroke(stroke);
                arc.add_polygon(&arc_points(bounds.origin, size, radius, start, 0.7), false);
                if let Ok(path) = arc.build() {
                    window.paint_path(path, accent);
                }
            },
        )
        .size(px(size));

        arc.with_animation(
            ElementId::Name("tm-spinner".into()),
            Animation::new(self.speed).repeat(),
            move |element, delta| {
                angle.set(delta);
                element
            },
        )
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_spinner_tests.rs"]
mod tests;
