// test-intent: behavior
use super::*;

/// The iced theme mode maps onto the shared observation with the GPUI
/// fallback: no OS preference (`Mode::None`) reads as Light, exactly like
/// the GPUI shell's Unknown→Light path in `apply_system_color_scheme`.
#[test]
fn os_theme_mode_resolves_light_dark_with_none_as_light() {
    assert_eq!(
        resolve_os_color_scheme(iced::theme::Mode::Dark),
        OsColorScheme::Dark
    );
    assert_eq!(
        resolve_os_color_scheme(iced::theme::Mode::Light),
        OsColorScheme::Light
    );
    assert_eq!(
        resolve_os_color_scheme(iced::theme::Mode::None),
        OsColorScheme::Light,
        "no OS preference falls back to Light (GPUI Unknown parity)"
    );
    assert_eq!(OsColorScheme::Light.light_dark(), LightDark::Light);
    assert_eq!(OsColorScheme::Dark.light_dark(), LightDark::Dark);
}

/// Explicit mode choices win absolutely against every observation state, and
/// `System` follows the observation (Dark until the first platform event —
/// the default this frontend shipped before the appearance provider).
#[test]
fn explicit_choices_ignore_the_observation_and_system_follows_it() {
    let observations = [None, Some(OsColorScheme::Light), Some(OsColorScheme::Dark)];
    for observed in observations {
        assert_eq!(
            resolve_color_mode_with(ModeChoice::Light, observed),
            LightDark::Light
        );
        assert_eq!(
            resolve_color_mode_with(ModeChoice::Dark, observed),
            LightDark::Dark
        );
        assert_eq!(
            resolve_color_mode_with(ModeChoice::EyeForest, observed),
            LightDark::EyeForest
        );
    }
    assert_eq!(
        resolve_color_mode_with(ModeChoice::System, None),
        LightDark::Dark,
        "System before any observation keeps the Dark default"
    );
    assert_eq!(
        resolve_color_mode_with(ModeChoice::System, Some(OsColorScheme::Light)),
        LightDark::Light
    );
    assert_eq!(
        resolve_color_mode_with(ModeChoice::System, Some(OsColorScheme::Dark)),
        LightDark::Dark
    );
}

/// The reducer contract against a live app: an explicit choice is never
/// overridden by an OS flip, a System preference follows the desktop live,
/// and switching into System resolves against the observation the app kept
/// while the explicit choice was active.
#[test]
fn system_mode_follows_the_desktop_and_explicit_choices_do_not() {
    let mut app = IcedApp::demo();
    // An explicit Dark choice: OS observations land but never move the theme.
    app.apply_settings_change(SettingsChange::Mode(ModeChoice::Dark));
    assert_eq!(app.theme().mode, LightDark::Dark);
    app.apply_observed_color_scheme(iced::theme::Mode::Light);
    assert_eq!(
        app.theme().mode,
        LightDark::Dark,
        "an explicit user choice is never overridden by the OS"
    );

    // Switching into System resolves against the observation stored while the
    // explicit choice was active.
    app.apply_settings_change(SettingsChange::Mode(ModeChoice::System));
    assert_eq!(app.theme().mode, LightDark::Light);

    // The desktop flipping moves a System preference live, in both directions.
    app.apply_observed_color_scheme(iced::theme::Mode::Dark);
    assert_eq!(app.theme().mode, LightDark::Dark);
    app.apply_observed_color_scheme(iced::theme::Mode::Light);
    assert_eq!(app.theme().mode, LightDark::Light);

    // An unchanged observation is absorbed without disturbing the theme.
    app.apply_observed_color_scheme(iced::theme::Mode::Light);
    assert_eq!(app.theme().mode, LightDark::Light);
}

#[test]
fn os_observations_are_isolated_between_app_instances() {
    let mut light = IcedApp::demo();
    let mut dark = IcedApp::demo();
    light.apply_settings_change(SettingsChange::Mode(ModeChoice::System));
    dark.apply_settings_change(SettingsChange::Mode(ModeChoice::System));

    light.apply_observed_color_scheme(iced::theme::Mode::Light);
    dark.apply_observed_color_scheme(iced::theme::Mode::Dark);

    assert_eq!(light.theme().mode, LightDark::Light);
    assert_eq!(dark.theme().mode, LightDark::Dark);
}
