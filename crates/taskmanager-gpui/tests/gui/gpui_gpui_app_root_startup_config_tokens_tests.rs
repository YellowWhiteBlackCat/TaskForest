use super::{
    DEFAULT_APP_ID, apply_cfg_to_theme, page_from_token, page_token, resolve_app_id,
    resolve_startup_page, startup_page_from_token, text_rendering_from_token,
    text_rendering_mode_from_token,
};
use crate::gpui_app::root::navigation::TopPage;
use taskforest_core::core::config::{
    COLOR_SCHEME_DARK, COLOR_SCHEME_EYEFOREST, COLOR_SCHEME_LIGHT, COLOR_SCHEME_SYSTEM, Config,
    STARTUP_PAGE_PERFORMANCE, STARTUP_PAGE_PROCESSES, STARTUP_PAGE_REMEMBER,
    TEXT_RENDERING_PLATFORM_DEFAULT,
};
use taskmanager_theme::{
    FONT_MISANS_VF, FONT_ROBOTO_MONO, FONT_TOKEN_SYSTEM, FontAvailability, FontChoice,
    FontPreference, HighContrast, LightDark, ModePreference, ResolvedFonts, Skin, Theme,
};

fn availability() -> FontAvailability {
    FontAvailability::from_installed_families(
        Skin::ALL
            .into_iter()
            .flat_map(|skin| [skin.ui_font(), skin.mono_font()])
            .chain([FONT_MISANS_VF, FONT_ROBOTO_MONO]),
    )
}

#[test]
fn page_tokens_round_trip_for_every_page() {
    for page in TopPage::ALL {
        let token = page_token(page);
        assert_eq!(
            page_from_token(token),
            page,
            "token {token} must round-trip"
        );
    }
}

#[test]
fn app_id_resolution_preserves_default_and_accepts_custom_composition_value() {
    assert_eq!(resolve_app_id(None), DEFAULT_APP_ID);
    assert_eq!(DEFAULT_APP_ID, "io.github.YellowWhiteBlackCat.TaskForestG");
    assert_eq!(
        resolve_app_id(Some("org.example.TaskManager".into())),
        "org.example.TaskManager"
    );
}

#[test]
fn unknown_or_empty_page_token_falls_back_to_performance() {
    assert_eq!(page_from_token(""), TopPage::Performance);
    assert_eq!(page_from_token("garbage"), TopPage::Performance);
    assert_eq!(
        page_from_token("  system  "),
        TopPage::System,
        "tokens are trimmed"
    );
}

/// The axis codec is the theme owner's (ADR-063); these are the resolutions
/// GPUI applies to a stored config, asserted here because the launch path depends
/// on them.
#[test]
fn skin_and_mode_tokens_are_trimmed_case_insensitive_with_aliases() {
    assert_eq!(Skin::from_config_token(" KDE "), Some(Skin::Kde));
    assert_eq!(Skin::from_config_token("windows"), Some(Skin::Windows));
    assert_eq!(Skin::from_config_token("win"), Some(Skin::Windows));
    assert_eq!(Skin::from_config_token("mac"), Some(Skin::Macos));
    assert_eq!(Skin::from_config_token(""), None);
    assert_eq!(Skin::from_config_token("plasma"), None);
    assert_eq!(
        ModePreference::from_config_token("LIGHT").explicit(),
        Some(LightDark::Light)
    );
    assert_eq!(
        ModePreference::from_config_token("dark").explicit(),
        Some(LightDark::Dark)
    );
    assert_eq!(
        ModePreference::from_config_token("Eye-Forest").explicit(),
        Some(LightDark::EyeForest)
    );
    assert_eq!(ModePreference::from_config_token("auto").explicit(), None);
}

/// An unrecorded or unknown token must never displace the detected theme: the
/// launch path keeps the appearance it resolved from the desktop.
#[test]
fn apply_cfg_preserves_detected_axes_for_unrecorded_tokens() {
    let detected = Theme::build(
        Skin::Windows,
        LightDark::EyeForest,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Windows),
    );

    let mut untouched = detected;
    apply_cfg_to_theme(&mut untouched, &Config::default());
    assert_eq!(untouched.skin, Skin::Windows, "empty skin keeps detection");
    assert_eq!(untouched.mode, LightDark::EyeForest);
    assert!(!untouched.hc);

    let mut recorded = detected;
    let cfg = Config {
        skin: "KDE".to_string(),
        mode: COLOR_SCHEME_LIGHT.to_string(),
        hc: true,
        ..Config::default()
    };
    apply_cfg_to_theme(&mut recorded, &cfg);
    assert_eq!(
        (recorded.skin, recorded.mode, recorded.hc),
        (Skin::Kde, LightDark::Light, true)
    );

    let mut hostile = detected;
    let cfg = Config {
        skin: "cinnamon".to_string(),
        mode: "sepia".to_string(),
        ..Config::default()
    };
    apply_cfg_to_theme(&mut hostile, &cfg);
    assert_eq!(
        hostile.skin,
        Skin::Windows,
        "an unknown skin is not a palette"
    );
    assert_eq!(hostile.mode, LightDark::EyeForest);
}

#[test]
fn color_scheme_tokens_preserve_system_and_legacy_modes() {
    let resolve = |token: &str| ModePreference::from_config_token(token).config_token();
    assert_eq!(resolve("system"), COLOR_SCHEME_SYSTEM);
    assert_eq!(resolve("SYSTEM"), COLOR_SCHEME_SYSTEM);
    assert_eq!(resolve("light"), COLOR_SCHEME_LIGHT);
    assert_eq!(resolve("Dark"), COLOR_SCHEME_DARK);
    assert_eq!(resolve("eye-forest"), COLOR_SCHEME_EYEFOREST);
    assert_eq!(resolve(""), COLOR_SCHEME_SYSTEM);
    assert_eq!(
        resolve("future"),
        COLOR_SCHEME_SYSTEM,
        "a newer config cannot force a palette"
    );
}

#[test]
fn text_rendering_tokens_map_to_supported_gpui_modes_and_unknown_values_fall_back() {
    use gpui::TextRenderingMode;
    use taskforest_core::core::config::{TEXT_RENDERING_GRAYSCALE, TEXT_RENDERING_SUBPIXEL};
    assert_eq!(
        text_rendering_from_token(TEXT_RENDERING_SUBPIXEL),
        TEXT_RENDERING_SUBPIXEL
    );
    assert_eq!(
        text_rendering_from_token("GRAYSCALE"),
        TEXT_RENDERING_GRAYSCALE
    );
    assert_eq!(
        text_rendering_from_token(TEXT_RENDERING_GRAYSCALE),
        TEXT_RENDERING_GRAYSCALE
    );
    assert_eq!(
        text_rendering_mode_from_token(TEXT_RENDERING_SUBPIXEL),
        TextRenderingMode::Subpixel
    );
    assert_eq!(
        text_rendering_mode_from_token(TEXT_RENDERING_GRAYSCALE),
        TextRenderingMode::Grayscale
    );
    assert_eq!(
        text_rendering_from_token("cleartype"),
        TEXT_RENDERING_PLATFORM_DEFAULT,
        "a config from a newer version must never select an unresolvable mode"
    );
    assert_eq!(
        text_rendering_from_token(""),
        TEXT_RENDERING_PLATFORM_DEFAULT
    );
    assert_eq!(
        text_rendering_mode_from_token("future"),
        TextRenderingMode::PlatformDefault
    );
}

#[test]
fn startup_page_tokens_normalize_unknown_values_to_remember_last() {
    assert_eq!(
        startup_page_from_token(STARTUP_PAGE_REMEMBER),
        STARTUP_PAGE_REMEMBER
    );
    assert_eq!(
        startup_page_from_token(STARTUP_PAGE_PERFORMANCE),
        STARTUP_PAGE_PERFORMANCE
    );
    assert_eq!(
        startup_page_from_token(STARTUP_PAGE_PROCESSES),
        STARTUP_PAGE_PROCESSES
    );
    assert_eq!(
        startup_page_from_token("future-page"),
        STARTUP_PAGE_REMEMBER
    );
}

#[test]
fn startup_policy_resolves_fixed_pages_before_last_page_and_honors_deep_links() {
    assert_eq!(
        resolve_startup_page(STARTUP_PAGE_PERFORMANCE, "apps", TopPage::Services, false,),
        TopPage::Performance
    );
    assert_eq!(
        resolve_startup_page(
            STARTUP_PAGE_PROCESSES,
            "performance",
            TopPage::Services,
            false,
        ),
        TopPage::Apps
    );
    assert_eq!(
        resolve_startup_page(
            STARTUP_PAGE_REMEMBER,
            "services",
            TopPage::Performance,
            false
        ),
        TopPage::Services
    );
    assert_eq!(
        resolve_startup_page("future-page", "apps", TopPage::Performance, false),
        TopPage::Apps,
        "an unknown startup policy must fail closed to remember-last"
    );
    assert_eq!(
        resolve_startup_page(STARTUP_PAGE_PROCESSES, "performance", TopPage::System, true,),
        TopPage::System,
        "an explicit TM_PAGE deep-link must remain authoritative"
    );
}

#[test]
fn bundled_font_tokens_resolve_to_bundled_choice() {
    let bundled = Config {
        ui_font: FONT_ROBOTO_MONO.to_string(),
        mono_font: FONT_ROBOTO_MONO.to_string(),
        ..Config::default()
    };
    let pref =
        FontPreference::from_config_tokens(&bundled.ui_font, &bundled.mono_font, &availability());
    assert_eq!(pref.ui, FontChoice::Bundled);
    assert_eq!(pref.mono, FontChoice::Bundled);

    // MiSans VF stays loadable for the UI role so pre-2026-08 configs
    // keep working; it never became the mono face.
    let legacy = Config {
        ui_font: FONT_MISANS_VF.to_string(),
        mono_font: FONT_MISANS_VF.to_string(),
        ..Config::default()
    };
    let pref =
        FontPreference::from_config_tokens(&legacy.ui_font, &legacy.mono_font, &availability());
    assert_eq!(pref.ui, FontChoice::Bundled);
    assert_eq!(pref.mono, FontChoice::System);
}

/// The product default (2026-08 policy, user directive): an ABSENT font
/// token resolves to the BUNDLED product faces, never to the host's
/// system fonts. Explicit System persists via the dedicated token and
/// survives the round trip.
#[test]
fn an_absent_font_token_is_the_bundled_product_default() {
    let absent = Config::default();
    let pref =
        FontPreference::from_config_tokens(&absent.ui_font, &absent.mono_font, &availability());
    assert_eq!(pref.ui, FontChoice::Bundled);
    assert_eq!(pref.mono, FontChoice::Bundled);

    let explicit_system = Config {
        ui_font: FONT_TOKEN_SYSTEM.to_string(),
        mono_font: FONT_TOKEN_SYSTEM.to_string(),
        ..Config::default()
    };
    let pref = FontPreference::from_config_tokens(
        &explicit_system.ui_font,
        &explicit_system.mono_font,
        &availability(),
    );
    assert_eq!(pref.ui, FontChoice::System);
    assert_eq!(pref.mono, FontChoice::System);
}

#[test]
fn unknown_font_tokens_fall_back_to_system() {
    let cfg = Config {
        ui_font: "Fira Code".to_string(),
        mono_font: "Comic Sans".to_string(),
        ..Config::default()
    };
    let pref = FontPreference::from_config_tokens(&cfg.ui_font, &cfg.mono_font, &availability());
    assert_eq!(pref.ui, FontChoice::System);
    assert_eq!(pref.mono, FontChoice::System);
}

#[test]
fn an_observed_system_family_round_trips_as_a_custom_choice() {
    let availability =
        FontAvailability::from_installed_families([FONT_MISANS_VF, FONT_ROBOTO_MONO, "Fira Code"]);
    let cfg = Config {
        ui_font: "Fira Code".to_string(),
        mono_font: FONT_ROBOTO_MONO.to_string(),
        ..Config::default()
    };
    let pref = FontPreference::from_config_tokens(&cfg.ui_font, &cfg.mono_font, &availability);
    assert_eq!(pref.ui, FontChoice::Custom("Fira Code"));
    assert_eq!(pref.mono, FontChoice::Bundled);
}
