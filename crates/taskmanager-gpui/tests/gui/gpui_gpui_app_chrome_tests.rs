//! Pure window-frame policy facts: persisted-token round-trips, the
//! decoration mode each preference requests, and the honest-outcome notice
//! decision. Data-layer only — no gpui test window needed.

use super::*;
use gpui::WindowDecorations;
use taskmanager_core::core::config::{
    WINDOW_DECORATIONS_CUSTOM, WINDOW_DECORATIONS_NATIVE, WINDOW_DECORATIONS_SYSTEM,
};

#[test]
fn window_decorations_tokens_round_trip_and_fail_closed_to_system() {
    // The three canonical tokens parse back to their preferences.
    assert_eq!(
        WindowDecorationsPreference::from_config_token(WINDOW_DECORATIONS_SYSTEM),
        WindowDecorationsPreference::System
    );
    assert_eq!(
        WindowDecorationsPreference::from_config_token(WINDOW_DECORATIONS_NATIVE),
        WindowDecorationsPreference::Native
    );
    assert_eq!(
        WindowDecorationsPreference::from_config_token(WINDOW_DECORATIONS_CUSTOM),
        WindowDecorationsPreference::Custom
    );

    // Whitespace is tolerated at the persistence boundary.
    assert_eq!(
        WindowDecorationsPreference::from_config_token(" custom "),
        WindowDecorationsPreference::Custom
    );

    // Unknown future tokens fail closed to System: a hand-edited or newer
    // config can never force a frame this build cannot deliver.
    assert_eq!(
        WindowDecorationsPreference::from_config_token("glass"),
        WindowDecorationsPreference::System
    );

    // Every preference serializes back to the exact canonical token.
    for (pref, token) in [
        (
            WindowDecorationsPreference::System,
            WINDOW_DECORATIONS_SYSTEM,
        ),
        (
            WindowDecorationsPreference::Native,
            WINDOW_DECORATIONS_NATIVE,
        ),
        (
            WindowDecorationsPreference::Custom,
            WINDOW_DECORATIONS_CUSTOM,
        ),
    ] {
        assert_eq!(pref.config_token(), token);
        assert_eq!(WindowDecorationsPreference::from_config_token(token), pref);
    }

    // The System token stays the empty sentinel so an untouched preference
    // round-trips byte-identically with pre-preference config files.
    assert!(WINDOW_DECORATIONS_SYSTEM.is_empty());
    assert_eq!(
        WindowDecorationsPreference::default(),
        WindowDecorationsPreference::System
    );
}

#[test]
fn window_decorations_preference_requests_the_matching_mode() {
    // System and Native both request the native frame — System silently
    // accepts the CSD fallback when refused, Native expects it to be honored.
    assert_eq!(
        WindowDecorationsPreference::System.requested_decorations(),
        WindowDecorations::Server
    );
    assert_eq!(
        WindowDecorationsPreference::Native.requested_decorations(),
        WindowDecorations::Server
    );
    // Custom asks for client decorations up front (the Zed-style rounded
    // app chrome), even where the compositor could draw a native frame.
    assert_eq!(
        WindowDecorationsPreference::Custom.requested_decorations(),
        WindowDecorations::Client
    );
}

#[test]
fn decoration_outcome_notice_reports_only_contradicted_explicit_requests() {
    use DecorationOutcomeNotice::*;

    // System never promised a mode — no notice in either outcome, including
    // the compositor-forced CSD fallback that GNOME users always get.
    assert_eq!(
        decoration_outcome_notice(WindowDecorationsPreference::System, true),
        None
    );
    assert_eq!(
        decoration_outcome_notice(WindowDecorationsPreference::System, false),
        None
    );

    // Native honored → silent; Native refused (GNOME/Mutter) → honest notice.
    assert_eq!(
        decoration_outcome_notice(WindowDecorationsPreference::Native, true),
        None
    );
    assert_eq!(
        decoration_outcome_notice(WindowDecorationsPreference::Native, false),
        Some(NativeRefused)
    );

    // Custom granted (every Wayland compositor honors a client request) →
    // silent; Custom refused (platforms whose toolkit ignores the request) →
    // honest notice.
    assert_eq!(
        decoration_outcome_notice(WindowDecorationsPreference::Custom, false),
        None
    );
    assert_eq!(
        decoration_outcome_notice(WindowDecorationsPreference::Custom, true),
        Some(CustomRefused)
    );

    // Each refusal carries its own localized copy.
    assert_ne!(
        NativeRefused.i18n_key(),
        CustomRefused.i18n_key(),
        "the two refusals describe opposite outcomes and must not share copy"
    );
}
