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

use taskforest_core::core::appearance::DesktopAppearance;
use taskmanager_theme::color::mix;
use taskmanager_theme::{
    AppearanceOverride, Color, HighContrast, LightDark, ModePreference, ResolvedFonts, Skin, Theme,
};

use crate::TuiApp;
use crate::TuiTerminalProfile;
use taskforest_core::core::appearance::PreferredColorScheme;
use taskmanager_ui_contract::IconId;

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
        Self::from_config_tokens_with_appearance(skin, mode, hc, None)
    }

    /// Parse the opaque config tokens with optional observed desktop
    /// appearance facts from the platform runtime.
    #[must_use]
    pub fn from_config_tokens_with_appearance(
        skin: &str,
        mode: &str,
        hc: bool,
        appearance: Option<DesktopAppearance>,
    ) -> Self {
        // The axis codec is the theme owner's (ADR-063): the terminal spells no
        // token of its own, so a preference written by any product reads back as
        // the same skin and the same mode here.
        let skin = Skin::from_config_token(skin).unwrap_or(Skin::Gnome);
        let observed = match appearance.map(|app| app.color_scheme) {
            Some(PreferredColorScheme::Light) => Some(LightDark::Light),
            Some(PreferredColorScheme::Dark) => Some(LightDark::Dark),
            Some(PreferredColorScheme::Unknown) | None => None,
        };
        let mode = ModePreference::from_config_token(mode).resolve(observed);
        let hc = hc
            || appearance
                .and_then(|app| app.high_contrast)
                .unwrap_or(false);
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

/// Project the theme owner's forced appearance onto the terminal's own
/// parameters. The override is a FALLBACK: an explicit configuration/preference
/// (the production [`ThemeParams::from_config_tokens_with_appearance`] path) is
/// never displaced because it does not consult the environment.
impl From<AppearanceOverride> for ThemeParams {
    fn from(forced: AppearanceOverride) -> Self {
        let (skin, mode, hc) = forced.axes();
        Self { skin, mode, hc }
    }
}

/// Read the shared `TM_SKIN`/`TM_SKIN_HC` override from the process
/// environment for the demo/capture boot. `None` when the variable is unset or
/// invalid, which keeps the demo default exactly as it is today.
#[must_use]
pub(crate) fn forced_theme_params_from_env() -> Option<ThemeParams> {
    AppearanceOverride::from_env().map(ThemeParams::from)
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
    /// The immutable terminal capability profile used to resolve this
    /// palette and all semantic glyphs in the frame.
    pub terminal: TuiTerminalProfile,
}

impl Default for TuiTheme {
    fn default() -> Self {
        Self::from_theme(&Theme::dark())
    }
}

impl TuiTheme {
    /// Resolve the terminal palette from a neutral theme snapshot.
    pub fn from_theme(theme: &Theme) -> Self {
        Self::from_theme_with_profile(theme, TuiTerminalProfile::default())
    }

    /// Resolve the neutral skin through an explicit terminal capability
    /// profile.  Palette quantization happens here at the terminal edge, so
    /// component renderers only consume already-safe opaque colors.
    pub fn from_theme_with_profile(theme: &Theme, terminal: TuiTerminalProfile) -> Self {
        let palette = theme.palette();
        // Translucent tints composited over the backdrop for terminal
        // display (terminal colors are opaque).
        let highlight = mix(palette.window_backdrop, palette.accent, 0.25);
        let overlay = mix(palette.window_backdrop, palette.surface, 0.5);
        let gauge_track = mix(palette.window_backdrop, palette.fg, 0.12);
        Self {
            accent: terminal.color(rgb(palette.accent)),
            dim: terminal.color(rgb(palette.fg_muted)),
            good: terminal.color(rgb(palette.success)),
            warn: terminal.color(rgb(palette.warning)),
            danger: terminal.color(rgb(palette.danger)),
            bg: terminal.color(rgb(palette.window_backdrop)),
            border: terminal.color(rgb(palette.border)),
            overlay_bg: terminal.color(rgb(overlay)),
            highlight_bg: terminal.color(rgb(highlight)),
            gauge_track_bg: terminal.color(rgb(gauge_track)),
            // Per-category graph accents come straight off the neutral theme
            // tokens (not the palette), matching the gpui/iced segment colors.
            memory: terminal.color(rgb(theme.memory)),
            disk: terminal.color(rgb(theme.disk)),
            network: terminal.color(rgb(theme.network)),
            zfs_arc: terminal.color(rgb(mix(palette.window_backdrop, theme.disk, 0.55))),
            fg_dim: terminal.color(rgb(theme.fg_dim)),
            shade: terminal.color(rgb(theme.shade)),
            terminal,
        }
    }

    /// Build the terminal palette from the runtime's construction parameters,
    /// resolving the skin's system fonts exactly like the graphical frontends.
    pub fn from_params(params: ThemeParams) -> Self {
        Self::from_params_with_profile(params, TuiTerminalProfile::default())
    }

    /// Build a terminal theme from runtime parameters and the profile
    /// captured at the composition edge.
    pub fn from_params_with_profile(params: ThemeParams, terminal: TuiTerminalProfile) -> Self {
        let theme = Theme::build(
            params.skin,
            params.mode,
            params.high_contrast(),
            ResolvedFonts::system_for(params.skin),
        );
        Self::from_theme_with_profile(&theme, terminal)
    }

    /// Resolve one ad-hoc Ratatui color through this frame's profile.  This
    /// keeps legacy semantic colors (white text, black-on-accent hints, and
    /// graph palette extensions) honest on low-color/no-color terminals too.
    #[must_use]
    pub fn color(self, color: ratatui::style::Color) -> ratatui::style::Color {
        self.terminal.color(color)
    }

    /// Resolve a semantic icon through this frame's glyph repertoire.
    #[must_use]
    pub const fn glyph(self, icon: IconId) -> &'static str {
        self.terminal.glyph(icon)
    }
}

/// Map a neutral sRGB token onto a terminal color (alpha ignored — terminals
/// paint opaque cells).
pub fn rgb(c: Color) -> ratatui::style::Color {
    let [r, g, b] = c.to_srgb8();
    ratatui::style::Color::Rgb(r, g, b)
}

impl TuiApp {
    /// Apply one correlated native appearance snapshot to theme parameters.
    pub fn apply_desktop_appearance(&mut self, appearance: DesktopAppearance) {
        self.observed_appearance = Some(appearance);
        let config = self.config_draft.clone();
        if ModePreference::from_config_token(&config.mode).follows_native() {
            self.theme_params = ThemeParams::from_config_tokens_with_appearance(
                &config.skin,
                &config.mode,
                config.hc,
                Some(appearance),
            );
        }
    }
}

#[cfg(test)]
#[path = "../tests/gui/theme_tests.rs"]
mod tests;
