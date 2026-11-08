//! Toolkit-neutral color and measurement types (ADR-026).
//!
//! `Color` is the single color vocabulary of the skin system: an sRGB RGBA
//! value with the luminance/contrast/mix math the skins need. `Length`,
//! `Ratio` and `Weight` are the neutral forms of the spacing/type tokens —
//! absolute pixels, relative factors and font weights — which each frontend
//! adapter maps onto its own unit system (gpui `Pixels`/`DefiniteLength`/
//! `FontWeight`, ratatui `Color`, iced `Pixels`/…).
//!
//! No type in this module may depend on a toolkit. The gpui conversions
//! live behind the theme's optional `gpui` feature (ADR-026).

/// sRGB color with linear alpha. Channel values are in 0.0..=1.0.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// A color from raw sRGB channels.
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Opaque color from a `0xRRGGBB` hex literal (`Color::from_hex(0x222226)`).
    pub const fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xff) as f32 / 255.0,
            g: ((hex >> 8) & 0xff) as f32 / 255.0,
            b: (hex & 0xff) as f32 / 255.0,
            a: 1.0,
        }
    }

    /// Opaque black.
    pub const BLACK: Self = Self::from_hex(0x000000);
    /// Opaque white.
    pub const WHITE: Self = Self::from_hex(0xffffff);
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    /// Same channels with a new alpha.
    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Whether the alpha is fully opaque (>= 1.0).
    pub const fn is_opaque(self) -> bool {
        self.a >= 1.0
    }

    /// The opaque sRGB channels as 8-bit values (rounds, never clamps
    /// silently — callers pass in-range colors).
    pub fn to_srgb8(self) -> [u8; 3] {
        [
            (self.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.b.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }
}

/// An absolute length in pixels (the neutral form of a `px` value).
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub struct Length(pub f32);

impl From<Length> for f32 {
    fn from(length: Length) -> f32 {
        length.0
    }
}

/// A toolkit-neutral typographic size on the 14px Small-profile baseline.
/// Backends may keep the raw value (Iced, whose program scale zooms the whole
/// surface) or resolve it as a root-relative unit (GPUI).
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub struct FontSize(pub f32);

impl From<FontSize> for f32 {
    fn from(size: FontSize) -> f32 {
        size.0
    }
}

/// A relative factor (e.g. line-height ratio), the neutral form of a
/// `relative(…)` value.
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub struct Ratio(pub f32);

impl From<Ratio> for f32 {
    fn from(ratio: Ratio) -> f32 {
        ratio.0
    }
}

/// A font weight (variable-font value, e.g. 450 = between regular and
/// medium), the neutral form of a `FontWeight`.
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub struct Weight(pub f32);

impl From<Weight> for f32 {
    fn from(weight: Weight) -> f32 {
        weight.0
    }
}

use zhuge_ui_tokens::Rgba;

/// Pick the higher-contrast control foreground for an accent fill. Several
/// native palettes define a separate accent *link* color, but that token must
/// not be reused as text on an accent button (blue-on-blue in GNOME light was
/// the visible failure that prompted this guard).
pub fn on_accent(accent: Color) -> Color {
    let dark = Color::BLACK;
    let light = Color::WHITE;
    let resolved = |color: Color| Rgba::new(color.r, color.g, color.b, color.a);
    let accent = resolved(accent);
    let dark = resolved(dark);
    let light = resolved(light);
    if accent.contrast_ratio(dark) >= accent.contrast_ratio(light) {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

/// Linear sRGB interpolation between two tokens: `t = 0` → `a`, `t = 1` →
/// `b`, with `t` clamped to the unit interval. Used by the derived surface
/// tokens ([`crate::Theme::card_surface`] and friends) so derived colors stay
/// channel-exact instead of re-inventing per-view blends.
///
/// The blend formula has one authority: [`Rgba::lerp`] in `zhuge-ui-tokens`.
/// This is an adapter that maps the neutral [`Color`] onto it and back; naming
/// the four channels here again would be a second formula for one fact.
pub const fn mix(a: Color, b: Color, t: f32) -> Color {
    let mixed = Rgba::new(a.r, a.g, a.b, a.a).lerp(Rgba::new(b.r, b.g, b.b, b.a), t);
    Color::new(mixed.r, mixed.g, mixed.b, mixed.a)
}

#[cfg(test)]
#[path = "../tests/headless/theme_color.rs"]
mod tests;
