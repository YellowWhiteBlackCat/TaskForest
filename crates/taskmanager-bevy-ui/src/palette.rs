//! Theme-token → bevy adapter (charter boundary 2).
//!
//! Pure mapping from the toolkit-neutral [`taskmanager_theme`] tokens onto
//! bevy colors and type metrics. It lives in this crate and only this crate:
//! the theme never grows a `bevy` feature, and every product color this
//! frontend paints originates in a theme token, never in a literal. All
//! sizing comes from the theme's typed spacing/type/weight scales.
//!
//! bevy values appear only at this adapter's output; they stay inside the
//! crate and out of the crate's public API. Product component semantics
//! (nav rail, tables, dialogs) consume [`UiPalette`] — never the official
//! Feathers skin system, which this frontend does not adopt.

use bevy::color::Color;
use bevy::text::{FontSize, FontWeight, TextFont, TextLayout};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::{self, UiSize};

/// Strictly single-line text layout for bounded rows: a value wider than its
/// box clips at the edge, never wraps and never stretches its siblings. The
/// typography-discipline companion to the theme's type scale — bounded rows
/// compose this with `Overflow::clip_x()` so long facts degrade by clipping,
/// never by reflowing into a wrapped stack.
#[must_use]
pub(crate) fn no_wrap_text() -> TextLayout {
    TextLayout {
        linebreak: bevy::text::LineBreak::NoWrap,
        ..TextLayout::default()
    }
}

/// sRGB channel mapping: the theme's linear-alpha sRGB quadruple becomes a
/// bevy `Color::srgba` with identical channels. Exact by construction — no
/// clamping, premultiplication or gamma reinterpretation happens here, so a
/// round trip through [`Color::to_srgba`][bevy_color] recovers the token.
pub(crate) fn theme_color(color: taskmanager_theme::Color) -> Color {
    Color::srgba(color.r, color.g, color.b, color.a)
}

/// Weight token → bevy font weight. Variable-font weights live in 1..=1000;
/// the token scale never leaves that range, and the clamp is a guard for
/// future token edits, not an expected path.
fn theme_weight(weight: taskmanager_theme::Weight) -> FontWeight {
    FontWeight(weight.0.round().clamp(1.0, 1000.0) as u16)
}

/// The resolved window surface: every color, radius, and typographic role the
/// M0 app shell renders, derived from one [`Theme`] snapshot. Component
/// semantics read this — colors are never inlined at widget call sites.
///
/// The font *handle* is deliberately absent: handles are runtime state, not
/// theme tokens, and text styling is applied by the insert-observers in
/// [`crate::window`] which join this palette with the embedded-face resource.
#[derive(Clone, Debug)]
pub(crate) struct UiPalette {
    /// Window clear color — the theme's window backdrop token.
    pub(crate) window_clear: Color,
    /// Card/panel fill — the theme's derived elevated surface. Read by
    /// the menu/dialog panels; their call sites land with W4.
    #[allow(dead_code)]
    pub(crate) panel_fill: Color,
    /// Content-region backdrop — the theme's view surface.
    pub(crate) content_bg: Color,
    /// Left navigation rail backdrop — the theme's sidebar surface.
    pub(crate) nav_bg: Color,
    /// Active nav item fill — the theme's elevated sidebar card surface.
    pub(crate) nav_active_bg: Color,
    /// Hovered control surface — the theme's semantic accent tint.
    pub(crate) hover_bg: Color,
    /// Pressed control surface — the theme's semantic selection tint.
    pub(crate) selection_bg: Color,
    /// Text/icon ink used on the active navigation accent surface.
    pub(crate) nav_active_ink: Color,
    /// Modal scrim — the theme's dimming overlay token. Read by the
    /// dialog scrim; its call site lands with W4.
    #[allow(dead_code)]
    pub(crate) scrim: Color,
    /// Card corner radius in px, from the theme's radius scale.
    pub(crate) panel_radius_px: f32,
    /// Control (nav item, table row) corner radius in px.
    pub(crate) control_radius_px: f32,
    /// Standard control / table-row height in px, from the density scale.
    pub(crate) control_height_px: f32,
    /// Accent ink — the theme's accent token.
    pub(crate) accent: Color,
    /// Heading ink.
    pub(crate) heading_color: Color,
    /// Body ink.
    pub(crate) body_color: Color,
    /// Dimmed ink (captions, summary lines, idle nav labels).
    pub(crate) dim_color: Color,
    /// Page-title type metrics (size + weight; handle stamped later).
    pub(crate) heading: TextFont,
    /// Body type metrics.
    pub(crate) body: TextFont,
    /// Caption/nav-label type metrics.
    pub(crate) caption: TextFont,
    /// Monospace metrics for aligned telemetry values and diagnostics.
    pub(crate) mono: TextFont,
}

/// Resolve the window palette from a theme snapshot.
///
/// The M0 window deliberately takes the cold-start theme (`Theme::dark()`);
/// appearance detection and config restoration arrive with the first feature
/// page (M1), so the adapter stays the only place tokens become bevy values.
pub(crate) fn ui_palette(theme: &Theme) -> UiPalette {
    let standard = UiSize::Standard;
    UiPalette {
        window_clear: theme_color(theme.window_bg),
        panel_fill: theme_color(theme.card_surface()),
        content_bg: theme_color(theme.view_bg),
        nav_bg: theme_color(theme.sidebar_bg),
        nav_active_bg: theme_color(theme.sidebar_card_bg),
        hover_bg: theme_color(theme.hover_bg()),
        selection_bg: theme_color(theme.selection_bg()),
        nav_active_ink: theme_color(theme.accent_text),
        scrim: theme_color(theme.scrim),
        panel_radius_px: tokens::card_radius(theme).0,
        control_radius_px: tokens::control_radius(theme).0,
        control_height_px: standard.control_height().0,
        accent: theme_color(theme.accent),
        heading_color: theme_color(theme.fg),
        body_color: theme_color(theme.fg),
        dim_color: theme_color(theme.fg_dim),
        heading: TextFont {
            font_size: FontSize::Px(standard.page_title_font_size().0),
            weight: theme_weight(tokens::FONT_WEIGHT_HEADER),
            ..TextFont::default()
        },
        body: TextFont {
            font_size: FontSize::Px(f32::from(tokens::FONT_BODY)),
            weight: theme_weight(tokens::FONT_WEIGHT_NORMAL),
            ..TextFont::default()
        },
        caption: TextFont {
            font_size: FontSize::Px(f32::from(tokens::FONT_CAPTION)),
            weight: theme_weight(tokens::FONT_WEIGHT_NORMAL),
            ..TextFont::default()
        },
        mono: TextFont {
            font_size: FontSize::Px(f32::from(tokens::FONT_BODY)),
            weight: theme_weight(tokens::FONT_WEIGHT_NORMAL),
            ..TextFont::default()
        },
    }
}

/// Theme spacing accessors for bsn! field expressions. Thin named wrappers:
/// a bare token path inside a `bsn!` field (`Val::Px(tokens::SPACE_8.0)`)
/// re-parses as a nested scene patch and fails; a plain lowercase function
/// call is the macro's dynamic-expression form. The theme stays the only
/// source — no literal ever appears at a scene call site.
pub(crate) fn space_2() -> f32 {
    tokens::SPACE_2.0
}

pub(crate) fn space_4() -> f32 {
    tokens::SPACE_4.0
}

pub(crate) fn space_8() -> f32 {
    tokens::SPACE_8.0
}

pub(crate) fn space_12() -> f32 {
    tokens::SPACE_12.0
}

pub(crate) fn space_16() -> f32 {
    tokens::SPACE_16.0
}

pub(crate) fn space_24() -> f32 {
    tokens::SPACE_24.0
}

#[cfg(test)]
#[path = "../tests/headless/palette.rs"]
mod tests;
