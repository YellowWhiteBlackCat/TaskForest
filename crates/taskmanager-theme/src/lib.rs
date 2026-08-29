//! TaskForest's own theme layer (ADR-017 Phase 1, ADR-026).
//!
//! Everything the eight skin variants (4 skins × light/dark), the
//! high-contrast axis, fonts, corner radii, and window-chrome decisions need —
//! **toolkit-neutral** (ADR-026): the toolkit bindings live in cfg'd modules
//! (`gpui`, `iced`, each behind its own optional feature, off by default),
//! and a frontend (TUI) whose color type is a lossy terminal mapping maps
//! these types onto its own rendering vocabulary. Modules:
//!
//! - [`color`] — the neutral color/length/ratio/weight types.
//! - [`theme`] — the resolved runtime token snapshot ([`Theme`]) plus the
//!   axis enums (skin / mode / high-contrast / material / chrome / corners).
//! - [`skins`] — the 8-variant token tables ([`SkinTokens`],
//!   [`skins::tokens_for`]).
//! - [`palette`] — the window/panel color contract for UI crates:
//!   `window_backdrop` (transparent-capable) and `surface` (always opaque)
//!   are independent tokens (ADR-017 §3.1 upstream lesson).
//! - [`tokens`] — semantic radius/spacing/type tokens.
//! - [`platform`] — platform rendering compensation policy, projected by
//!   BOTH toolkit bindings so it can never drift to one frontend (CORE-07).
//! - [`fonts`] — font roles, preferences, host availability, resolution.
//! - [`detection`] — native-appearance facts → theme axes mapping.

#![forbid(unsafe_code)]
#![deny(clippy::wildcard_imports)]

pub mod color;
pub mod detection;
pub mod fonts;
pub mod palette;
pub mod platform;
pub mod skins;
pub mod theme;
pub mod tokens;

/// The gpui bindings (conversions + window/appearance/font/animation
/// helpers), compiled only under `feature = "gpui"` (ADR-026).
#[cfg(feature = "gpui")]
pub mod gpui;

/// The iced bindings (free-function token→value conversions), compiled only
/// under `feature = "iced"` (ADR-026, CORE-07). Free functions, not `From`
/// impls: the orphan rule would actually permit `From<Color> for
/// iced::Color` here, but iced's enum weight and named-family font targets
/// are not `From`-shaped, and one uniform call surface keeps the binding
/// auditable as the frontend's single conversion source.
#[cfg(feature = "iced")]
pub mod iced;

pub use color::{Color, FontSize, Length, Ratio, Weight};
pub use detection::{NativeAppearance, detect_high_contrast, detect_mode, detect_skin};
pub use fonts::{
    FONT_MISANS_VF, FONT_ROBOTO_MONO, FontAvailability, FontChoice, FontPreference, FontRole,
    ResolvedFonts, intern_font_family, resolve_fonts,
};
pub use palette::Palette;
pub use skins::SkinTokens;
pub use theme::{
    EdgeTiling, HighContrast, LightDark, Material, RadiusScale, Skin, Theme, WindowChromeState,
    WindowControls, WindowCorner, with_alpha,
};
