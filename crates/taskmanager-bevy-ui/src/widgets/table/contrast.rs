//! Ink contrast calibration for the table's dimmed values.
//!
//! A dimmed zero is a muted value, not an invisible one: the muted ink still
//! has to be READABLE against the surface it sits on. The theme's dim token
//! is tuned for captions on chrome, not for a value inside a data row, so the
//! table calibrates it: the muted ink is mixed toward the body ink until the
//! pair clears the WCAG AA contrast ratio against every surface the row can
//! sit on.
//!
//! The calibration is a pure function of theme colors — no window, no
//! screenshot — so the ratio it guarantees is the ratio the pixels have.

use bevy::color::{Color, Mix};

/// The WCAG AA contrast ratio for normal-size text.
pub(crate) const AA_CONTRAST_RATIO: f32 = 4.5;

/// The relative luminance of a color (WCAG 2.x).
#[must_use]
pub(crate) fn relative_luminance(color: Color) -> f32 {
    let srgba = color.to_srgba();
    let channel = |value: f32| {
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(srgba.red) + 0.7152 * channel(srgba.green) + 0.0722 * channel(srgba.blue)
}

/// The contrast ratio between two colors (WCAG 2.x): 1.0 for identical
/// luminances, 21.0 for black on white.
#[must_use]
pub(crate) fn contrast_ratio(foreground: Color, background: Color) -> f32 {
    let first = relative_luminance(foreground);
    let second = relative_luminance(background);
    let lighter = first.max(second);
    let darker = first.min(second);
    (lighter + 0.05) / (darker + 0.05)
}

/// The worst contrast ratio of `ink` against the given surfaces.
#[must_use]
pub(crate) fn worst_contrast_ratio(ink: Color, surfaces: &[Color]) -> f32 {
    surfaces
        .iter()
        .map(|surface| contrast_ratio(ink, *surface))
        .fold(f32::INFINITY, f32::min)
}

/// Calibrate the muted ink so it stays legible on every surface the value can
/// sit on.
///
/// The muted ink is mixed toward the body ink in the smallest steps that
/// clear [`AA_CONTRAST_RATIO`]; when even the body ink cannot clear it (an
/// unreadable surface), the body ink is returned — the honest best effort
/// instead of a value that pretends to be dimmed.
#[must_use]
pub(crate) fn calibrate_muted_ink(muted: Color, body: Color, surfaces: &[Color]) -> Color {
    if surfaces.is_empty() {
        return muted;
    }
    if worst_contrast_ratio(muted, surfaces) >= AA_CONTRAST_RATIO {
        return muted;
    }
    for step in 1..=16 {
        let candidate = muted.mix(&body, step as f32 / 16.0);
        if worst_contrast_ratio(candidate, surfaces) >= AA_CONTRAST_RATIO {
            return candidate;
        }
    }
    body
}
