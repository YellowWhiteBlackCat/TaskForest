// test-intent: behavior
//
// Behavior tests for the primitives component family: the tone/token
// mapping seams (badge fills with luminance-picked foregrounds, the
// four-state panel grammar, the source → panel-state wrapper mapping) and
// the progress resolution seam that keeps unavailable data from
// masquerading as a measured value — plus a construction sweep across
// skins and modes that exercises the geometry/style paths.

use super::*;
use crate::ui::components::{
    banner_title_key, message_panel, source_panel_state, source_state_panel, titled_card,
};
use taskmanager_application::{RefreshRequest, SourceStateKind, merge_source_lines};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};

use taskmanager_theme::color::{contrast_ratio, on_accent};
use taskmanager_theme::{HighContrast, LightDark, Skin};

fn theme_for(skin: Skin, mode: LightDark) -> Theme {
    Theme::build(
        skin,
        mode,
        HighContrast::Off,
        taskmanager_theme::ResolvedFonts::system_for(skin),
    )
}

fn status(outcome: SourceOutcome) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed("test.primitives"),
        outcome,
        item_count: 0,
    }
}

/// Every badge tone fills from its palette semantic token; the neutral chip
/// stays quiet (muted foreground on the panel surface) while tinted chips
/// pick their foreground by fill luminance — and that pick is the WCAG
/// black/white choice, which never drops below the 4.5:1 text band over
/// any skin or mode.
#[test]
fn badge_tones_map_onto_palette_semantics_with_contrast_foregrounds() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let palette = theme.palette();
    assert_eq!(BadgeTone::Neutral.fill(&theme), palette.surface);
    assert_eq!(BadgeTone::Success.fill(&theme), palette.success);
    assert_eq!(BadgeTone::Warning.fill(&theme), palette.warning);
    assert_eq!(BadgeTone::Danger.fill(&theme), palette.danger);
    assert_eq!(BadgeTone::Accent.fill(&theme), palette.accent);
    assert_eq!(BadgeTone::Neutral.foreground(&theme), palette.fg_muted);

    let tinted = [
        BadgeTone::Success,
        BadgeTone::Warning,
        BadgeTone::Danger,
        BadgeTone::Accent,
    ];
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = theme_for(skin, mode);
            for tone in tinted {
                let fill = tone.fill(&theme);
                let foreground = tone.foreground(&theme);
                assert_eq!(foreground, on_accent(fill));
                assert!(
                    contrast_ratio(foreground, fill) >= 4.5,
                    "{skin:?}/{mode:?}/{tone:?}: {}",
                    contrast_ratio(foreground, fill)
                );
            }
        }
    }
}

/// Unavailable progress resolves to the Unknown grammar — never to a
/// measured value, and specifically never to a disguised 0% — while real
/// measurements (including a true measured zero) clamp into 0..=1.
#[test]
fn unavailable_progress_never_resolves_to_a_measured_value() {
    assert_eq!(progress_fill(None), ProgressFill::Unknown);
    assert_ne!(progress_fill(None), ProgressFill::Determinate(0.0));
    assert_eq!(progress_fill(Some(f32::NAN)), ProgressFill::Unknown);
    assert_eq!(progress_fill(Some(f32::INFINITY)), ProgressFill::Unknown);
    assert_eq!(
        progress_fill(Some(f32::NEG_INFINITY)),
        ProgressFill::Unknown
    );
    assert_eq!(progress_fill(Some(0.0)), ProgressFill::Determinate(0.0));
    assert_eq!(progress_fill(Some(0.42)), ProgressFill::Determinate(0.42));
    assert_eq!(progress_fill(Some(-3.0)), ProgressFill::Determinate(0.0));
    assert_eq!(progress_fill(Some(1.9)), ProgressFill::Determinate(1.0));
}

/// The four-state grammar owns a fixed tone mapping per state: quiet muted
/// for empty, failure for unanswered, caution for partial, success for
/// recovery — constant across every skin and mode.
#[test]
fn panel_states_map_onto_their_palette_tone_grammar() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = theme_for(skin, mode);
            let palette = theme.palette();
            assert_eq!(PanelState::Empty.tone(&theme), palette.fg_muted);
            assert_eq!(PanelState::Unavailable.tone(&theme), palette.danger);
            assert_eq!(PanelState::Partial.tone(&theme), palette.warning);
            assert_eq!(PanelState::Recovery.tone(&theme), palette.success);
        }
    }
}

/// The source wrapper keeps its original semantics at the new seam: a
/// degraded merged source (rows still usable) maps to Partial with the
/// partial copy, every unanswered source maps to Unavailable with the
/// unavailable copy, and the copy-key choice is unchanged.
#[test]
fn source_wrapper_maps_merged_kinds_onto_the_shared_panel_grammar() {
    let degraded = status(SourceOutcome::Partial(FailureKind::MissingDependency));
    let merged = merge_source_lines(&[degraded]).expect("degraded source merges");
    assert_eq!(merged.kind, SourceStateKind::Degraded);
    assert_eq!(source_panel_state(merged.kind), PanelState::Partial);
    assert_eq!(banner_title_key(merged.kind), "source.partial_title");

    let failed = status(SourceOutcome::Unavailable(FailureKind::PermissionDenied));
    let merged = merge_source_lines(&[failed]).expect("failed source merges");
    assert_eq!(merged.kind, SourceStateKind::Failed);
    assert_eq!(source_panel_state(merged.kind), PanelState::Unavailable);
    assert_eq!(banner_title_key(merged.kind), "source.unavailable_title");

    for kind in [
        SourceStateKind::Ok,
        SourceStateKind::Unknown,
        SourceStateKind::Stale,
        SourceStateKind::Failed,
    ] {
        assert_eq!(source_panel_state(kind), PanelState::Unavailable);
    }
    assert_eq!(
        source_panel_state(SourceStateKind::Degraded),
        PanelState::Partial
    );
}

/// The wrappers stay construction-compatible: the empty-state wrapper builds
/// from a localized message, a degraded source still produces a retryable
/// panel, and absent sources still produce NO panel (an absent source never
/// fabricates a failure surface).
#[test]
fn message_panel_wrapper_builds_and_absent_sources_stay_absent() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let _ = message_panel(&theme, "waiting");
    let _ = titled_card(&theme, "card", text("body"));
    assert!(source_state_panel(&theme, None, RefreshRequest::All).is_none());

    let degraded = [status(SourceOutcome::Partial(
        FailureKind::MissingDependency,
    ))];
    assert!(source_state_panel(&theme, Some(&degraded), RefreshRequest::All).is_some());
}

/// Every primitive constructs without I/O across every skin and mode, in
/// its normal and degenerate shapes (non-finite progress samples, every
/// tone, every panel state with and without detail/action) — the sweep
/// exercises the token reads and FillPortion geometry where a broken token
/// binding would otherwise panic in a real frame.
#[test]
fn primitives_build_across_skins_modes_and_degenerate_states() {
    let tones = [
        BadgeTone::Neutral,
        BadgeTone::Success,
        BadgeTone::Warning,
        BadgeTone::Danger,
        BadgeTone::Accent,
    ];
    let states = [
        PanelState::Empty,
        PanelState::Unavailable,
        PanelState::Partial,
        PanelState::Recovery,
    ];
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = theme_for(skin, mode);
            for tone in tones {
                let _ = badge(&theme, "label", tone);
                let _ = progress(&theme, None, tone);
                let _ = progress(&theme, Some(0.5), tone);
                let _ = progress(&theme, Some(f32::NAN), tone);
            }
            let _ = divider(&theme);
            let _ = tooltip(&theme, text("trigger").into(), "a tip");
            for state in states {
                let _ = state_panel(&theme, state, IconId::TriangleAlert, "title", None, None);
                let _ = state_panel(
                    &theme,
                    state,
                    IconId::Applications,
                    "title",
                    Some("detail".to_string()),
                    Some(text("act").into()),
                );
            }
        }
    }
}
