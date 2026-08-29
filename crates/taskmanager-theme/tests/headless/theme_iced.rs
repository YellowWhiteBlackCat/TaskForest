//! Iced binding conversion tests (CORE-07) — value-level parity with the
//! single conversion source, including the paired weight-decision table.

use super::*;
use crate::color::Weight;
use crate::platform::WeightCompensationAxis;
use crate::tokens::{
    FONT_WEIGHT_BODY, FONT_WEIGHT_BOLD, FONT_WEIGHT_EXTRA_BOLD, FONT_WEIGHT_HEADER,
    FONT_WEIGHT_MEDIUM, FONT_WEIGHT_NORMAL,
};
use crate::{Color, Theme};

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
#[test]
fn font_weight_decision_table_over_both_axes() {
    for axis in WeightCompensationAxis::ALL {
        // FreeType keeps the authored 450 and the ladder tie (450 between
        // 400 and 500) resolves toward the DENSER step by documented
        // policy; DirectWrite compensates 450→500 first and the ladder
        // then maps it to the same Medium step the gpui binding renders.
        let expected = [
            (FONT_WEIGHT_NORMAL, iced_core::font::Weight::Normal),
            (FONT_WEIGHT_BODY, iced_core::font::Weight::Medium),
            (FONT_WEIGHT_MEDIUM, iced_core::font::Weight::Medium),
            (FONT_WEIGHT_HEADER, iced_core::font::Weight::Semibold),
            (FONT_WEIGHT_BOLD, iced_core::font::Weight::Bold),
            (FONT_WEIGHT_EXTRA_BOLD, iced_core::font::Weight::ExtraBold),
        ];
        for (weight, step) in expected {
            assert_eq!(
                font_weight_over(weight, axis),
                step,
                "axis {axis:?}, weight {weight:?}"
            );
        }
    }
}

/// Ties on the ladder resolve toward the denser step (450→Medium, 550→
/// Semibold) — the documented quantization policy, distinct on purpose from
/// round-half-down.
#[test]
fn ladder_ties_resolve_toward_the_denser_step() {
    assert_eq!(
        font_weight_over(Weight(450.0), WeightCompensationAxis::FreeType),
        iced_core::font::Weight::Medium
    );
    assert_eq!(
        font_weight_over(Weight(550.0), WeightCompensationAxis::FreeType),
        iced_core::font::Weight::Semibold
    );
    assert_eq!(
        font_weight_over(Weight(425.0), WeightCompensationAxis::FreeType),
        iced_core::font::Weight::Normal,
        "below the tie the NEARER (lighter) step wins normally"
    );
}

/// The font helpers read the SAME resolved family fields the gpui binding
/// reads — one neutral source, two projections.
#[test]
fn fonts_read_the_resolved_neutral_families() {
    let theme = Theme::default();
    assert_eq!(ui_font(&theme), iced_core::Font::with_name(theme.ui_font));
    assert_eq!(
        mono_font(&theme),
        iced_core::Font::with_name(theme.mono_font)
    );
    let weighted = ui_font_weight(&theme, FONT_WEIGHT_HEADER);
    assert_eq!(
        weighted.family,
        iced_core::font::Family::Name(theme.ui_font)
    );
    assert_eq!(weighted.weight, iced_core::font::Weight::Semibold);
    assert_eq!(BUNDLED_UI_FONT, iced_core::Font::with_name(FONT_MISANS_VF));
}
