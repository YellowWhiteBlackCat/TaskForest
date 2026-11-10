//! The Iced binding from a shared control material to the gradient a widget
//! paints.
//!
//! `crate::theme::control_glass` owns the ONE glass material the control
//! family wears; a widget style only reads it. This module resolves the
//! gradient out of that material WITHOUT a panic: production code must not
//! abort because a material it did not author carried a different shape. A
//! material without a gradient degrades to the theme's own flat glass fill
//! — the same surface the painted fallback wears — so the widget keeps a
//! real, theme-owned surface instead of a fabricated one.

use iced::widget::container;
use taskmanager_theme::{GlassTier, Theme};

use crate::theme_binding::color;

/// The gradient a control paints for the shared glass material.
///
/// The shared material paints a gradient; when it carries a flat fill the
/// theme's own glass gradient is used, so every control still wears one
/// theme-owned surface.
#[must_use]
pub fn control_glass_gradient(theme: &Theme, material: &container::Style) -> iced::Gradient {
    match material.background {
        Some(iced::Background::Gradient(gradient)) => gradient,
        _ => glass_gradient(theme),
    }
}

/// The theme's own glass gradient for the card tier.
fn glass_gradient(theme: &Theme) -> iced::Gradient {
    let [top, bottom] = theme.glass_gradient(GlassTier::Card);
    iced::Gradient::Linear(
        iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
            .add_stop(0.0, color(top))
            .add_stop(1.0, color(bottom)),
    )
}
