//! The single quarantined module where the neutral theme meets iced
//! (ADR-026/CORE-07, relocated by ADR-051).
//!
//! The binding surface is FREE FUNCTIONS so this module owns the whole
//! token→value edge; the iced frontend consumes THIS module as its only
//! conversion source — its `theme.rs` keeps only style ASSEMBLY (container/
//! button styles over converted values), which is frontend policy per
//! ADR-026 rule 3.
//!
//! Platform compensation is shared with the gpui binding through
//! `taskmanager_theme::platform`: the two bindings may differ only through
//! named, paired decisions there, never through one-sided cfg logic
//! (CORE-07).

use taskmanager_theme::color::{Color, Weight};
use taskmanager_theme::fonts::FONT_MISANS_VF;
use taskmanager_theme::platform::{WeightCompensationAxis, effective_weight};
use taskmanager_theme::theme::Theme;

/// Map one neutral sRGB token onto an iced color (alpha preserved — iced
/// paints with alpha).
#[must_use]
pub fn color(c: Color) -> iced::Color {
    iced::Color::from_rgba(c.r, c.g, c.b, c.a)
}

/// The bundled UI face as an iced font. The composition edge registers the
/// same bytes GPUI embeds; used as the application builder's
/// `default_font` so unstyled `text()` renders the same face everywhere.
pub const BUNDLED_UI_FONT: iced::Font = iced::Font::with_name(FONT_MISANS_VF);

/// The iced font for the UI role, read from the neutral theme's resolved
/// family (honors the persisted System/Bundled preference).
#[must_use]
pub fn ui_font(theme: &Theme) -> iced::Font {
    iced::Font::with_name(theme.ui_font)
}

/// The iced font for the monospace role, read from the neutral theme's
/// resolved family (honors the persisted mono-font preference).
#[must_use]
pub fn mono_font(theme: &Theme) -> iced::Font {
    iced::Font::with_name(theme.mono_font)
}

/// The resolved UI family at one neutral [`Weight`] — the type-weight
/// hierarchy the reference shell renders for section titles, headings and
/// big readouts. Only the weight changes; the family stays resolved.
#[must_use]
pub fn ui_font_weight(theme: &Theme, weight: Weight) -> iced::Font {
    iced::Font {
        family: iced::font::Family::Name(theme.ui_font),
        weight: font_weight(weight),
        ..iced::Font::DEFAULT
    }
}

/// The iced weight-ladder step for one neutral weight on the compiling
/// target's platform axis.
///
/// Two PAIRED decisions, both deliberate (CORE-07):
///
/// 1. **Platform compensation** — the weight flows through
///    [`effective_weight`] first, exactly like the gpui binding, so the
///    DirectWrite 450→500 density compensation applies to both toolkits.
/// 2. **Ladder quantization, ties toward the LIGHTER step** — iced's
///    `font::Weight` is an enum (100..900 in 100s) and cannot express the
///    fractional weights the neutral tokens author (body text is 450). A
///    tie (450 between 400 and 500) resolves DOWNWARD, onto the Normal
///    step, because the neutral ladder pairs `BODY` (450) with `MEDIUM`
///    (500) as two DISTINCT grayscale roles: rounding the tie UP collapsed
///    both roles onto the same Medium step, so body text rendered as heavy
///    as the Medium emphasis (the "neutral weight reads too bold" report)
///    and the Normal/Medium grayscale distinction was lost. Keeping the
///    tie on the lighter step restores that distinction: `BODY` renders on
///    Normal, `MEDIUM` renders on Medium.
#[must_use]
pub fn font_weight(weight: Weight) -> iced::font::Weight {
    font_weight_over(weight, WeightCompensationAxis::target())
}

/// [`font_weight`] with an explicit platform axis — the decision-table seam
/// that lets every host test both platform columns of the paired matrix.
#[must_use]
pub fn font_weight_over(weight: Weight, axis: WeightCompensationAxis) -> iced::font::Weight {
    font_weight_mapping(weight, axis).step
}

/// One neutral weight's complete mapping decision: the authored value, the
/// platform-compensated value, the ladder step the frontend can actually
/// render, the numeric `wght` value that step applies, and the signed
/// quantization error between the compensated and the rendered value.
///
/// The seam exists so the weight mapping is AUDITABLE instead of implied:
/// the neutral tokens author fractional weights (450), iced's `font::Weight`
/// enum can only render 100-step values, and the difference between those
/// two numbers is exactly the "how much density did the render lose or
/// gain" question the Windows/Linux grayscale acceptance asks about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontWeightMapping {
    /// The authored neutral weight (the token value).
    pub requested: u16,
    /// The weight after the shared platform compensation.
    pub compensated: u16,
    /// The ladder step the frontend renders.
    pub step: iced::font::Weight,
    /// The numeric `wght` value the rendered step applies.
    pub rendered: u16,
    /// The signed distance between the compensated and the rendered value.
    pub error: i32,
}

/// The numeric `wght` value each ladder step applies, ascending with the
/// step's density. The numeric column is the one the rasterizer actually
/// applies to a variable font, so it is the value the mapping audit and the
/// capture metadata report.
const LADDER: [(u16, iced::font::Weight); 9] = [
    (100, iced::font::Weight::Thin),
    (200, iced::font::Weight::ExtraLight),
    (300, iced::font::Weight::Light),
    (400, iced::font::Weight::Normal),
    (500, iced::font::Weight::Medium),
    (600, iced::font::Weight::Semibold),
    (700, iced::font::Weight::Bold),
    (800, iced::font::Weight::ExtraBold),
    (900, iced::font::Weight::Black),
];

/// The full mapping decision for one neutral weight on one platform axis.
///
/// The compensation is the SHARED platform decision ([`effective_weight`]);
/// the quantization is iced's own, because iced's `font::Weight` is an enum
/// and cannot carry a fractional weight. A tie between two ladder steps
/// resolves toward the LIGHTER step (see [`font_weight`] for why).
#[must_use]
pub fn font_weight_mapping(weight: Weight, axis: WeightCompensationAxis) -> FontWeightMapping {
    let requested = weight.0.round().clamp(0.0, u16::MAX as f32) as u16;
    let compensated = effective_weight(weight, axis).0;
    let compensated = compensated.round().clamp(0.0, u16::MAX as f32) as u16;
    // LADDER ascends and the strict `<` comparison keeps the EARLIER (and so
    // the lighter) step on a tie.
    let mut best = LADDER[0];
    for step in LADDER {
        let distance = step.0.abs_diff(compensated);
        let best_distance = best.0.abs_diff(compensated);
        if distance < best_distance {
            best = step;
        }
    }
    FontWeightMapping {
        requested,
        compensated,
        step: best.1,
        rendered: best.0,
        error: i32::from(best.0) - i32::from(compensated),
    }
}

/// The numeric `wght` value the frontend renders for one neutral weight on
/// one platform axis — the value a variable font's weight axis receives.
#[must_use]
pub fn font_weight_value(weight: Weight, axis: WeightCompensationAxis) -> u16 {
    font_weight_mapping(weight, axis).rendered
}

#[cfg(test)]
#[path = "../tests/headless/theme_binding_weights.rs"]
mod tests;
