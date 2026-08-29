//! Persisted-config ↔ token mapping glue.
//!
//! Local string↔enum mappers for the fields `core::Config` carries as opaque
//! `String`s (it lives in `core`, which cannot depend on `gpui_app::theme`).
//! The tokens are the theme labels (`Skin::label` / `LightDark::label`) so a
//! human can read the JSON; unknown values fall back to the detected theme
//! rather than forcing a wrong skin/mode. These are pure functions of the
//! stored config; [`super::init`] applies them to the theme/RootView.

use crate::gpui_app::root::TopPage;
use taskmanager_assets::product;
use taskmanager_core::core::config::{
    COLOR_SCHEME_DARK, COLOR_SCHEME_EYEFOREST, COLOR_SCHEME_LIGHT, COLOR_SCHEME_SYSTEM, Config,
    STARTUP_PAGE_PERFORMANCE, STARTUP_PAGE_PROCESSES, STARTUP_PAGE_REMEMBER,
    TEXT_RENDERING_PLATFORM_DEFAULT,
};
use taskmanager_theme::{
    FONT_MISANS_VF, FONT_ROBOTO_MONO, FontAvailability, FontChoice, FontPreference, LightDark,
    Skin, Theme,
};

pub(super) const DEFAULT_APP_ID: &str = product::GPUI_APP_ID;

/// Map a stored skin token back to `Skin`. Case-insensitive; returns `None`
/// for empty/unknown so the caller keeps the host-detected skin.
fn skin_from_token(s: &str) -> Option<Skin> {
    match s.trim().to_ascii_lowercase().as_str() {
        "gnome" => Some(Skin::Gnome),
        "kde" => Some(Skin::Kde),
        "windows" | "win" => Some(Skin::Windows),
        "macos" | "mac" => Some(Skin::Macos),
        "" => None,
        _ => None,
    }
}

pub(in crate::gpui_app::root) fn skin_preference_from_config(cfg: &Config) -> Option<Skin> {
    skin_from_token(&cfg.skin)
}

/// Map a stored color-mode token back to `LightDark`. Case-insensitive; `None`
/// for empty/unknown.
fn mode_from_token(s: &str) -> Option<LightDark> {
    match s.trim().to_ascii_lowercase().as_str() {
        "light" => Some(LightDark::Light),
        "dark" => Some(LightDark::Dark),
        "eyeforest" | "eye-forest" => Some(LightDark::EyeForest),
        _ => None,
    }
}

/// Resolve the persisted color-scheme preference token. Empty and unknown
/// values deliberately mean System so a newer/corrupt config cannot force a
/// palette that the user did not select. Legacy Light/Dark files remain
/// explicit overrides because they predate the System option.
pub(in crate::gpui_app::root) fn color_scheme_from_token(s: &str) -> &'static str {
    match s.trim().to_ascii_lowercase().as_str() {
        "light" => COLOR_SCHEME_LIGHT,
        "dark" => COLOR_SCHEME_DARK,
        "eyeforest" | "eye-forest" => COLOR_SCHEME_EYEFOREST,
        "system" | "" => COLOR_SCHEME_SYSTEM,
        _ => COLOR_SCHEME_SYSTEM,
    }
}

/// Normalize a stored text-rendering token to the only mode the published
/// GPUI 0.2.2 renderer can actually apply. The Zed fork exposes a separate
/// text-raster API, but crates.io GPUI does not; preserving a subpixel or
/// grayscale token here would make Settings report a change that never reaches
/// the renderer.
pub(super) fn text_rendering_from_token(_token: &str) -> &'static str {
    TEXT_RENDERING_PLATFORM_DEFAULT
}

/// Map persisted font tokens (`""` = bundled default, `"system"` = skin
/// default, a bundled family, or an observed system family) back to a
/// [`FontPreference`]. Unknown names fall back to the skin default so a config
/// written by a newer version never forces an unresolvable family; a token
/// naming a retired bundled face (no longer embedded) therefore resolves to
/// the skin default until the user re-saves the preference.
/// Stable config token for the explicit system choice (round-trips through
/// [`font_tokens`]). Distinct from the empty default so an explicit System
/// preference survives reloads while the ABSENT token still means "product
/// default" — the bundled faces.
pub(in crate::gpui_app::root) const FONT_TOKEN_SYSTEM: &str = "system";

pub(in crate::gpui_app::root) fn font_pref_from_config(
    cfg: &Config,
    availability: &FontAvailability,
) -> FontPreference {
    // The shipped default is the PRODUCT font: an absent token ("" — never
    // chosen, or a pre-policy config) resolves to the bundled faces. Only an
    // explicit opt-in ("system", or a legacy named system family) selects the
    // host's fonts.
    let parse = |token: &str, bundled_names: &[&str]| {
        let token = token.trim();
        if token.is_empty()
            || bundled_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(token))
        {
            FontChoice::Bundled
        } else if token.eq_ignore_ascii_case(FONT_TOKEN_SYSTEM) {
            FontChoice::System
        } else if let Some(choice) = availability.choice_for(token) {
            choice
        } else {
            FontChoice::System
        }
    };
    FontPreference {
        ui: parse(&cfg.ui_font, &[FONT_MISANS_VF, FONT_ROBOTO_MONO]),
        mono: parse(&cfg.mono_font, &[FONT_ROBOTO_MONO]),
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
    if let Some(skin) = skin_from_token(&cfg.skin) {
        theme.set_skin(skin);
    }
    if let Some(mode) = mode_from_token(&cfg.mode) {
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
