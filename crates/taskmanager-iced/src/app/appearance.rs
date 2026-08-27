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

use std::sync::atomic::{AtomicU8, Ordering};

use super::*;

/// The last OS color scheme observed through the platform channel
/// (0 = not yet observed, 1 = light, 2 = dark).
///
/// Process-global is honest here: the desktop color scheme is process-wide
/// environment state, the iced product is single-instance (the launcher's
/// instance guard), and `IcedApp` owns the one window — there is no second
/// desktop observation this could cross.
static OS_COLOR_SCHEME: AtomicU8 = AtomicU8::new(0);

/// The OS color scheme as observed from the platform. The discriminants are
/// the stored encoding (see [`OS_COLOR_SCHEME`]): 0 stays reserved for
/// "not yet observed".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OsColorScheme {
    Light = 1,
    Dark = 2,
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

/// The stored observation (`None` before the first platform event).
#[must_use]
pub(crate) fn observed_color_scheme() -> Option<OsColorScheme> {
    match OS_COLOR_SCHEME.load(Ordering::Relaxed) {
        1 => Some(OsColorScheme::Light),
        2 => Some(OsColorScheme::Dark),
        _ => None,
    }
}

/// Store one observation, returning the previous stored value.
fn store_observed_color_scheme(scheme: OsColorScheme) -> Option<OsColorScheme> {
    let previous = observed_color_scheme();
    OS_COLOR_SCHEME.store(scheme as u8, Ordering::Relaxed);
    previous
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

/// [`resolve_color_mode_with`] against the currently stored observation.
#[must_use]
pub(crate) fn resolve_color_mode(mode: ModeChoice) -> LightDark {
    resolve_color_mode_with(mode, observed_color_scheme())
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
        if store_observed_color_scheme(scheme) == Some(scheme) {
            return;
        }
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
