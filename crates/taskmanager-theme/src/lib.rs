//! TaskForest's own theme layer (ADR-017 Phase 1, ADR-026).
//!
//! Everything the eight skin variants (4 skins × light/dark), the
//! high-contrast axis, fonts, corner radii, and window-chrome decisions need —
//! **toolkit-neutral** (ADR-026, ADR-051): every module compiles zero toolkit
//! code and the dependency table names no toolkit on any target. Each
//! frontend owns its token→toolkit binding inside its own dependency closure
//! (the GPUI binding in `taskmanager-ui::theme_binding`, the iced binding in
//! `taskmanager-iced::theme_binding`); the TUI keeps its lossy terminal color
//! quantization locally (ADR-026). Modules:
//!
//! - [`color`] — the neutral color/length/ratio/weight types.
//! - [`theme`] — the resolved runtime token snapshot ([`Theme`]) plus the
//!   axis enums (skin / mode / high-contrast / material / chrome / corners).
//! - [`skins`] — the 8-variant token tables ([`SkinTokens`],
//!   [`skins::tokens_for`]).
//! - [`palette`] — the window/panel color contract for UI crates:
//!   `window_backdrop` (transparent-capable) and `surface` (always opaque)
//!   are independent tokens (ADR-017 §3.1 upstream lesson).
//! - [`optical_glass`] — the toolkit-neutral optical-glass spec, renderer
//!   capability report, and fail-closed fallback contract (ADR-061).
//! - [`surface`] — shared semantic glass roles and tier/elevation mapping.
//! - [`tokens`] — semantic radius/spacing/type tokens.
//! - [`platform`] — platform rendering compensation policy, projected by
//!   BOTH toolkit bindings so it can never drift to one frontend (CORE-07).
//! - [`fonts`] — font roles, preferences, host availability, resolution.
//! - [`settings`] — the public theme-settings contract: the color-scheme
//!   preference axis, the shared appearance override, and the one token
//!   vocabulary each axis persists through (ADR-063).
//! - [`detection`] — native-appearance facts → theme axes mapping.

#![forbid(unsafe_code)]
#![deny(clippy::wildcard_imports)]

pub mod color;
pub mod detection;
pub mod fonts;
pub mod optical_glass;
pub mod palette;
pub mod platform;
pub mod settings;
pub mod skins;
pub mod surface;
pub mod theme;
pub mod tokens;

pub use color::{Color, FontSize, Length, Ratio, Weight};
pub use detection::{NativeAppearance, detect_high_contrast, detect_mode, detect_skin};
pub use fonts::{
    BUNDLED_FONT_FAMILIES, FONT_MISANS_VF, FONT_ROBOTO_MONO, FONT_TOKEN_SYSTEM, FontAvailability,
    FontChoice, FontPreference, FontRole, ResolvedFonts, intern_font_family, resolve_fonts,
};
pub use optical_glass::{
    OpticalGlassCapability, OpticalGlassContext, OpticalGlassDecision, OpticalGlassFallback,
    OpticalGlassFeature, OpticalGlassGeometry, OpticalGlassOperations, OpticalGlassQuality,
    OpticalGlassSpec, OpticalGlassUnavailableReason,
};
pub use palette::Palette;
pub use settings::{
    AppearanceOverride, HIGH_CONTRAST_OVERRIDE_ENV, MODE_TOKEN_SYSTEM, ModePreference,
    SKIN_OVERRIDE_ENV,
};
pub use skins::SkinTokens;
pub use surface::{GlassSurfaceRole, SurfaceElevation};
pub use theme::{
    EdgeTiling, GlassLayer, GlassShadow, GlassShadowElevation, GlassShadowLayer,
    GlassShadowProjection, GlassTier, HighContrast, LightDark, Material, RadiusScale, Skin, Theme,
    WindowChromeState, WindowControls, WindowCorner, with_alpha,
};
