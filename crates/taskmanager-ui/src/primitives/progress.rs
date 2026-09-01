//! Progress bar (visual primitive with determinate/indeterminate modes).

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, ElementId, IntoElement, ParentElement, RenderOnce, Styled,
    Window, div, px, relative,
};
use taskmanager_theme::Palette;

/// A horizontal progress bar. `value` in `0..=1` renders a determinate fill;
/// `None` renders an indeterminate sweeping segment (animated).
#[derive(IntoElement)]
pub struct ProgressBar {
    palette: Palette,
    value: Option<f32>,
    height: f32,
    radius: f32,
}

impl ProgressBar {
    /// Build a determinate progress bar with `value` in `0..=1` (clamped).
    pub fn new(value: f32, palette: Palette) -> Self {
        Self {
            palette,
            value: Some(value.clamp(0.0, 1.0)),
            height: 6.0,
            radius: 3.0,
        }
    }

    /// Build an indeterminate progress bar (animated sweep).
    pub fn indeterminate(palette: Palette) -> Self {
        Self {
            palette,
            value: None,
            height: 6.0,
            radius: 3.0,
        }
    }

    /// Bar height override (default 6px).
    #[must_use]
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Corner radius override (default half the height).
    #[must_use]
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// The clamped determinate value, or None when indeterminate.
    #[must_use]
    pub fn value(&self) -> Option<f32> {
        self.value
    }
}

impl RenderOnce for ProgressBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let track = div()
            .w_full()
            .h(px(self.height))
            .rounded(px(self.radius))
            .bg(crate::theme_binding::fill(self.palette.border))
            .overflow_hidden();

        match self.value {
            Some(value) => track.child(
                div()
                    .h_full()
                    .w(relative(value))
                    .rounded(px(self.radius))
                    .bg(crate::theme_binding::fill(self.palette.accent)),
            ),
            None => track.child(
                div()
                    .h_full()
                    .w_1_2()
                    .rounded(px(self.radius))
                    .bg(crate::theme_binding::fill(self.palette.accent))
                    .with_animation(
                        ElementId::Name("tm-progress-indeterminate".into()),
                        Animation::new(Duration::from_millis(1200)).repeat(),
                        |element, delta| {
                            // Slide the segment across the bar.
                            element.ml(relative(delta * 0.5))
                        },
                    ),
            ),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_progress_tests.rs"]
mod tests;
