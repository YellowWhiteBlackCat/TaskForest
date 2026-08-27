use super::{
    DEFAULT_APP_ID, FontAvailability, font_pref_from_config, mode_from_token, page_from_token,
    page_token, resolve_app_id, resolve_startup_page, skin_from_token, startup_page_from_token,
    text_rendering_from_token,
};
use super::{
    FONT_MISANS_VF, FONT_ROBOTO_MONO, FontChoice, LightDark, Skin, TEXT_RENDERING_PLATFORM_DEFAULT,
    color_scheme_from_token,
};
use crate::core::config::{
    COLOR_SCHEME_DARK, COLOR_SCHEME_EYEFOREST, COLOR_SCHEME_LIGHT, COLOR_SCHEME_SYSTEM, Config,
    STARTUP_PAGE_PERFORMANCE, STARTUP_PAGE_PROCESSES, STARTUP_PAGE_REMEMBER,
};
use crate::gpui_app::root::navigation::TopPage;

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

#[test]
fn skin_and_mode_tokens_are_trimmed_case_insensitive_with_aliases() {
    assert_eq!(skin_from_token(" KDE "), Some(Skin::Kde));
    assert_eq!(skin_from_token("windows"), Some(Skin::Windows));
    assert_eq!(skin_from_token("win"), Some(Skin::Windows));
    assert_eq!(skin_from_token("mac"), Some(Skin::Macos));
    assert_eq!(skin_from_token(""), None);
    assert_eq!(skin_from_token("plasma"), None);
    assert_eq!(mode_from_token("LIGHT"), Some(LightDark::Light));
    assert_eq!(mode_from_token("dark"), Some(LightDark::Dark));
    assert_eq!(mode_from_token("Eye-Forest"), Some(LightDark::EyeForest));
    assert_eq!(mode_from_token("auto"), None);
}

#[test]
fn color_scheme_tokens_preserve_system_and_legacy_modes() {
    assert_eq!(color_scheme_from_token("system"), COLOR_SCHEME_SYSTEM);
    assert_eq!(color_scheme_from_token("SYSTEM"), COLOR_SCHEME_SYSTEM);
    assert_eq!(color_scheme_from_token("light"), COLOR_SCHEME_LIGHT);
    assert_eq!(color_scheme_from_token("Dark"), COLOR_SCHEME_DARK);
    assert_eq!(
        color_scheme_from_token("eye-forest"),
        COLOR_SCHEME_EYEFOREST
    );
    assert_eq!(color_scheme_from_token(""), COLOR_SCHEME_SYSTEM);
    assert_eq!(color_scheme_from_token("future"), COLOR_SCHEME_SYSTEM);
}

#[test]
fn every_text_rendering_token_falls_back_to_platform_default_until_gpui_supports_it() {
    use crate::core::config::{TEXT_RENDERING_GRAYSCALE, TEXT_RENDERING_SUBPIXEL};
    assert_eq!(
        text_rendering_from_token(TEXT_RENDERING_SUBPIXEL),
        TEXT_RENDERING_PLATFORM_DEFAULT
    );
    assert_eq!(
        text_rendering_from_token("GRAYSCALE"),
        TEXT_RENDERING_PLATFORM_DEFAULT
    );
    assert_eq!(
        text_rendering_from_token(TEXT_RENDERING_GRAYSCALE),
        TEXT_RENDERING_PLATFORM_DEFAULT
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
    let pref = font_pref_from_config(&bundled, &availability());
    assert_eq!(pref.ui, FontChoice::Bundled);
    assert_eq!(pref.mono, FontChoice::Bundled);

    // MiSans VF stays loadable for the UI role so pre-2026-08 configs
    // keep working; it never became the mono face.
    let legacy = Config {
        ui_font: FONT_MISANS_VF.to_string(),
        mono_font: FONT_MISANS_VF.to_string(),
        ..Config::default()
    };
    let pref = font_pref_from_config(&legacy, &availability());
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
    let pref = font_pref_from_config(&absent, &availability());
    assert_eq!(pref.ui, FontChoice::Bundled);
    assert_eq!(pref.mono, FontChoice::Bundled);

    let explicit_system = Config {
        ui_font: super::FONT_TOKEN_SYSTEM.to_string(),
        mono_font: super::FONT_TOKEN_SYSTEM.to_string(),
        ..Config::default()
    };
    let pref = font_pref_from_config(&explicit_system, &availability());
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
    let pref = font_pref_from_config(&cfg, &availability());
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
    let pref = font_pref_from_config(&cfg, &availability);
    assert_eq!(pref.ui, FontChoice::Custom("Fira Code"));
    assert_eq!(pref.mono, FontChoice::Bundled);
}
