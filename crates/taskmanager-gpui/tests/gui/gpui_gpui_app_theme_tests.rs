use taskmanager_theme::{AppearanceOverride, LightDark, Skin};

#[test]
fn every_skin_and_mode_parses_case_insensitively() {
    for (input, expected) in [
        ("gnome-dark", (Skin::Gnome, LightDark::Dark)),
        ("kde-light", (Skin::Kde, LightDark::Light)),
        ("win-dark", (Skin::Windows, LightDark::Dark)),
        ("windows-dark", (Skin::Windows, LightDark::Dark)),
        ("mac-light", (Skin::Macos, LightDark::Light)),
        ("macos-light", (Skin::Macos, LightDark::Light)),
        ("gnome-eyeforest", (Skin::Gnome, LightDark::EyeForest)),
        ("kde-eye-forest", (Skin::Kde, LightDark::EyeForest)),
        ("GNOME-DARK", (Skin::Gnome, LightDark::Dark)),
        ("Win-Light", (Skin::Windows, LightDark::Light)),
    ] {
        assert_eq!(
            AppearanceOverride::parse(input, false).map(|forced| (forced.skin, forced.mode)),
            Some(expected),
            "input {input}"
        );
    }
}

#[test]
fn unknown_skin_or_mode_is_not_a_valid_override() {
    assert_eq!(
        AppearanceOverride::parse("plasma-dark", false),
        None,
        "unknown skin"
    );
    assert_eq!(
        AppearanceOverride::parse("kde-vibes", false),
        None,
        "unknown mode"
    );
    assert_eq!(
        AppearanceOverride::parse("kde", false),
        None,
        "missing mode separator"
    );
    assert_eq!(
        AppearanceOverride::parse("kde-dark-extra", false),
        None,
        "extra segment"
    );
    assert_eq!(AppearanceOverride::parse("", false), None, "empty input");
}
