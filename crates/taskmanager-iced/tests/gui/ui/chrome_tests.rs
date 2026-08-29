// test-intent: behavior
use super::*;
/// The quitting notice replaces the live status only once the shell has
/// consumed a quit request; otherwise the status passes through verbatim
/// (a telemetry refresh line must never be mistaken for the quit state).
#[test]
fn footer_status_swaps_in_the_quitting_notice_only_after_quit() {
    use taskmanager_application::i18n::{Language, set_language};

    set_language(Language::En);

    let mut app = crate::IcedApp::demo();
    app.shell.set_feedback_activity("collecting telemetry");
    app.shell.clear_feedback_notice();
    assert_eq!(footer_status(&app.shell), "collecting telemetry");

    app.shell
        .request_quit(taskmanager_shell::QuitReason::Keyboard);
    assert_eq!(footer_status(&app.shell), t("hint.quitting"));

    set_language(Language::En);
}

/// The footer alert pill maps the worst active severity onto the shared
/// badge grammar with the SAME palette semantics the old inline
/// severity-colored text pill used: Critical → danger, Warning → caution,
/// Info → accent. The tone fills stay the palette's semantic tokens, so the
/// pill's color vocabulary is unchanged — only its presentation moved into
/// the tone-filled capsule.
#[test]
fn alert_badge_tone_keeps_the_severity_palette_mapping() {
    use taskmanager_core::core::alerts::AlertSeverity;

    let app = crate::IcedApp::demo();
    let theme = app.theme();
    assert_eq!(
        alert_badge_tone(AlertSeverity::Critical),
        components::BadgeTone::Danger
    );
    assert_eq!(
        alert_badge_tone(AlertSeverity::Warning),
        components::BadgeTone::Warning
    );
    assert_eq!(
        alert_badge_tone(AlertSeverity::Info),
        components::BadgeTone::Accent
    );
    assert_eq!(
        alert_badge_tone(AlertSeverity::Critical).fill(theme),
        theme.palette().danger
    );
    assert_eq!(
        alert_badge_tone(AlertSeverity::Warning).fill(theme),
        theme.palette().warning
    );
    assert_eq!(
        alert_badge_tone(AlertSeverity::Info).fill(theme),
        theme.palette().accent
    );
}
