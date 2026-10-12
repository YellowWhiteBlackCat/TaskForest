//! Typed demo/capture appearance resolution.

use taskmanager_theme::tokens::UiSize;
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

/// The light reference skin the capture gate has always rendered, and the
/// fallback whenever no appearance override is present.
fn reference_demo_theme() -> Theme {
    Theme::build(
        Skin::Gnome,
        LightDark::Light,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    )
}

/// Parse the shared testing/developer appearance override GPUI owns
/// (`TM_SKIN=<skin>-<mode>`): skin `gnome`/`kde`/`win`/`windows`/`mac`/`macos`
/// and mode `dark`/`light`/`eyeforest`/`eye-forest`, all case-insensitive.
/// `None` for an unset or syntactically invalid value. This is the SAME
/// vocabulary (and env name) GPUI's
/// `taskmanager_gpui::gpui_app::theme::forced_skin_from_env` reads — a second
/// vocabulary would let the capture harness and the frontends drift.
fn parse_tm_skin(value: &str) -> Option<(Skin, LightDark)> {
    let (skin_token, mode_token) = value.split_once('-')?;
    let skin = match skin_token.to_ascii_lowercase().as_str() {
        "gnome" => Skin::Gnome,
        "kde" => Skin::Kde,
        "win" | "windows" => Skin::Windows,
        "mac" | "macos" => Skin::Macos,
        _ => return None,
    };
    let mode = match mode_token.to_ascii_lowercase().as_str() {
        "dark" => LightDark::Dark,
        "light" => LightDark::Light,
        "eyeforest" | "eye-forest" => LightDark::EyeForest,
        _ => return None,
    };
    Some((skin, mode))
}

/// Resolve the demo/capture theme. The override is a FALLBACK: an explicit
/// appearance preference (the persisted config, in production) always wins and
/// the demo path has none, so `TM_SKIN` is the only input here. An unset or
/// invalid value resolves the unchanged light reference skin.
pub(super) fn resolve_demo_theme(value: Option<&str>, high_contrast: bool) -> Theme {
    let Some((skin, mode)) = value.and_then(parse_tm_skin) else {
        return reference_demo_theme();
    };
    Theme::build(
        skin,
        mode,
        if high_contrast {
            HighContrast::On
        } else {
            HighContrast::Off
        },
        ResolvedFonts::system_for(skin),
    )
}

/// Read the shared `TM_SKIN`/`TM_SKIN_HC` override for the demo/capture boot.
pub(super) fn demo_theme_from_env() -> Theme {
    let value = std::env::var("TM_SKIN").ok();
    let high_contrast = std::env::var("TM_SKIN_HC").is_ok_and(|raw| !raw.is_empty());
    resolve_demo_theme(value.as_deref(), high_contrast)
}

/// Read the shared `TM_UI_SIZE` capture override (`Small`/`Standard`/`Large`,
/// case-insensitive, same token vocabulary as the persisted config). The
/// capture matrix exercises the UI-size ladder through the same env seam
/// `TM_SKIN` uses; unset or invalid resolves the readability-first Standard.
pub(super) fn demo_ui_size_from_env() -> UiSize {
    std::env::var("TM_UI_SIZE")
        .ok()
        .as_deref()
        .map(str::trim)
        .map(UiSize::from_config_token)
        .unwrap_or(UiSize::Standard)
}
