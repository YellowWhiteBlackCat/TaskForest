//! test-intent: behavior

use super::super::begin_observations;
use crate::{TuiApp, TuiSurface};
use taskmanager_core::core::appearance::{DesktopAppearance, DesktopFamily, PreferredColorScheme};
use taskmanager_shell::ShellApp;
use taskmanager_test_support::desktop_appearance::platform;

#[test]
fn native_boot_observes_desktop_appearance_once_and_information_uses_the_response() {
    let expected = DesktopAppearance {
        family: DesktopFamily::Kde,
        color_scheme: PreferredColorScheme::Dark,
        high_contrast: Some(false),
    };
    let (mut platform, recorder) = platform(expected);
    let mut app = TuiApp::from_shell(ShellApp::new());
    begin_observations(&mut app, &mut platform);
    assert_eq!(recorder.submissions().expect("requests").len(), 1);
    assert!(
        app.observed_appearance.is_none(),
        "submission alone cannot invent an observation"
    );
    app.apply_platform_batch(platform.try_drain().expect("observed response"));
    app.apply_platform_batch(platform.try_drain().expect("quiet port"));
    assert_eq!(recorder.submissions().expect("requests").len(), 1);
    assert_eq!(app.observed_appearance, Some(expected));
    app.open_system_information();
    let Some(TuiSurface::SystemInformation(view)) = app.local_surface() else {
        panic!("native information");
    };
    assert!(
        view.facts
            .iter()
            .flat_map(|group| &group.rows)
            .any(|row| row.value == "KDE Plasma")
    );
    assert!(
        view.facts
            .iter()
            .flat_map(|group| &group.rows)
            .any(|row| row.label_key == "system_about.color_scheme")
    );
}
