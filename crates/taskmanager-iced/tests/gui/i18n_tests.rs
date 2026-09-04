use super::*;

#[test]
fn every_language_renders_every_key_non_empty_and_stable() {
    for language in Language::ALL {
        for key in [
            Key::Settings,
            Key::Containers,
            Key::Health,
            Key::WindowCapture,
            Key::Export,
            Key::About,
            Key::Close,
            Key::Appearance,
            Key::Skin,
            Key::Mode,
            Key::HighContrast,
            Key::On,
            Key::Off,
            Key::Fonts,
            Key::Density,
            Key::Units,
            Key::Language,
            Key::Version,
            Key::SystemInfo,
            Key::AlertRules,
            Key::DeviceSummary,
            Key::ExportDone,
            Key::ExportNoData,
            Key::ExportFailed,
            Key::ContainersUnavailable,
            Key::ContainersUnsupported,
            Key::ContainersPermissionDenied,
            Key::ContainersNoContainers,
            Key::ContainersWaiting,
            Key::ContainersHeader,
            Key::HealthWaiting,
            Key::HealthNoData,
            Key::DetailsTitle,
            Key::Confirm,
            Key::Cancel,
            Key::Light,
            Key::Dark,
            Key::EyeForest,
            Key::System,
            Key::Bundled,
            Key::Comfortable,
            Key::Compact,
            Key::Bytes,
            Key::Bits,
            Key::Default,
            Key::Subpixel,
            Key::Grayscale,
            Key::RememberLast,
            Key::Performance,
            Key::Applications,
        ] {
            let text = t(language, key);
            assert!(!text.is_empty(), "{key:?} missing for {language:?}");
            // Same key, same language → identical text (determinism).
            assert_eq!(text, t(language, key));
        }
    }
}

#[test]
fn zh_translations_differ_from_english_where_localized() {
    assert_ne!(
        t(Language::En, Key::Settings),
        t(Language::Zh, Key::Settings)
    );
    assert_eq!(t(Language::En, Key::Settings), "Settings");
    assert_eq!(t(Language::Zh, Key::Settings), "设置");
    assert_eq!(t(Language::En, Key::Containers), "Containers");
    assert_eq!(t(Language::Zh, Key::Containers), "容器");
    assert_eq!(t(Language::En, Key::WindowCapture), "Capture window");
    assert_eq!(t(Language::Zh, Key::WindowCapture), "截图当前窗口");
    // Language self-names stay in their own tongue.
    assert_eq!(Language::En.label(), "English");
    assert_eq!(Language::Zh.label(), "中文");
}

#[test]
fn key_codes_are_stable_locale_identifiers() {
    assert_eq!(Key::Settings.code(), "settings.title");
    assert_eq!(Key::Close.code(), "chrome.close");
    assert_eq!(Key::WindowCapture.code(), "window_capture.capture");
    assert_eq!(
        Key::ContainersNoContainers.code(),
        "containers.no_containers"
    );
}

/// The persisted `Config::language` token round-trips through every
/// supported language, and unknown/empty tokens stay `None` (the
/// first-launch "no preference" sentinel, G-22).
#[test]
fn language_tokens_round_trip_and_unknown_tokens_keep_the_default() {
    for language in Language::ALL {
        assert_eq!(Language::from_token(language.token()), Some(language));
    }
    assert_eq!(Language::from_token(""), None);
    assert_eq!(Language::from_token("fr"), None);
    assert_eq!(
        Language::from_token("EN"),
        None,
        "tokens are case-sensitive"
    );
    assert_eq!(Language::token(Language::En), "en");
    assert_eq!(Language::token(Language::Zh), "zh");
}
