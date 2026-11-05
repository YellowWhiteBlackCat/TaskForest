//! Typed demo/capture appearance resolution.

use taskmanager_theme::tokens::UiSize;
use taskmanager_theme::{AppearanceOverride, HighContrast, LightDark, Skin, Theme};

/// The light reference skin the capture gate has always rendered, and the
/// fallback whenever no appearance override is present.
fn reference_demo_theme() -> Theme {
    Theme::build(
        Skin::Gnome,
        LightDark::Light,
        HighContrast::Off,
        crate::pages::settings::embedded_fonts_for(Skin::Gnome),
    )
}

/// Project one forced appearance onto the reference skin's theme snapshot. The
/// font faces are this renderer's embedded-only resolution, so the snapshot
/// names nothing this product cannot paint.
fn theme_from_override(forced: AppearanceOverride) -> Theme {
    Theme::build(
        forced.skin,
        forced.mode,
        if forced.high_contrast {
            HighContrast::On
        } else {
            HighContrast::Off
        },
        crate::pages::settings::embedded_fonts_for(forced.skin),
    )
}

/// Resolve the demo/capture theme from a forced appearance. The override is a
/// FALLBACK: an explicit appearance preference (the persisted config, in
/// production) always wins and the demo path has none, so `TM_SKIN` is the only
/// input here. `None` — unset, invalid, or unknown — resolves the unchanged
/// light reference skin. Parsing is the theme owner's job
/// ([`AppearanceOverride::parse`]), never this frontend's.
pub(super) fn resolve_demo_theme(forced: Option<AppearanceOverride>) -> Theme {
    forced
        .map(theme_from_override)
        .unwrap_or_else(reference_demo_theme)
}

/// Read the shared `TM_SKIN`/`TM_SKIN_HC` override for the demo/capture boot.
pub(super) fn demo_theme_from_env() -> Theme {
    resolve_demo_theme(AppearanceOverride::from_env())
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
