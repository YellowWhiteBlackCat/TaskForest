//! Design tokens: the per-skin radius scale, lifted from [`Theme::radius`]
//! into [`Length`] so call sites read `rounded(tokens::card_radius(t))`
//! instead of a hardcoded `rounded(px(8.0))` that ignores the active skin.
//! Each helper is a semantic binding onto the skin's unified
//! [`RadiusScale`] gradient, so the token name says what the surface IS
//! (card/control/window) while the tier picks the pixels.
//!
//! Types are toolkit-neutral ([`Length`]/[`Ratio`]/[`Weight`], ADR-026):
//! the theme's optional `gpui` module maps them onto gpui's
//! `Pixels`/`DefiniteLength`/`FontWeight`, so call sites keep reading
//! `tokens::SPACE_8` / `tokens::FONT_13` unchanged.

use std::time::Duration;

use crate::color::{FontSize, Length, Ratio, Weight};
use crate::theme::{RadiusScale, Theme};

/// Card / graph / panel corner radius for the active skin — the scale's
/// [`RadiusScale::Large`] tier.
pub fn card_radius(t: &Theme) -> Length {
    Length(t.radius(RadiusScale::Large))
}
/// Control (button / tab / row / input) corner radius for the active skin —
/// the scale's [`RadiusScale::Medium`] tier.
pub fn control_radius(t: &Theme) -> Length {
    Length(t.radius(RadiusScale::Medium))
}
/// Small controls (badges, chips, keycap hints, row highlights) — the scale's
/// [`RadiusScale::Small`] tier.
pub fn small_radius(t: &Theme) -> Length {
    Length(t.radius(RadiusScale::Small))
}
/// Tiny surfaces (legend dots, track fills) — the scale's
/// [`RadiusScale::XSmall`] tier.
pub fn xsmall_radius(t: &Theme) -> Length {
    Length(t.radius(RadiusScale::XSmall))
}
/// Window / large-surface corner radius for the active skin — the scale's
/// [`RadiusScale::XLarge`] tier (the same tier [`Theme::window_corner_radius`]
/// paints for Linux CSD corners).
pub fn window_radius(t: &Theme) -> Length {
    Length(t.radius(RadiusScale::XLarge))
}

// ─────────────────────────────────────────────────────────────────────────────
// Spacing + type scale: layout governance pass. Unlike the radius scale these
// are skin-INDEPENDENT constants (light/dark skins share spacing and type),
// so call sites read `tokens::SPACE_8` / `tokens::FONT_13` instead of
// hardcoded `px(8.0)` / `px(13.0)` that silently drift from the scale.
// Spacing values are the scale actually used by the views; sizes (column
// widths, chart dimensions) are layout contracts, not tokens.

macro_rules! spacing_consts {
    ($($name:ident => $value:expr;)*) => {
        $(
            /// Spacing scale token (`px` value, skin-independent).
            pub const $name: Length = Length($value);
        )*
    };
}

spacing_consts! {
    SPACE_1 => 1.0;
    SPACE_2 => 2.0;
    SPACE_3 => 3.0;
    SPACE_4 => 4.0;
    SPACE_5 => 5.0;
    SPACE_6 => 6.0;
    SPACE_7 => 7.0;
    SPACE_8 => 8.0;
    SPACE_9 => 9.0;
    SPACE_10 => 10.0;
    SPACE_12 => 12.0;
    SPACE_14 => 14.0;
    SPACE_16 => 16.0;
    SPACE_24 => 24.0;
}

macro_rules! font_consts {
    ($($name:ident => $value:expr;)*) => {
        $(
            /// Type scale token (`px` size, skin-independent).
            pub const $name: FontSize = FontSize($value);
        )*
    };
}

font_consts! {
    FONT_8 => 8.0;
    FONT_9 => 9.0;
    FONT_10 => 10.0;
    FONT_11 => 11.0;
    FONT_12 => 12.0;
    FONT_13 => 13.0;
    FONT_14 => 14.0;
    FONT_15 => 15.0;
    FONT_16 => 16.0;
    FONT_18 => 18.0;
    FONT_20 => 20.0;
    FONT_26 => 26.0;
}

// ─────────────────────────────────────────────────────────────────────────────
// Semantic type roles + line-height: a thin semantic layer over the raw FONT_*
// scale so call sites read `tokens::FONT_BODY` / `FONT_HEADER` / `FONT_CAPTION`
// (what the text IS) instead of a raw size that silently drifts. Roles match the
// Win11 Task Manager / Mission Center convention: the column header is DELIBERATELY
// one step smaller than the body (Win11 Caption 12 Semibold over Body 14 Regular;
// Adwaita caption-heading 9pt Bold over body 11pt) — hierarchy comes from weight,
// not from enlarging the header.
//
// Line-height tokens make row height deterministic: `.text_size()` sets only
// font-size, so without an explicit line-height the table falls back to intrinsic
// leading. Like the size scale, these are skin-independent. The values are
// [`Ratio`]s (relative factors), mapped to each toolkit's relative line-height.

/// Body text — process names, data cells (Win11 TM Body 14 / Mission Center ~14.7px).
pub const FONT_BODY: FontSize = FontSize(14.0);
/// Column-header text — deliberately one step below body; hierarchy via
/// `Weight(600.0)` (Win11 Caption 12 Semibold / Adwaita caption-heading).
pub const FONT_HEADER: FontSize = FontSize(12.0);
/// Caption tier — badges, the Trend micro-label (one step below body).
pub const FONT_CAPTION: FontSize = FontSize(10.0);

/// Body-row line-height — Fluent Body ratio ~1.4 (14px → ~20px line box).
pub const LINE_HEIGHT_NORMAL: Ratio = Ratio(1.4);
/// Header-row line-height — Fluent Caption ratio ~1.33 (12px → 16px line box).
pub const LINE_HEIGHT_HEADER: Ratio = Ratio(1.33);

// -----------------------------------------------------------------------------
// User-facing UI size. This axis is intentionally independent from
// [`RowDensity`]: size owns readable type/icon/control metrics, while density
// owns how much vertical whitespace a data table spends between rows. Keeping
// the axes separate prevents a compact table from silently becoming a
// small-print table.

/// Product-wide desktop UI size. `Standard` is the readability-first default;
/// `Small` preserves the former dense desktop scale and `Large` provides an
/// explicit accessibility step without relying on platform DPI overrides.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum UiSize {
    Small,
    #[default]
    Standard,
    Large,
}

impl UiSize {
    pub const ALL: [Self; 3] = [Self::Small, Self::Standard, Self::Large];

    /// Stable token stored in the toolkit-neutral configuration payload.
    #[must_use]
    pub const fn config_token(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Standard => "Standard",
            Self::Large => "Large",
        }
    }

    /// Resolve a persisted token. Empty legacy input and unknown future input
    /// both fail closed to the readability-first Standard profile.
    #[must_use]
    pub fn from_config_token(token: &str) -> Self {
        match token.trim() {
            "Small" => Self::Small,
            "Large" => Self::Large,
            "Standard" | "" => Self::Standard,
            _ => Self::Standard,
        }
    }

    /// Primary reading size for page copy, table values, and row identities.
    pub const fn body_font_size(self) -> Length {
        match self {
            Self::Small => Length(14.0),
            Self::Standard => Length(16.0),
            Self::Large => Length(18.0),
        }
    }

    /// Column headers and secondary control labels.
    pub const fn header_font_size(self) -> Length {
        match self {
            Self::Small => Length(12.0),
            Self::Standard => Length(14.0),
            Self::Large => Length(16.0),
        }
    }

    /// Smallest supported explanatory/caption tier. Standard UI never drops
    /// below 12 logical pixels, including Chinese text.
    pub const fn caption_font_size(self) -> Length {
        match self {
            Self::Small => Length(11.0),
            Self::Standard => Length(12.0),
            Self::Large => Length(14.0),
        }
    }

    /// Section heading used inside cards and sidebars.
    pub const fn section_title_font_size(self) -> Length {
        match self {
            Self::Small => Length(18.0),
            Self::Standard => Length(20.0),
            Self::Large => Length(24.0),
        }
    }

    /// Page-level title size (Applications count, Performance device title).
    pub const fn page_title_font_size(self) -> Length {
        match self {
            Self::Small => Length(24.0),
            Self::Standard => Length(28.0),
            Self::Large => Length(32.0),
        }
    }

    /// Standard inline icon extent paired with body/control copy.
    pub const fn icon_size(self) -> Length {
        match self {
            Self::Small => Length(14.0),
            Self::Standard => Length(18.0),
            Self::Large => Length(22.0),
        }
    }

    /// Minimum control height for buttons, navigation tabs, and inputs.
    pub const fn control_height(self) -> Length {
        match self {
            Self::Small => Length(32.0),
            Self::Standard => Length(38.0),
            Self::Large => Length(46.0),
        }
    }

    /// Application zoom factor used only by renderers with a native program
    /// scale hook. The platform compositor's DPI factor remains independent
    /// and multiplies this user preference at the toolkit boundary.
    pub const fn renderer_scale(self) -> f32 {
        match self {
            // Iced's former unscaled geometry is the Small baseline. The
            // other factors track the 14 → 16 → 18 body-type ladder.
            Self::Small => 1.0,
            Self::Standard => 16.0 / 14.0,
            Self::Large => 18.0 / 14.0,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Type weights. The app's faces are variable fonts (Roboto Mono / MiSans
// VF), so weights are fractional. Body text runs slightly heavier than the
// classic 400 — 450 sits between Regular and Medium, crisper on LCDs while
// staying readable for dense data tables; headers use the semibold 600
// convention (Win11 Caption Semibold / Adwaita caption-heading Bold) and the
// strongest emphasis (section titles, the Trend header) uses 700.

/// Standard regular text weight — 400.
pub const FONT_WEIGHT_NORMAL: Weight = Weight(400.0);
/// Body text weight — 450 (between Regular and Medium), the app's default.
pub const FONT_WEIGHT_BODY: Weight = Weight(450.0);
/// Medium text weight — 500 (badges, button labels, medium-prominence indicators).
pub const FONT_WEIGHT_MEDIUM: Weight = Weight(500.0);
/// Column-header / section-header weight — 600 (semibold).
pub const FONT_WEIGHT_HEADER: Weight = Weight(600.0);
/// Semibold weight alias for [`FONT_WEIGHT_HEADER`].
pub const FONT_WEIGHT_SEMIBOLD: Weight = Weight(600.0);
/// Strong emphasis (Trend header, bold labels) — 700.
pub const FONT_WEIGHT_STRONG: Weight = Weight(700.0);
/// Bold weight alias for [`FONT_WEIGHT_STRONG`].
pub const FONT_WEIGHT_BOLD: Weight = Weight(700.0);
/// Extra bold emphasis (large hero metrics, prominent summary numbers) — 800.
pub const FONT_WEIGHT_EXTRA_BOLD: Weight = Weight(800.0);

// ─────────────────────────────────────────────────────────────────────────────
// Motion. Duration constants follow the Fluent motion scale (Fast 80ms /
// Normal 200ms class). The app's motion policy is deliberately restrained:
// animations play for APPEAR transitions (tooltip, modal, panel fade-ins)
// where a one-shot 0→1 sweep is stable; hover/selection STATE changes on
// virtualized rows and 10k-item lists stay instant (per-row animations would
// fight the uniform-list recycling and cost frames on every tick). The gpui
// `Animation` builders wrapping these durations live behind the theme's
// optional `gpui` feature (`gpui::motion_animation`, ADR-026).

/// Fast micro-interaction duration — 80ms (Fluent Fast).
pub const DURATION_FAST: Duration = Duration::from_millis(80);
/// Hover/state-transition class duration — 120ms.
pub const DURATION_HOVER: Duration = Duration::from_millis(120);
/// Panel/appear transition duration — 180ms (Fluent Normal class).
pub const DURATION_MEDIUM: Duration = Duration::from_millis(180);

/// Accessibility-aware policy for one-shot visual transitions.
///
/// `Normal` preserves the semantic token passed by the caller. `Reduced`
/// keeps a short opacity/color transition but caps longer effects at the
/// existing fast token. `NoMotion` returns no animation duration, so a
/// renderer can paint the final state without constructing a zero-duration
/// animation. Layout and list-item movement are outside this policy: shared
/// helpers only animate opacity or colors, and callers must keep ids stable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MotionPolicy {
    /// Use the complete 80/120/180ms semantic scale.
    #[default]
    Normal,
    /// Keep a brief transition, capped at the existing 80ms fast token.
    Reduced,
    /// Skip animation and apply the final visual state immediately.
    NoMotion,
}

impl MotionPolicy {
    /// All policies in their stable semantic order.
    pub const ALL: [Self; 3] = [Self::Normal, Self::Reduced, Self::NoMotion];

    /// Resolve a token duration for an animation-capable renderer.
    ///
    /// `None` means the caller must apply the final state directly. A zero
    /// duration is also rejected so GPUI's explicit animation primitive never
    /// receives a value that would make its progress calculation divide by
    /// zero.
    pub fn animation_duration(self, duration: Duration) -> Option<Duration> {
        if duration == Duration::ZERO {
            return None;
        }

        match self {
            Self::Normal => Some(duration),
            Self::Reduced => Some(if duration > DURATION_FAST {
                DURATION_FAST
            } else {
                duration
            }),
            Self::NoMotion => None,
        }
    }

    /// Whether this policy permits an explicit animation wrapper.
    pub const fn allows_animation(self) -> bool {
        match self {
            Self::NoMotion => false,
            Self::Normal | Self::Reduced => true,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/theme_tokens.rs"]
mod density_tests;
// ─────────────────────────────────────────────────────────────────────────────
// Row density axis (Win11 Task Manager "density" / Mission Center row-size
// parity). A table-wide preference: `Comfortable` is the app's standard row
// geometry (current look); `Compact` tightens vertical padding and the body
// line-height so the same data fits more rows per viewport. Density is a
// ROW metric source — views read `row_padding_y()` / `line_height()` instead
// of hardcoding paddings, and the header mirrors the body so rows stay
// pixel-aligned with the header in both densities.

/// Table row density. Default = [`RowDensity::Comfortable`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RowDensity {
    #[default]
    Comfortable,
    Compact,
}

impl RowDensity {
    pub const ALL: [RowDensity; 2] = [RowDensity::Comfortable, RowDensity::Compact];

    /// Body-row vertical padding for this density.
    pub const fn row_padding_y(self) -> Length {
        match self {
            RowDensity::Comfortable => Length(6.0),
            RowDensity::Compact => Length(2.0),
        }
    }

    /// Header-row vertical padding for this density (the header mirrors the
    /// body so header and rows stay pixel-aligned in both densities).
    pub const fn header_padding_y(self) -> Length {
        match self {
            RowDensity::Comfortable => Length(6.0),
            RowDensity::Compact => Length(3.0),
        }
    }

    /// Body-row line-height for this density (compact tightens the leading).
    pub const fn line_height(self) -> Ratio {
        match self {
            RowDensity::Comfortable => LINE_HEIGHT_NORMAL,
            RowDensity::Compact => Ratio(1.25),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Selection chrome. The selected-row identity comes from an accent RAIL on
// the row's leading edge (Win11 TM / Mission Center parity) plus the gentle
// [`Theme::selection_bg`] tint — not from a full-strength wash.

/// Width of the selected-row accent rail in the leading edge of the row.
pub const SELECTION_RAIL: Length = Length(4.0);
