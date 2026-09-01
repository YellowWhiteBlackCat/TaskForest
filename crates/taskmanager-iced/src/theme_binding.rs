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
/// 2. **Ladder quantization, ties toward the denser step** — iced's
///    `font::Weight` is an enum (100..900 in 100s) and cannot express the
///    fractional weights the neutral tokens author (body text is 450). A
///    tie (450 between 400 and 500) rounds UP because the token ladder's
///    fractional weights intend extra density; rounding down would visibly
///    thin against the reference render on every platform.
#[must_use]
pub fn font_weight(weight: Weight) -> iced::font::Weight {
    font_weight_over(weight, WeightCompensationAxis::target())
}

/// [`font_weight`] with an explicit platform axis — the decision-table seam
/// that lets every host test both platform columns of the paired matrix.
#[must_use]
pub fn font_weight_over(weight: Weight, axis: WeightCompensationAxis) -> iced::font::Weight {
    let compensated = effective_weight(weight, axis).0;
    // LADDER ascends and `<=` keeps the LATER step on a tie, so a tie
    // resolves toward the denser weight.
    const LADDER: [(f32, iced::font::Weight); 9] = [
        (100.0, iced::font::Weight::Thin),
        (200.0, iced::font::Weight::ExtraLight),
        (300.0, iced::font::Weight::Light),
        (400.0, iced::font::Weight::Normal),
        (500.0, iced::font::Weight::Medium),
        (600.0, iced::font::Weight::Semibold),
        (700.0, iced::font::Weight::Bold),
        (800.0, iced::font::Weight::ExtraBold),
        (900.0, iced::font::Weight::Black),
    ];
    let mut best = LADDER[0];
    for step in LADDER {
        if (step.0 - compensated).abs() <= (best.0 - compensated).abs() {
            best = step;
        }
    }
    best.1
}

#[cfg(test)]
#[path = "../tests/headless/theme_binding.rs"]
mod tests;
