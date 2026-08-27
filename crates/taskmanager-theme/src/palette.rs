//! The window/panel color contract for UI crates (P2/P3 consumers, ADR-017
//! §3.1).
//!
//! Upstream lesson: gpui-component uses ONE `background` token for both the
//! window backdrop and Dialog/panel surfaces, so a transparent backdrop and an
//! opaque panel cannot coexist — the project carried a vendored patch for
//! Linux CSD rounded corners (`patches/gpui-component/`). [`Palette`] splits
//! them: [`Palette::window_backdrop`] may be transparent (Linux CSD), while
//! [`Palette::surface`] is ALWAYS opaque and never derived from the backdrop.
//!
//! Values derive from the resolved [`Theme`] tokens so nothing re-invents
//! hues: `surface` is the theme's panel surface (`view_bg`, the same token
//! the tooltip/popover/dialog family paints), the status accents come from the
//! skin tables (`Theme::danger`/`success`/`warning`), and `ring` carries the
//! per-frame focus-visible decision as its alpha (0 = non-keyboard render) —
//! the same semantics the project's former `sync_gc_frame_state` helper once
//! pushed into the toolkit before that helper was removed.

use crate::color::{Color, Length};
use crate::theme::{RadiusScale, Theme, with_alpha};

/// Resolved color contract handed to UI components. Every field is a concrete
/// sRGB token; state-derived colors (hover/active/disabled) are derived by
/// components from these, and focus rings always read [`Palette::ring`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Palette {
    /// Window backdrop. Transparent-capable: on a Linux CSD surface
    /// (`Theme::window_transparent`) the alpha is 0 so the chrome's own
    /// rounded-corner paint shows through. Never used for panel surfaces.
    pub window_backdrop: Color,
    /// Component panel surface (dialog / tooltip / card family) — the theme's
    /// ELEVATED card fill ([`Theme::card_surface`]: the skin's own `card_bg`
    /// when distinct, a derived lift otherwise). ALWAYS opaque — an
    /// independent token, never derived from `window_backdrop`
    /// (ADR-017 §3.1 upstream lesson).
    pub surface: Color,
    /// Primary text on `surface`.
    pub fg: Color,
    /// Secondary / disabled text.
    pub fg_muted: Color,
    /// Interactive accent (selection, primary actions, focus fill).
    pub accent: Color,
    /// Selected-row / active-selection surface (translucent accent tint,
    /// derived from the theme's [`Theme::selection_bg`]).
    pub selection: Color,
    /// Hovered-row / hovered-control surface (fainter accent tint, derived
    /// from the theme's [`Theme::hover_bg`]).
    pub hover: Color,
    /// Search-match text color (the theme's highlight token).
    pub highlight_fg: Color,
    /// Destructive actions / error states.
    pub danger: Color,
    /// Positive outcomes.
    pub success: Color,
    /// Cautionary states.
    pub warning: Color,
    /// Hairline separators / outlines.
    pub border: Color,
    /// Focus ring. alpha = 0 means this render is not keyboard-focused
    /// (pointer-driven focus does not draw a ring).
    pub ring: Color,
    /// Card/panel drop-shadow ink (the theme's [`Theme::card_shadow`]) —
    /// components build their two-layer shadows from this single token.
    pub card_shadow: Color,
    /// Accent-gradient start stop for primary surfaces
    /// ([`Theme::gradient_from`], brighter).
    pub gradient_from: Color,
    /// Accent-gradient end stop for primary surfaces
    /// ([`Theme::gradient_to`], darker).
    pub gradient_to: Color,
    /// Panel / popup / dialog / menu corner radius — the skin's
    /// [`RadiusScale::Large`] tier. Components must round surfaces through
    /// this token, never a hardcoded pixel (skins differ: Breeze 5, GNOME 10).
    pub panel_radius: Length,
    /// Control (button / tooltip / select / input) corner radius — the skin's
    /// [`RadiusScale::Medium`] tier.
    pub control_radius: Length,
    /// Small control (checkbox / header / row highlight) corner radius — the
    /// skin's [`RadiusScale::Small`] tier.
    pub small_radius: Length,
    /// Tiny surface (track fill / legend dot) corner radius — the skin's
    /// [`RadiusScale::XSmall`] tier.
    pub xsmall_radius: Length,
}

impl Theme {
    /// The resolved color contract for this theme snapshot.
    pub fn palette(self) -> Palette {
        Palette {
            // The backdrop may be fully transparent on a Linux CSD surface
            // (the chrome paints the rounded corners itself); otherwise the
            // skin's own — possibly Mica-hinted, sub-1 alpha — backdrop.
            window_backdrop: with_alpha(
                self.window_bg,
                if self.window_transparent {
                    0.0
                } else {
                    self.window_bg.a
                },
            ),
            // The app's own panel family (tooltip / popover / dialog
            // surfaces) paints the elevated card fill; it is opaque in
            // every variant.
            surface: with_alpha(self.card_surface(), 1.0),
            fg: self.fg,
            fg_muted: self.fg_dim,
            accent: self.accent,
            selection: self.selection_bg(),
            hover: self.hover_bg(),
            highlight_fg: self.highlight_fg(),
            danger: self.danger,
            success: self.success,
            warning: self.warning,
            border: self.border,
            ring: with_alpha(self.accent, if self.focus_visible() { 1.0 } else { 0.0 }),
            card_shadow: self.card_shadow(),
            gradient_from: self.gradient_from(),
            gradient_to: self.gradient_to(),
            panel_radius: Length(self.radius(RadiusScale::Large)),
            control_radius: Length(self.radius(RadiusScale::Medium)),
            small_radius: Length(self.radius(RadiusScale::Small)),
            xsmall_radius: Length(self.radius(RadiusScale::XSmall)),
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/theme_palette.rs"]
mod tests;
