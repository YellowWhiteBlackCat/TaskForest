//! Platform-neutral host desktop appearance preferences.

use serde::{Deserialize, Serialize};

/// Desktop family whose interaction skin best matches the current session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DesktopFamily {
    Gnome,
    Kde,
    Windows,
    Macos,
    #[default]
    Unknown,
}

/// Host preference for light or dark application surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PreferredColorScheme {
    Light,
    Dark,
    #[default]
    Unknown,
}

/// Appearance facts observed by a native desktop adapter.
///
/// `None` for `high_contrast` means the platform could not observe the
/// preference. It is deliberately different from a confirmed `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DesktopAppearance {
    pub family: DesktopFamily,
    pub color_scheme: PreferredColorScheme,
    pub high_contrast: Option<bool>,
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_appearance_tests.rs"]
mod tests;
