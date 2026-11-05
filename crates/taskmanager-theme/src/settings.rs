//! The public theme-settings contract (ADR-063).
//!
//! A saved preference is a token string in `taskforest-core::Config`, and a
//! resolved appearance is a typed axis of [`crate::theme::Theme`]. This module is
//! the ONLY codec between them: the color-scheme preference axis (whose
//! follow-the-desktop member is a preference, never a palette), the shared
//! developer/capture appearance override, and the vocabulary each axis persists
//! through. Every product reads the same config file into the same theme because
//! they all call here; a frontend that re-spells `"Dark"`, matches a token by
//! hand, or invents its own `System` variant forks the theme-settings layer,
//! which is what let one product's Settings page drift from another's.
//!
//! The per-axis token methods live with their own axis ([`Skin::config_token`],
//! [`LightDark::config_token`], [`RowDensity::config_token`],
//! [`UiSize::config_token`], [`MotionPolicy::config_token`],
//! [`FontChoice::config_token`]); this module owns only what is genuinely
//! cross-axis, so no name has two addresses.
//!
//! [`Skin::config_token`]: crate::theme::Skin::config_token
//! [`LightDark::config_token`]: crate::theme::LightDark::config_token
//! [`RowDensity::config_token`]: crate::tokens::RowDensity::config_token
//! [`UiSize::config_token`]: crate::tokens::UiSize::config_token
//! [`MotionPolicy::config_token`]: crate::tokens::MotionPolicy::config_token
//! [`FontChoice::config_token`]: crate::fonts::FontChoice::config_token

use crate::theme::{LightDark, Skin};

/// The persisted `Config::mode` token for the follow-native preference. Core
/// carries the same string as its wire constant; the `taskmanager-gates` census
/// welds the two tables so neither drifts.
pub const MODE_TOKEN_SYSTEM: &str = "System";

/// The saved color-scheme preference: an explicit palette, or follow the native
/// desktop. `FollowNative` is a preference about *where the mode comes from*, so
/// it is never a [`LightDark`] variant and no frontend needs a local fourth
/// variant of the mode axis.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ModePreference {
    /// Resolve from the observed native color scheme. The empty `Config::mode`
    /// sentinel and any unknown token mean this (a newer or hand-edited config
    /// never forces a palette the user did not select).
    #[default]
    FollowNative,
    /// An explicit Light / Dark / EyeForest choice.
    Explicit(LightDark),
}

impl ModePreference {
    /// The picker order every product paints: the three explicit palettes first,
    /// then follow-native. A settings surface iterates this instead of spelling
    /// its own choice list, so a new axis member cannot arrive in one product
    /// only.
    pub const CHOICES: [Self; 4] = [
        Self::Explicit(LightDark::Light),
        Self::Explicit(LightDark::Dark),
        Self::Explicit(LightDark::EyeForest),
        Self::FollowNative,
    ];

    /// The persisted token for this preference.
    #[must_use]
    pub const fn config_token(self) -> &'static str {
        match self {
            Self::FollowNative => MODE_TOKEN_SYSTEM,
            Self::Explicit(mode) => mode.config_token(),
        }
    }

    /// Resolve a persisted color-scheme token. Case-insensitive; empty, `System`,
    /// and unknown tokens all yield [`Self::FollowNative`].
    #[must_use]
    pub fn from_config_token(token: &str) -> Self {
        match LightDark::from_config_token(token) {
            Some(mode) => Self::Explicit(mode),
            None => Self::FollowNative,
        }
    }

    /// Resolve a persisted color-scheme token strictly: `Some` only for a token
    /// the axis can actually express — the three palettes, `System`, and the
    /// empty first-launch sentinel. A settings surface that must light no segment
    /// for an unrecognized value uses this instead of matching token strings
    /// itself, so the picker's "unknown" path stays a policy of the axis owner.
    #[must_use]
    pub fn from_saved_token(token: &str) -> Option<Self> {
        if let Some(mode) = LightDark::from_config_token(token) {
            return Some(Self::Explicit(mode));
        }
        let trimmed = token.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(MODE_TOKEN_SYSTEM) {
            Some(Self::FollowNative)
        } else {
            None
        }
    }

    /// The explicit choice, when the user pinned one.
    #[must_use]
    pub const fn explicit(self) -> Option<LightDark> {
        match self {
            Self::Explicit(mode) => Some(mode),
            Self::FollowNative => None,
        }
    }

    /// Resolve the preference against the observed native scheme. A product with
    /// no observation yet (or none that means anything) resolves follow-native to
    /// [`LightDark::Dark`], the same fallback the shared detection policy uses, so
    /// the first frame is never a bright surprise.
    #[must_use]
    pub fn resolve(self, native: Option<LightDark>) -> LightDark {
        match self {
            Self::Explicit(mode) => mode,
            Self::FollowNative => native.unwrap_or(LightDark::Dark),
        }
    }

    /// Whether an observed native change may re-resolve the theme at all: an
    /// explicit choice is never displaced by a desktop event.
    #[must_use]
    pub const fn follows_native(self) -> bool {
        matches!(self, Self::FollowNative)
    }
}

impl From<LightDark> for ModePreference {
    fn from(mode: LightDark) -> Self {
        Self::Explicit(mode)
    }
}

/// The environment variable that forces an appearance for tests, demos, and the
/// capture harness. One name, one vocabulary, read by the theme owner instead of
/// four frontend copies of the same parser.
pub const SKIN_OVERRIDE_ENV: &str = "TM_SKIN";
/// The companion high-contrast flag: present and non-empty means force it on.
pub const HIGH_CONTRAST_OVERRIDE_ENV: &str = "TM_SKIN_HC";

/// A forced appearance requested through [`SKIN_OVERRIDE_ENV`].
///
/// The override is a FALLBACK for every product: an explicit persisted
/// preference always wins, because the production config path never consults the
/// environment. That ordering is what keeps capture deterministic without
/// letting a stray variable outrank a user's choice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AppearanceOverride {
    pub skin: Skin,
    pub mode: LightDark,
    pub high_contrast: bool,
}

impl AppearanceOverride {
    /// Parse the shared `<skin>-<mode>` vocabulary: skin `gnome`/`kde`/`win`/
    /// `windows`/`mac`/`macos` and mode `dark`/`light`/`eyeforest`/`eye-forest`,
    /// all case-insensitive. `None` for an unset or syntactically invalid value,
    /// so a malformed request never forces a wrong skin.
    #[must_use]
    pub fn parse(value: &str, high_contrast: bool) -> Option<Self> {
        let (skin_token, mode_token) = value.split_once('-')?;
        Some(Self {
            skin: Skin::from_config_token(skin_token)?,
            mode: LightDark::from_config_token(mode_token)?,
            high_contrast,
        })
    }

    /// Read both override variables from the process environment.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let value = std::env::var(SKIN_OVERRIDE_ENV).ok()?;
        let high_contrast =
            std::env::var(HIGH_CONTRAST_OVERRIDE_ENV).is_ok_and(|raw| !raw.is_empty());
        Self::parse(&value, high_contrast)
    }

    /// The theme-facing form of the override: the two axes plus contrast.
    #[must_use]
    pub const fn axes(self) -> (Skin, LightDark, bool) {
        (self.skin, self.mode, self.high_contrast)
    }
}

#[cfg(test)]
#[path = "../tests/headless/theme_settings.rs"]
mod tests;
