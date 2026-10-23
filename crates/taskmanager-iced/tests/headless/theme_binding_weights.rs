//! Iced binding conversion tests (CORE-07) — value-level parity with the
//! single conversion source, including the paired weight-decision table and
//! the weight-mapping precision audit (ICED-052).
//!
//! This file replaces the earlier `theme_binding.rs` test module, whose
//! weight expectations documented the OLD quantization policy (a ladder tie
//! resolving toward the denser step). The policy changed: a tie now resolves
//! toward the LIGHTER step, so the neutral `BODY` weight (450) renders on
//! the Normal step and stops collapsing onto the `MEDIUM` role (500).

use super::*;
use taskmanager_theme::color::Weight;
use taskmanager_theme::platform::WeightCompensationAxis;
use taskmanager_theme::tokens::{
    FONT_WEIGHT_BODY, FONT_WEIGHT_BOLD, FONT_WEIGHT_EXTRA_BOLD, FONT_WEIGHT_HEADER,
    FONT_WEIGHT_MEDIUM, FONT_WEIGHT_NORMAL,
};
use taskmanager_theme::{Color, Theme};

fn token() -> Color {
    Color::from_hex(0x3584e4)
}

/// The neutral channel/alpha values survive the iced conversion unchanged.
#[test]
fn color_maps_channels_and_alpha() {
    let converted = color(token());
    assert_eq!(converted.r, 0x35 as f32 / 255.0);
    assert_eq!(converted.g, 0x84 as f32 / 255.0);
    assert_eq!(converted.b, 0xe4 as f32 / 255.0);
    assert_eq!(converted.a, 1.0);

    let translucent = color(Color::from_hex(0x222226).with_alpha(0.55));
    assert_eq!(translucent.a, 0.55);
}

/// The PAIRED weight decision table (CORE-07): both platform columns of the
/// (compensation × quantization) matrix, runnable on every host.
///
/// The neutral ladder keeps `BODY` (450) and `MEDIUM` (500) as two
/// DISTINCT grayscale roles: a ladder tie resolves toward the lighter step,
/// so on the FreeType column `BODY` renders on Normal and `MEDIUM` renders
/// on Medium. Under the old denser-step tie both roles collapsed onto
/// Medium and body text read as heavy as the Medium emphasis.
///
/// The DirectWrite column still lands `BODY` on Medium: the shared platform
/// compensation snaps the fractional body weight to 500 BEFORE the ladder
/// runs (a decision owned by `taskmanager_theme::platform`, not by this
/// binding), so there is no tie left to resolve there.
#[test]
fn font_weight_decision_table_over_both_axes() {
    let freetype = [
        (FONT_WEIGHT_NORMAL, iced::font::Weight::Normal),
        (FONT_WEIGHT_BODY, iced::font::Weight::Normal),
        (FONT_WEIGHT_MEDIUM, iced::font::Weight::Medium),
        (FONT_WEIGHT_HEADER, iced::font::Weight::Semibold),
        (FONT_WEIGHT_BOLD, iced::font::Weight::Bold),
        (FONT_WEIGHT_EXTRA_BOLD, iced::font::Weight::ExtraBold),
    ];
    let direct_write = [
        (FONT_WEIGHT_NORMAL, iced::font::Weight::Normal),
        (FONT_WEIGHT_BODY, iced::font::Weight::Medium),
        (FONT_WEIGHT_MEDIUM, iced::font::Weight::Medium),
        (FONT_WEIGHT_HEADER, iced::font::Weight::Semibold),
        (FONT_WEIGHT_BOLD, iced::font::Weight::Bold),
        (FONT_WEIGHT_EXTRA_BOLD, iced::font::Weight::ExtraBold),
    ];
    for (weight, step) in freetype {
        assert_eq!(
            font_weight_over(weight, WeightCompensationAxis::FreeType),
            step,
            "freetype column, weight {weight:?}"
        );
    }
    for (weight, step) in direct_write {
        assert_eq!(
            font_weight_over(weight, WeightCompensationAxis::DirectWrite),
            step,
            "directwrite column, weight {weight:?}"
        );
    }
}

/// The neutral body weight and the medium weight must land on DIFFERENT
/// ladder steps on the column that renders the authored fractional weight:
/// that separation is the Normal/Medium grayscale the acceptance asks for,
/// and the old denser-step tie destroyed it.
#[test]
fn body_and_medium_weights_stay_on_distinct_ladder_steps() {
    let body = font_weight_over(FONT_WEIGHT_BODY, WeightCompensationAxis::FreeType);
    let medium = font_weight_over(FONT_WEIGHT_MEDIUM, WeightCompensationAxis::FreeType);
    assert_ne!(
        body, medium,
        "the body weight must not collapse onto the medium role"
    );
    assert_eq!(body, iced::font::Weight::Normal);
    assert_eq!(medium, iced::font::Weight::Medium);
}

/// Ladder ties resolve toward the LIGHTER step (450→Normal, 550→Medium):
/// the documented quantization policy, distinct on purpose from
/// round-half-up.
#[test]
fn ladder_ties_resolve_toward_the_lighter_step() {
    assert_eq!(
        font_weight_over(Weight(450.0), WeightCompensationAxis::FreeType),
        iced::font::Weight::Normal
    );
    assert_eq!(
        font_weight_over(Weight(550.0), WeightCompensationAxis::FreeType),
        iced::font::Weight::Medium
    );
    assert_eq!(
        font_weight_over(Weight(425.0), WeightCompensationAxis::FreeType),
        iced::font::Weight::Normal,
        "below the tie the NEARER (lighter) step wins normally"
    );
    assert_eq!(
        font_weight_over(Weight(475.0), WeightCompensationAxis::FreeType),
        iced::font::Weight::Medium,
        "above the tie the NEARER (denser) step wins normally"
    );
}

/// The mapping audit reports the numbers the grayscale acceptance compares:
/// the authored weight, the platform-compensated weight, the rendered
/// ladder step and its numeric `wght` value, and the signed quantization
/// error between them. The error is never larger than half a ladder step,
/// so the render can never drift more than 50 units from the authored
/// weight.
#[test]
fn weight_mapping_audit_reports_the_rendered_numeric_weight() {
    for axis in WeightCompensationAxis::ALL {
        for weight in [
            FONT_WEIGHT_NORMAL,
            FONT_WEIGHT_BODY,
            FONT_WEIGHT_MEDIUM,
            FONT_WEIGHT_HEADER,
            FONT_WEIGHT_BOLD,
            FONT_WEIGHT_EXTRA_BOLD,
        ] {
            let mapping = font_weight_mapping(weight, axis);
            assert_eq!(mapping.requested, weight.0 as u16);
            assert_eq!(
                mapping.compensated,
                effective_weight(weight, axis).0 as u16,
                "axis {axis:?}: the compensation is the shared platform decision"
            );
            assert_eq!(
                mapping.rendered,
                font_weight_value(weight, axis),
                "the numeric value is the one the step renders"
            );
            assert_eq!(
                mapping.error,
                i32::from(mapping.rendered) - i32::from(mapping.compensated)
            );
            assert!(
                mapping.error.abs() <= 50,
                "axis {axis:?}, weight {weight:?}: the quantization error must stay \
                 inside half a ladder step, got {}",
                mapping.error
            );
        }
    }
}

/// The FreeType column keeps the authored fractional weight visible in the
/// audit (the rasterizer axis that renders it), while the DirectWrite
/// column reports the shared 450→500 compensation — the two columns of the
/// paired matrix stay distinguishable instead of collapsing into one.
#[test]
fn mapping_audit_separates_the_two_platform_columns() {
    let free_type = font_weight_mapping(FONT_WEIGHT_BODY, WeightCompensationAxis::FreeType);
    assert_eq!(free_type.compensated, 450);
    assert_eq!(free_type.rendered, 400);
    assert_eq!(free_type.error, -50);

    let direct_write = font_weight_mapping(FONT_WEIGHT_BODY, WeightCompensationAxis::DirectWrite);
    assert_eq!(
        direct_write.compensated, 500,
        "the shared DirectWrite compensation snaps the body weight to Medium"
    );
    assert_eq!(direct_write.rendered, 500);
    assert_eq!(direct_write.error, 0);
}

/// The font helpers read the SAME resolved family fields the gpui binding
/// reads — one neutral source, two projections.
#[test]
fn fonts_read_the_resolved_neutral_families() {
    let theme = Theme::default();
    assert_eq!(ui_font(&theme), iced::Font::with_name(theme.ui_font));
    assert_eq!(mono_font(&theme), iced::Font::with_name(theme.mono_font));
    let weighted = ui_font_weight(&theme, FONT_WEIGHT_HEADER);
    assert_eq!(weighted.family, iced::font::Family::Name(theme.ui_font));
    assert_eq!(weighted.weight, iced::font::Weight::Semibold);
    assert_eq!(BUNDLED_UI_FONT, iced::Font::with_name(FONT_MISANS_VF));
}
