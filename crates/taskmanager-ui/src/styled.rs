#![forbid(unsafe_code)]
//! Palette-driven style composition helpers (absorbing gc `styled.rs` ideas,
//! trimmed to TaskManager needs). No color constants live here: every value is
//! derived from a [`Palette`] snapshot (architecture §3.3: state colors are
//! derived by components from palette tokens; focus rings read `palette.ring`).
//!
//! Color math (blend/luminance/contrast/on-accent) is the single-source
//! implementation from `taskmanager-theme` (ADR-020, ADR-026) — this module
//! only composes it into component state colors and converts at the gpui
//! styling boundary (`.bg`/`.text_color` accept the neutral [`Color`] through
//! the theme's `gpui` feature).

use gpui::{Hsla, transparent_black};

use taskmanager_theme::color::mix;
use taskmanager_theme::{Color, Palette};
use zhuge_ui_tokens::Rgba;

/// Composite `a` over `b` with `t` in [0, 1] (linear sRGB channels) — the
/// theme's `mix`, re-exported under the historical component-library name.
#[must_use]
pub fn blend(a: Color, b: Color, t: f32) -> Color {
    mix(a, b, t)
}

/// Hover state for a fill: move toward white on dark fills, toward black on
/// light fills (gc's `background.blend(0.9)` equivalent).
#[must_use]
pub fn hover_fill(color: Color) -> Color {
    let luminance = Rgba::new(color.r, color.g, color.b, color.a).relative_luminance();
    let toward = if luminance > 0.5 {
        Color::BLACK
    } else {
        Color::WHITE
    };
    blend(color, toward, 0.12)
}

/// Active (pressed) state for a fill: always darken slightly.
#[must_use]
pub fn active_fill(color: Color) -> Color {
    blend(color, Color::BLACK, 0.15)
}

/// Disabled foreground: mute toward the surface.
#[must_use]
pub fn disabled_fg(palette: &Palette) -> Color {
    blend(palette.fg, palette.surface, 0.55)
}

/// The shared disabled opacity: one muted, never-interactive weight for every
/// control that reduces itself to a static readout. Callers still keep the
/// control's measured target; only paint fades.
#[must_use]
pub const fn disabled_opacity() -> f32 {
    0.5
}

/// Translucent modal depth wash over the window.
#[must_use]
pub fn scrim(palette: &Palette, alpha: f32) -> Color {
    palette
        .theme
        .glass_scrim()
        .fill
        .with_alpha(alpha.clamp(0.0, 1.0))
}

/// A hairline separator color derived from the palette border token.
#[must_use]
pub fn hairline(palette: &Palette) -> Color {
    palette.border
}

/// Transparent placeholder (e.g. to suppress a hover fill on disabled state).
#[must_use]
pub fn transparent() -> Hsla {
    transparent_black()
}

#[cfg(test)]
#[path = "../tests/gui/ui_styled_tests.rs"]
mod tests;
