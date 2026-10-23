// test-intent: behavior
//! The severity palette is the single authority for severity ink across every
//! frontend (BEVY-174 convergence). These assertions pin the palette's
//! contract against every skin's own status tokens, so a frontend can resolve
//! it and never assemble its own severity table.

use crate::Theme;
use crate::fonts::ResolvedFonts;
use crate::palette::{SeverityLevel, severity_palette};
use crate::theme::{HighContrast, LightDark, Skin};

/// Every skin, in both modes and both contrast settings.
fn variants() -> impl Iterator<Item = Theme> {
    Skin::ALL.into_iter().flat_map(|skin| {
        [LightDark::Light, LightDark::Dark]
            .into_iter()
            .flat_map(move |mode| {
                [HighContrast::Off, HighContrast::On]
                    .into_iter()
                    .map(move |hc| Theme::build(skin, mode, hc, ResolvedFonts::system_for(skin)))
            })
    })
}

/// The palette resolves each level from the theme's own status tokens.
#[test]
fn every_severity_level_resolves_from_the_theme_status_tokens() {
    for theme in variants() {
        let severity = severity_palette(&theme);

        assert_eq!(
            severity.critical(),
            theme.danger,
            "critical reads the theme's danger token"
        );
        assert_eq!(
            severity.warning(),
            theme.warning,
            "warning reads the theme's warning token"
        );
        assert_eq!(
            severity.notice(),
            theme.accent,
            "notice reads the theme's accent token"
        );
    }
}

/// The warning level must not borrow the GPU accent: a warning painted in the
/// success hue is the fork this palette exists to remove.
#[test]
fn the_warning_level_is_not_the_success_hue() {
    for theme in variants() {
        assert_ne!(
            severity_palette(&theme).warning(),
            theme.gpu,
            "a warning never reads the GPU/success accent"
        );
    }
}

/// A level resolves to the ink its own name promises, and the three levels
/// stay distinguishable within one theme.
#[test]
fn the_three_levels_stay_distinguishable() {
    for theme in variants() {
        let severity = severity_palette(&theme);

        assert_ne!(severity.critical(), severity.warning());
        assert_ne!(severity.warning(), severity.notice());
        assert_ne!(severity.critical(), severity.notice());
    }
}

/// The palette is reachable from the theme snapshot a frontend already
/// holds, so a frontend never re-derives the authority.
#[test]
fn the_theme_snapshot_carries_the_severity_authority() {
    for theme in variants() {
        assert_eq!(
            theme.severity_palette(),
            severity_palette(&theme),
            "the theme hands out the same severity authority"
        );
    }
}

/// The level vocabulary is closed: a level the palette does not know cannot
/// be invented by a frontend, and every level paints the ink its own name
/// promises.
#[test]
fn the_severity_vocabulary_is_closed() {
    assert_eq!(SeverityLevel::ALL.len(), 3);
    assert_eq!(SeverityLevel::ALL[0], SeverityLevel::Critical);
    assert_eq!(SeverityLevel::ALL[1], SeverityLevel::Warning);
    assert_eq!(SeverityLevel::ALL[2], SeverityLevel::Notice);

    for theme in variants() {
        let severity = severity_palette(&theme);
        for level in SeverityLevel::ALL {
            let expected = match level {
                SeverityLevel::Critical => severity.critical(),
                SeverityLevel::Warning => severity.warning(),
                SeverityLevel::Notice => severity.notice(),
            };
            assert_eq!(
                level.ink(severity),
                expected,
                "a level paints exactly the ink the palette resolves for it"
            );
        }
    }
}
