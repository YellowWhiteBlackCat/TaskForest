//! Theme/appearance setters on [`RootView`] (skin switch, font preference).
//!
//! Split out of `root.rs` to keep that file under the 800-line guard; these
//! setters own the font stack + window-background re-application whenever the
//! skin, mode, contrast or font preference changes.

use gpui::Context;

use super::{AppearancePreferences, RootView};
use crate::gpui_app::theme::skin_for_appearance;
use taskmanager_core::core::PreferredColorScheme;
use taskmanager_core::core::config::Config;
use taskmanager_core::core::config::{
    COLOR_SCHEME_DARK, COLOR_SCHEME_EYEFOREST, COLOR_SCHEME_LIGHT, COLOR_SCHEME_SYSTEM,
};
use taskmanager_theme::gpui::background_appearance;
use taskmanager_theme::{
    FontChoice, FontRole, HighContrast, LightDark, Skin, Theme, resolve_fonts,
};

impl RootView {
    pub(crate) fn resolved_persisted_appearance(&self, config: &Config) -> AppearancePreferences {
        let mut appearance = self.presentation.appearance();
        appearance.font = super::startup::font_pref_from_config(config, &self.font_availability);
        appearance.skin = super::startup::skin_preference_from_config(config);
        appearance.color_scheme = super::startup::color_scheme_from_token(&config.mode);
        appearance.high_contrast = config.hc;
        appearance
    }

    /// Rebuild the resolved theme from the already-committed immutable
    /// presentation snapshot. The config fold updates every persisted section
    /// first, so no renderer can observe a mixed appearance generation.
    pub(crate) fn sync_theme_from_presentation(&mut self, cx: &mut Context<Self>) {
        let appearance = self.presentation.appearance();
        if let Some(skin) = appearance.skin {
            self.theme.set_skin(skin);
        } else {
            self.theme
                .set_skin(skin_for_appearance(self.desktop_appearance));
        }
        self.theme.set_high_contrast(appearance.high_contrast);
        if appearance.color_scheme == COLOR_SCHEME_SYSTEM {
            self.apply_system_color_scheme(cx);
        } else {
            self.theme.set_mode(match appearance.color_scheme {
                COLOR_SCHEME_DARK => LightDark::Dark,
                COLOR_SCHEME_EYEFOREST => LightDark::EyeForest,
                _ => LightDark::Light,
            });
        }
        self.rebuild_theme_with_fonts(cx);
        self.apply_window_background(cx);
        cx.notify();
    }

    /// Rebuild the theme against the current skin/mode/HC and this view's font
    /// preference + host availability snapshot, then push the new tokens into
    /// the active theme (theme-level sync, not per-frame).
    fn rebuild_theme_with_fonts(&mut self, _cx: &mut Context<Self>) {
        self.theme = Theme::build(
            self.theme.skin,
            self.theme.mode,
            if self.theme.hc {
                HighContrast::On
            } else {
                HighContrast::Off
            },
            resolve_fonts(
                self.presentation.appearance().font,
                self.theme.skin,
                &self.font_availability,
            ),
        );
    }

    /// Apply the user's font choice for one role (Settings → Font pills) and
    /// re-skin live on the next frame.
    pub fn set_font_choice(&mut self, role: FontRole, choice: FontChoice, cx: &mut Context<Self>) {
        let mut appearance = self.presentation.appearance();
        let pref = &mut appearance.font;
        match role {
            FontRole::Ui => pref.ui = choice,
            FontRole::Mono => pref.mono = choice,
        }
        self.presentation.set_appearance(appearance);
        self.rebuild_theme_with_fonts(cx);
        self.apply_window_background(cx);
        cx.notify();
    }

    /// Switch skin and re-resolve the font stack for it (the new skin's system
    /// family may or may not be installed on this host).
    pub fn set_skin(&mut self, skin: Skin, cx: &mut Context<Self>) {
        let mut appearance = self.presentation.appearance();
        appearance.skin = Some(skin);
        self.presentation.set_appearance(appearance);
        self.theme.set_skin(skin);
        self.rebuild_theme_with_fonts(cx);
        self.apply_window_background(cx);
        cx.notify();
    }

    pub(crate) fn set_high_contrast(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let mut appearance = self.presentation.appearance();
        appearance.high_contrast = enabled;
        self.presentation.set_appearance(appearance);
        self.theme.set_high_contrast(enabled);
        self.apply_window_background(cx);
        cx.notify();
    }

    /// Set the user's color-scheme preference. The System choice resolves
    /// only from the correlated native appearance already owned by RootView;
    /// it never launches a desktop command from the render path.
    pub fn set_color_scheme(&mut self, preference: &'static str, cx: &mut Context<Self>) {
        let preference = match preference {
            COLOR_SCHEME_LIGHT
            | COLOR_SCHEME_DARK
            | COLOR_SCHEME_EYEFOREST
            | COLOR_SCHEME_SYSTEM => preference,
            _ => COLOR_SCHEME_SYSTEM,
        };
        let mut appearance = self.presentation.appearance();
        appearance.color_scheme = preference;
        self.presentation.set_appearance(appearance);
        self.apply_system_color_scheme(cx);
        if preference != COLOR_SCHEME_SYSTEM {
            let mode = match preference {
                COLOR_SCHEME_DARK => LightDark::Dark,
                COLOR_SCHEME_EYEFOREST => LightDark::EyeForest,
                _ => LightDark::Light,
            };
            self.theme.set_mode(mode);
            self.apply_window_background(cx);
        }
        cx.notify();
    }

    /// Re-resolve a System preference after a fresh native appearance event.
    /// This is called by the correlated platform-batch consumer, not by
    /// render, so a changing desktop setting has one typed update path.
    pub(crate) fn apply_system_color_scheme(&mut self, cx: &mut Context<Self>) {
        if self.presentation.appearance().color_scheme != COLOR_SCHEME_SYSTEM {
            return;
        }
        let mode = match self.desktop_appearance.color_scheme {
            PreferredColorScheme::Dark => LightDark::Dark,
            PreferredColorScheme::Light | PreferredColorScheme::Unknown => LightDark::Light,
        };
        if self.theme.mode != mode {
            self.theme.set_mode(mode);
            self.apply_window_background(cx);
        }
    }

    /// Apply one correlated native appearance snapshot. Native family changes
    /// are followed only while the user has not selected an explicit skin;
    /// this prevents a Windows theme notification from undoing a deliberate
    /// GNOME/KDE/Windows/macOS choice. Color mode remains independently gated
    /// by the System color-scheme preference.
    pub(crate) fn apply_desktop_appearance(&mut self, cx: &mut Context<Self>) {
        if self.presentation.appearance().skin.is_none() {
            let skin = skin_for_appearance(self.desktop_appearance);
            if self.theme.skin != skin {
                self.theme.set_skin(skin);
                self.rebuild_theme_with_fonts(cx);
                self.apply_window_background(cx);
            }
        }
        self.apply_system_color_scheme(cx);
    }

    /// Push the current theme's window-background appearance (Opaque/Blurred/
    /// Transparent) onto the platform window, Zed-style. Runs deferred via
    /// `cx.defer`: skin/font switches happen inside the window's own update
    /// stack (where `Window` is already borrowed out of the app), so the
    /// `WindowHandle::update` must land after that stack unwinds — at effect
    /// flush the window is back in the map and the setter applies cleanly.
    fn apply_window_background(&mut self, cx: &mut Context<Self>) {
        let appearance = background_appearance(&self.theme);
        cx.defer(move |cx| {
            if let Some(window) = cx.active_window() {
                let _ = window.update(cx, |_, window, _| {
                    window.set_background_appearance(appearance);
                });
            }
        });
    }
}
