//! OS color-scheme following for the `System` appearance mode (GPUI parity).
//!
//! The shared contract mirrors the GPUI shell's `root/appearance.rs`: a user
//! who has not made an explicit mode choice (`mode = "System"`) follows the
//! desktop light/dark preference; an explicit Light/Dark/EyeForest choice is
//! NEVER overridden by a system change. The iced side observes the OS through
//! `iced::system` (the winit theme + mundy's freedesktop color-scheme stream
//! on Linux), reduces each observation ahead of the domain router, and
//! re-resolves the theme through the ordinary settings bridge — no renderer
//! ever queries the desktop itself.

use super::*;

/// The OS color scheme as observed from the platform. It is stored in the
/// per-`IcedApp` configuration state, never in a process-global slot, so two
/// headless windows/tests cannot overwrite each other's theme input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OsColorScheme {
    Light,
    Dark,
}

impl OsColorScheme {
    /// The shared [`LightDark`] token this observation resolves to.
    #[must_use]
    pub const fn light_dark(self) -> LightDark {
        match self {
            Self::Light => LightDark::Light,
            Self::Dark => LightDark::Dark,
        }
    }
}

/// Map one iced theme mode onto the observation. `Mode::None` (the OS reports
/// no preference) resolves Light — the same Unknown→Light fallback the GPUI
/// shell's `apply_system_color_scheme` uses.
#[must_use]
pub(crate) fn resolve_os_color_scheme(mode: iced::theme::Mode) -> OsColorScheme {
    match mode {
        iced::theme::Mode::Dark => OsColorScheme::Dark,
        iced::theme::Mode::None | iced::theme::Mode::Light => OsColorScheme::Light,
    }
}

/// Resolve the persisted color-mode token against one observation: explicit
/// choices win absolutely; `System` follows the observation and falls back to
/// Dark before the first platform event arrives (the pre-provider default
/// this frontend has always shipped).
#[must_use]
pub(crate) fn resolve_color_mode_with(
    mode: ModeChoice,
    observed: Option<OsColorScheme>,
) -> LightDark {
    match mode {
        ModeChoice::Light => LightDark::Light,
        ModeChoice::Dark => LightDark::Dark,
        ModeChoice::EyeForest => LightDark::EyeForest,
        ModeChoice::System => observed.map_or(LightDark::Dark, OsColorScheme::light_dark),
    }
}

impl IcedApp {
    /// Apply one observed OS color scheme (the subscription event or the boot
    /// query). The observation is always stored; the theme rebuild happens
    /// only when the persisted mode is `System` AND the observation actually
    /// changed — an explicit Light/Dark/EyeForest choice is never overridden
    /// (GPUI `apply_system_color_scheme` parity), and an unchanged
    /// observation never churns the config bridge.
    pub(crate) fn apply_observed_color_scheme(&mut self, mode: iced::theme::Mode) {
        let scheme = resolve_os_color_scheme(mode);
        if self.configuration.observed_color_scheme() == Some(scheme) {
            return;
        }
        self.configuration.set_observed_color_scheme(Some(scheme));
        let config = self.config_draft();
        if ModeChoice::from_token(config.mode.as_str()) != ModeChoice::System {
            return;
        }
        // Re-commit the UNCHANGED draft: the coordinator reports `NoChange`
        // and the local re-resolve rebuilds the theme against the stored
        // observation through the ordinary settings bridge.
        self.commit_config_draft(config);
    }
}

/// The OS light/dark subscription → [`Message::SystemThemeChanged`].
///
/// Always mounted, deliberately: iced's `theme_changes` is a passive filter
/// over the runtime's theme broadcast (mundy's event-driven preferences
/// stream runs for the whole process on Linux regardless of this
/// subscription), so gating it on `mode = System` would save nothing and
/// would leave a STALE observation for the next switch into System mode.
/// The rebuild cost stays gated in [`IcedApp::apply_observed_color_scheme`].
pub(crate) fn subscription() -> iced::Subscription<Message> {
    iced::system::theme_changes().map(Message::SystemThemeChanged)
}

/// The boot-time query for the CURRENT OS color scheme (`theme_changes` only
/// reports subsequent changes). Returned as the production boot task in
/// `run.rs` so the first System-mode frame can already follow the desktop.
pub(crate) fn initial_query() -> iced::Task<Message> {
    iced::system::theme().map(Message::SystemThemeChanged)
}

/// Reduce one [`Message::SystemThemeChanged`] ahead of the domain router
/// (wired in the `run.rs` update closure). The router's arm for the variant
/// only satisfies exhaustive routing; this is the real reduction path.
pub(crate) fn reduce_system_theme_change(app: &mut IcedApp, mode: iced::theme::Mode) {
    app.apply_observed_color_scheme(mode);
}

#[cfg(test)]
#[path = "../../tests/gui/appearance_tests.rs"]
mod tests;
