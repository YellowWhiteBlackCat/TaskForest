//! Persisted-config ↔ token mapping glue.
//!
//! The only mappers left in this file are the GPUI-shaped ones: the
//! text-rendering mode the pinned GPUI snapshot understands and the top-page
//! vocabulary this frontend owns. Every THEME axis resolves through the theme
//! owner's public codec (`taskmanager_theme::settings` plus each axis' own
//! `config_token` / `from_config_token`), never through a local string match, so
//! unknown values fail closed to the detected theme in all four products
//! identically (ADR-063). [`super::init`] applies the result to the
//! theme/RootView.

use crate::gpui_app::root::TopPage;
use gpui::TextRenderingMode;
use taskforest_core::core::config::{
    Config, STARTUP_PAGE_PERFORMANCE, STARTUP_PAGE_PROCESSES, STARTUP_PAGE_REMEMBER,
    TEXT_RENDERING_GRAYSCALE, TEXT_RENDERING_PLATFORM_DEFAULT, TEXT_RENDERING_SUBPIXEL,
};
use taskmanager_assets::product;
use taskmanager_theme::{ModePreference, Skin, Theme};

pub(super) const DEFAULT_APP_ID: &str = product::GPUI_APP_ID;

// The axis codecs are the theme owner's public contract (ADR-063): this
// frontend resolves persisted tokens through `Skin::from_config_token`,
// `LightDark::from_config_token`, `ModePreference::from_config_token` and
// `FontPreference::from_config_tokens`, and writes them back through the
// matching `config_token` methods. It keeps no local matcher over a token
// string, so a preference recorded by any product reads identically here.

/// Normalize a stored text-rendering token to a mode supported by the pinned
/// GPUI snapshot. Unknown values fail closed to the platform default.
pub(super) fn text_rendering_from_token(token: &str) -> &'static str {
    match token.trim().to_ascii_lowercase().as_str() {
        TEXT_RENDERING_SUBPIXEL => TEXT_RENDERING_SUBPIXEL,
        TEXT_RENDERING_GRAYSCALE => TEXT_RENDERING_GRAYSCALE,
        _ => TEXT_RENDERING_PLATFORM_DEFAULT,
    }
}

pub(in crate::gpui_app::root) fn text_rendering_mode_from_token(token: &str) -> TextRenderingMode {
    match text_rendering_from_token(token) {
        TEXT_RENDERING_SUBPIXEL => TextRenderingMode::Subpixel,
        TEXT_RENDERING_GRAYSCALE => TextRenderingMode::Grayscale,
        _ => TextRenderingMode::PlatformDefault,
    }
}

/// Stable string token for a [`TopPage`]. Round-trips through [`page_from_token`].
pub(in crate::gpui_app::root) fn page_token(p: TopPage) -> &'static str {
    match p {
        TopPage::Performance => "performance",
        TopPage::Apps => "apps",
        TopPage::Services => "services",
        TopPage::System => "system",
        TopPage::Startup => "startup",
        TopPage::Users => "users",
        TopPage::AppHistory => "app-history",
        TopPage::Containers => "containers",
    }
}

/// Parse a page token back to [`TopPage`]; unknown/empty → `Performance`
/// (the cold-start default — never panics, never a blank page).
fn page_from_token(s: &str) -> TopPage {
    match s.trim() {
        "apps" => TopPage::Apps,
        "services" => TopPage::Services,
        "system" => TopPage::System,
        "startup" => TopPage::Startup,
        "users" => TopPage::Users,
        "app-history" => TopPage::AppHistory,
        "containers" => TopPage::Containers,
        _ => TopPage::Performance,
    }
}

/// Normalize the startup-page preference token. Unknown values fail closed to
/// remember-last so a newer config cannot force an arbitrary page at launch.
pub(super) fn startup_page_from_token(s: &str) -> &'static str {
    match s.trim() {
        STARTUP_PAGE_PERFORMANCE => STARTUP_PAGE_PERFORMANCE,
        STARTUP_PAGE_PROCESSES => STARTUP_PAGE_PROCESSES,
        _ => STARTUP_PAGE_REMEMBER,
    }
}

/// Resolve the page selected by persisted startup policy.
///
/// An explicit `TM_PAGE` deep-link remains authoritative because it is a
/// launch-time command, not a persisted preference. Otherwise the two fixed
/// policies override `last_page`; remember-last (including an unknown policy
/// token normalized by [`startup_page_from_token`]) uses the saved page.
pub(super) fn resolve_startup_page(
    startup_token: &str,
    last_page_token: &str,
    initial_page: TopPage,
    has_explicit_page_override: bool,
) -> TopPage {
    if has_explicit_page_override {
        return initial_page;
    }

    match startup_page_from_token(startup_token) {
        STARTUP_PAGE_PERFORMANCE => TopPage::Performance,
        STARTUP_PAGE_PROCESSES => TopPage::Apps,
        _ => page_from_token(last_page_token),
    }
}

/// Apply the persisted theme preferences onto a detected theme. Unknown/empty
/// skin+mode tokens leave the detected value intact (first-launch behavior);
/// `hc` is always applied (its default `false` matches the non-HC host norm).
/// Each setter rebuilds the theme via `Theme::build`, preserving the other axes.
pub(super) fn apply_cfg_to_theme(theme: &mut Theme, cfg: &Config) {
    if let Some(skin) = Skin::from_config_token(&cfg.skin) {
        theme.set_skin(skin);
    }
    // Only an explicit palette overrides the detected mode; follow-native and
    // the empty/unknown sentinel keep it.
    if let Some(mode) = ModePreference::from_config_token(&cfg.mode).explicit() {
        theme.set_mode(mode);
    }
    theme.set_high_contrast(cfg.hc);
}

pub(super) fn resolve_app_id(custom_app_id: Option<String>) -> String {
    custom_app_id.unwrap_or_else(|| DEFAULT_APP_ID.to_owned())
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_root_startup_config_tokens_tests.rs"]
mod tests;
