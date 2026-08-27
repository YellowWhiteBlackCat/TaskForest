//! Pure mapping from native-adapter appearance facts to theme axes.
//!
//! The theme crate stays toolkit-neutral (gpui-only, ADR-017 §3.1): the
//! executable composition edge adapts `taskmanager-application`'s
//! `DesktopAppearance` onto [`NativeAppearance`] (see
//! `taskmanager_gpui::gpui_app::theme::detect`) — the adapter's `Unknown` states map to `None`,
//! never to a guessed value; the fallback decisions live here.

use crate::theme::{HighContrast, LightDark, Skin};

/// Host appearance facts in the theme's own vocabulary. `None` means the
/// adapter could not observe the preference (taskmanager-core's
/// `DesktopFamily::Unknown` / `PreferredColorScheme::Unknown`), deliberately
/// distinct from a confirmed value.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct NativeAppearance {
    pub family: Option<Skin>,
    pub scheme: Option<LightDark>,
    pub high_contrast: Option<bool>,
}

pub fn detect_skin(appearance: NativeAppearance) -> Skin {
    appearance.family.unwrap_or(Skin::Gnome)
}

pub fn detect_mode(appearance: NativeAppearance) -> LightDark {
    match appearance.scheme {
        Some(LightDark::Dark) => LightDark::Dark,
        _ => LightDark::Light,
    }
}

pub fn detect_high_contrast(appearance: NativeAppearance) -> HighContrast {
    if appearance.high_contrast == Some(true) {
        HighContrast::On
    } else {
        HighContrast::Off
    }
}

#[cfg(test)]
#[path = "../tests/headless/theme_detection.rs"]
mod tests;
