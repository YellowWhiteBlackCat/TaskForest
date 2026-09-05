//! Appearance and theme mutation tests for [`RootView`].

use super::*;
use gpui::{AppContext, TestAppContext};
use taskmanager_core::core::appearance::{DesktopAppearance, DesktopFamily, PreferredColorScheme};
use taskmanager_core::core::config::{COLOR_SCHEME_DARK, COLOR_SCHEME_SYSTEM};
use taskmanager_theme::{LightDark, Skin, Theme};

#[gpui::test]
fn set_high_contrast_toggles_theme_and_presentation(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, cx| {
        assert!(!view.theme.hc);
        assert!(!view.presentation_snapshot().appearance.high_contrast);

        view.set_high_contrast(true, cx);
        assert!(view.theme.hc);
        assert!(view.presentation_snapshot().appearance.high_contrast);
        assert_eq!(
            view.theme.border.a, 1.0,
            "high contrast border must be fully opaque"
        );

        view.set_high_contrast(false, cx);
        assert!(!view.theme.hc);
        assert!(!view.presentation_snapshot().appearance.high_contrast);
    });
}

#[gpui::test]
fn apply_system_color_scheme_follows_desktop_appearance_in_system_mode(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, cx| {
        view.set_color_scheme(COLOR_SCHEME_SYSTEM, cx);
        assert_eq!(
            view.presentation_snapshot().appearance.color_scheme,
            COLOR_SCHEME_SYSTEM
        );

        view.desktop_appearance = DesktopAppearance {
            family: DesktopFamily::Gnome,
            color_scheme: PreferredColorScheme::Dark,
            high_contrast: None,
        };
        view.apply_system_color_scheme(cx);
        assert_eq!(view.theme.mode, LightDark::Dark);

        view.desktop_appearance.color_scheme = PreferredColorScheme::Light;
        view.apply_system_color_scheme(cx);
        assert_eq!(view.theme.mode, LightDark::Light);

        view.desktop_appearance.color_scheme = PreferredColorScheme::Unknown;
        view.apply_system_color_scheme(cx);
        assert_eq!(
            view.theme.mode,
            LightDark::Light,
            "unknown scheme falls back to light"
        );

        // Explicit user color-scheme choice overrides following system appearance
        view.set_color_scheme(COLOR_SCHEME_DARK, cx);
        assert_eq!(view.theme.mode, LightDark::Dark);
        view.desktop_appearance.color_scheme = PreferredColorScheme::Light;
        view.apply_system_color_scheme(cx);
        assert_eq!(
            view.theme.mode,
            LightDark::Dark,
            "explicit color scheme preference must not be clobbered by desktop appearance"
        );
    });
}

#[gpui::test]
fn apply_desktop_appearance_follows_desktop_family_when_skin_unspecified(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, cx| {
        // With no explicit skin, desktop appearance drives the skin and mode
        view.desktop_appearance = DesktopAppearance {
            family: DesktopFamily::Gnome,
            color_scheme: PreferredColorScheme::Dark,
            high_contrast: None,
        };
        view.apply_desktop_appearance(cx);
        assert_eq!(view.theme.skin, Skin::Gnome);
        assert_eq!(view.theme.mode, LightDark::Dark);

        view.desktop_appearance = DesktopAppearance {
            family: DesktopFamily::Kde,
            color_scheme: PreferredColorScheme::Light,
            high_contrast: None,
        };
        view.apply_desktop_appearance(cx);
        assert_eq!(view.theme.skin, Skin::Kde);
        assert_eq!(view.theme.mode, LightDark::Light);

        // Explicit skin choice must not be clobbered by desktop appearance
        view.set_skin(Skin::Windows, cx);
        assert_eq!(view.theme.skin, Skin::Windows);

        view.desktop_appearance = DesktopAppearance {
            family: DesktopFamily::Gnome,
            color_scheme: PreferredColorScheme::Dark,
            high_contrast: None,
        };
        view.apply_desktop_appearance(cx);
        assert_eq!(
            view.theme.skin,
            Skin::Windows,
            "explicit skin choice must be preserved"
        );
    });
}
