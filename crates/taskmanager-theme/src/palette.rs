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

/// The three event-severity levels the product paints.
///
/// Severity is a cross-frontend vocabulary: a frontend maps its own domain
/// severity (an alert rule, a security event, a service state) onto one of
/// these levels and reads the ink from the theme's [`SeverityPalette`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SeverityLevel {
    /// Critical — the destructive red: the level demands immediate action.
    Critical,
    /// Warning — the cautionary amber: the level reports a degraded state.
    Warning,
    /// Notice — the informational accent: the level only informs.
    Notice,
}

impl SeverityLevel {
    /// Every level, in descending severity.
    pub const ALL: [Self; 3] = [Self::Critical, Self::Warning, Self::Notice];

    /// The ink this level paints inside the given palette.
    #[must_use]
    pub fn ink(self, palette: SeverityPalette) -> Color {
        match self {
            Self::Critical => palette.critical,
            Self::Warning => palette.warning,
            Self::Notice => palette.notice,
        }
    }
}

/// The resolved severity palette: one ink per severity level.
///
/// The palette is the single authority for severity color. A frontend never
/// assembles its own severity table; it resolves this palette (through the
/// theme snapshot or through the already-resolved status tokens its own
/// toolkit adapter carries) and reads the ink per level.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SeverityPalette {
    critical: Color,
    warning: Color,
    notice: Color,
}

impl SeverityPalette {
    /// Build the palette from the theme's three status tokens.
    ///
    /// [`severity_palette`] is the authority; this constructor exists for
    /// frontend adapters that already carry the resolved status colors and
    /// must not re-derive them.
    #[must_use]
    pub const fn from_status(critical: Color, warning: Color, notice: Color) -> Self {
        Self {
            critical,
            warning,
            notice,
        }
    }

    /// The critical ink.
    #[must_use]
    pub const fn critical(self) -> Color {
        self.critical
    }

    /// The warning ink.
    #[must_use]
    pub const fn warning(self) -> Color {
        self.warning
    }

    /// The notice ink.
    #[must_use]
    pub const fn notice(self) -> Color {
        self.notice
    }
}

/// Resolve the severity palette for a theme snapshot.
///
/// Critical rides the destructive red, Warning the cautionary amber and Notice
/// the informational accent — the three status hues every skin already
/// defines, so the severity palette can never drift from the theme it
/// belongs to.
#[must_use]
pub fn severity_palette(t: &Theme) -> SeverityPalette {
    SeverityPalette::from_status(t.danger, t.warning, t.accent)
}

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

    /// The event-severity palette for this theme snapshot. The single
    /// authority for severity color across every frontend.
    #[must_use]
    pub fn severity_palette(self) -> SeverityPalette {
        severity_palette(&self)
    }
}

#[cfg(test)]
#[path = "../tests/headless/theme_palette.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/headless/theme_severity.rs"]
mod severity_tests;
