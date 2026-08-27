//! Terminal mapping of the neutral skin registry (ADR-026).
//!
//! The TUI renders with the SAME design source as the GPUI frontend —
//! `taskmanager-theme`'s skin tables — mapped onto ratatui colors here, at
//! the terminal edge. No color literal lives in the TUI anymore; the theme
//! dependency is taken with default features (no gpui) so this frontend
//! proves the toolkit-neutral build works.
//!
//! The terminal resolves the default GNOME dark theme today. When the TUI
//! gains native-appearance facts it can build a `Theme` via
//! `Theme::detect(NativeAppearance)` and pass it to
//! [`TuiTheme::from_theme`] — nothing else in the renderer changes.

use taskmanager_theme::color::mix;
use taskmanager_theme::{Color, HighContrast, LightDark, ResolvedFonts, Skin, Theme};

/// Runtime-resolved theme construction parameters (ADR-026): the neutral
/// skin, the light/dark mode, and the high-contrast axis. The runtime holds
/// these and rebuilds the terminal palette on every frame, so a settings
/// change re-skins the TUI without touching the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeParams {
    pub skin: Skin,
    pub mode: LightDark,
    pub hc: bool,
}

impl Default for ThemeParams {
    fn default() -> Self {
        Self {
            skin: Skin::Gnome,
            mode: LightDark::Dark,
            hc: false,
        }
    }
}

impl ThemeParams {
    /// Parse the opaque config tokens (`Config::skin` / `Config::mode` /
    /// `Config::hc`) onto typed theme parameters. Unknown skin tokens fall
    /// back to GNOME; `"System"` (and the legacy empty token) resolve to Dark
    /// because the terminal has no native-appearance facts — the TUI's
    /// long-standing default.
    #[must_use]
    pub fn from_config_tokens(skin: &str, mode: &str, hc: bool) -> Self {
        let skin = Skin::ALL
            .into_iter()
            .find(|candidate| candidate.label().eq_ignore_ascii_case(skin))
            .unwrap_or(Skin::Gnome);
        let mode = match mode {
            "Light" => LightDark::Light,
            "Dark" => LightDark::Dark,
            "EyeForest" => LightDark::EyeForest,
            "System" | "" => LightDark::Dark,
            _ => LightDark::Dark,
        };
        Self { skin, mode, hc }
    }

    /// The high-contrast axis as the theme engine's typed token.
    #[must_use]
    pub const fn high_contrast(self) -> HighContrast {
        if self.hc {
            HighContrast::On
        } else {
            HighContrast::Off
        }
    }
}

/// The TUI's resolved terminal palette, derived once per app construction
/// from the neutral [`taskmanager_theme::Palette`]. Terminal colors are opaque RGB — translucent
/// tints are composited over the backdrop here, at the edge.
#[derive(Clone, Copy, Debug)]
pub struct TuiTheme {
    /// Interactive accent (selection, primary actions, focus).
    pub accent: ratatui::style::Color,
    /// Secondary / disabled text.
    pub dim: ratatui::style::Color,
    /// Positive outcomes (running services, live state).
    pub good: ratatui::style::Color,
    /// Cautionary states (pause, warning gauges).
    pub warn: ratatui::style::Color,
    /// Destructive actions / error states.
    pub danger: ratatui::style::Color,
    /// Window backdrop fill.
    pub bg: ratatui::style::Color,
    /// Hairline separators / panel borders.
    pub border: ratatui::style::Color,
    /// Overlay (dialog / help) surface.
    pub overlay_bg: ratatui::style::Color,
    /// Active tab / selected-row highlight surface.
    pub highlight_bg: ratatui::style::Color,
    /// Gauge track fill.
    pub gauge_track_bg: ratatui::style::Color,
    /// Per-category graph accents used by the memory-composition bar segments
    /// and the swap bar (token-derived, matching the gpui/iced palettes).
    pub memory: ratatui::style::Color,
    pub disk: ratatui::style::Color,
    pub network: ratatui::style::Color,
    /// ZFS ARC reclaimable segment: the disk hue composited toward the
    /// backdrop (terminals paint opaque cells), matching gpui/iced's
    /// dimmer ARC tint.
    pub zfs_arc: ratatui::style::Color,
    /// Subdued text / faint bar segment (free / available).
    pub fg_dim: ratatui::style::Color,
    /// Dark reserved fill (bar track / other-reserved segment).
    pub shade: ratatui::style::Color,
}

impl Default for TuiTheme {
    fn default() -> Self {
        Self::from_theme(&Theme::dark())
    }
}

impl TuiTheme {
    /// Resolve the terminal palette from a neutral theme snapshot.
    pub fn from_theme(theme: &Theme) -> Self {
        let palette = theme.palette();
        // Translucent tints composited over the backdrop for terminal
        // display (terminal colors are opaque).
        let highlight = mix(palette.window_backdrop, palette.accent, 0.25);
        let overlay = mix(palette.window_backdrop, palette.surface, 0.5);
        let gauge_track = mix(palette.window_backdrop, palette.fg, 0.12);
        Self {
            accent: rgb(palette.accent),
            dim: rgb(palette.fg_muted),
            good: rgb(palette.success),
            warn: rgb(palette.warning),
            danger: rgb(palette.danger),
            bg: rgb(palette.window_backdrop),
            border: rgb(palette.border),
            overlay_bg: rgb(overlay),
            highlight_bg: rgb(highlight),
            gauge_track_bg: rgb(gauge_track),
            // Per-category graph accents come straight off the neutral theme
            // tokens (not the palette), matching the gpui/iced segment colors.
            memory: rgb(theme.memory),
            disk: rgb(theme.disk),
            network: rgb(theme.network),
            zfs_arc: rgb(mix(palette.window_backdrop, theme.disk, 0.55)),
            fg_dim: rgb(theme.fg_dim),
            shade: rgb(theme.shade),
        }
    }

    /// Build the terminal palette from the runtime's construction parameters,
    /// resolving the skin's system fonts exactly like the graphical frontends.
    pub fn from_params(params: ThemeParams) -> Self {
        let theme = Theme::build(
            params.skin,
            params.mode,
            params.high_contrast(),
            ResolvedFonts::system_for(params.skin),
        );
        Self::from_theme(&theme)
    }
}

/// Map a neutral sRGB token onto a terminal color (alpha ignored — terminals
/// paint opaque cells).
pub fn rgb(c: Color) -> ratatui::style::Color {
    let [r, g, b] = c.to_srgb8();
    ratatui::style::Color::Rgb(r, g, b)
}

#[cfg(test)]
#[path = "../tests/gui/theme_tests.rs"]
mod tests;
