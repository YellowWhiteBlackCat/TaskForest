use crate::tokens::{MOTION_TOKEN_NONE_ALIAS, MotionPolicy, RowDensity, UiSize};

use super::*;

#[test]
fn every_axis_token_is_the_label_it_paints() {
    // One vocabulary per axis: the persisted token and the human-readable label
    // are the same string, so a config file names what the user saw.
    for skin in Skin::ALL {
        assert_eq!(skin.config_token(), skin.label());
        assert_eq!(Skin::from_config_token(skin.config_token()), Some(skin));
    }
    for mode in LightDark::ALL {
        assert_eq!(mode.config_token(), mode.label());
        assert_eq!(
            LightDark::from_config_token(mode.config_token()),
            Some(mode)
        );
        assert_eq!(
            ModePreference::Explicit(mode).config_token(),
            mode.config_token()
        );
    }
    for density in RowDensity::ALL {
        assert_eq!(
            RowDensity::from_config_token(density.config_token()),
            Some(density)
        );
    }
    for size in UiSize::ALL {
        assert_eq!(UiSize::from_config_token(size.config_token()), size);
    }
    for policy in MotionPolicy::ALL {
        assert_eq!(
            MotionPolicy::from_config_token(policy.config_token()),
            policy
        );
    }
}

#[test]
fn the_token_codec_is_case_and_padding_tolerant() {
    assert_eq!(Skin::from_config_token("  Windows "), Some(Skin::Windows));
    assert_eq!(Skin::from_config_token("win"), Some(Skin::Windows));
    assert_eq!(Skin::from_config_token("MAC"), Some(Skin::Macos));
    assert_eq!(
        LightDark::from_config_token("eye-forest"),
        Some(LightDark::EyeForest)
    );
    assert_eq!(
        ModePreference::from_config_token("LIGHT"),
        ModePreference::Explicit(LightDark::Light)
    );
    assert_eq!(
        RowDensity::from_config_token(" compact "),
        Some(RowDensity::Compact)
    );
    assert_eq!(
        MotionPolicy::from_config_token(MOTION_TOKEN_NONE_ALIAS),
        MotionPolicy::NoMotion
    );
    assert_eq!(
        MotionPolicy::from_config_token("Reduced"),
        MotionPolicy::Reduced
    );
}

#[test]
fn unknown_axis_tokens_fail_closed_instead_of_forcing_a_palette() {
    // Empty/unknown skin tokens keep the host detection; empty/unknown mode
    // tokens mean follow-native, never a fabricated palette.
    assert_eq!(Skin::from_config_token(""), None);
    assert_eq!(Skin::from_config_token("cinnamon"), None);
    assert_eq!(LightDark::from_config_token("sepia"), None);
    assert_eq!(LightDark::from_config_token(MODE_TOKEN_SYSTEM), None);
    assert_eq!(
        ModePreference::from_config_token(""),
        ModePreference::FollowNative
    );
    assert_eq!(
        ModePreference::from_config_token("auto"),
        ModePreference::FollowNative
    );
    // A density the product cannot express is ignored (the caller keeps its
    // geometry); motion and UI size resolve to their safe defaults.
    assert_eq!(RowDensity::from_config_token("double-spaced"), None);
    assert_eq!(
        MotionPolicy::from_config_token("whatever"),
        MotionPolicy::Normal
    );
    assert_eq!(UiSize::from_config_token("whatever"), UiSize::Standard);
}

#[test]
fn the_mode_picker_offers_the_three_palettes_then_follow_native() {
    // The strict codec is what lets a picker light no segment for a value the
    // axis cannot express, without the frontend matching tokens by hand.
    assert_eq!(
        ModePreference::from_saved_token("nope"),
        None,
        "an unknown token is not a saved preference"
    );
    assert_eq!(
        ModePreference::from_saved_token(""),
        Some(ModePreference::FollowNative)
    );
    assert_eq!(
        ModePreference::from_saved_token(" system "),
        Some(ModePreference::FollowNative)
    );
    assert_eq!(
        ModePreference::from_saved_token("eye-forest"),
        Some(ModePreference::Explicit(LightDark::EyeForest))
    );
    for choice in ModePreference::CHOICES {
        assert_eq!(
            ModePreference::from_saved_token(choice.config_token()),
            Some(choice),
            "every ladder member round-trips strictly"
        );
    }
    assert_eq!(
        ModePreference::CHOICES,
        [
            ModePreference::Explicit(LightDark::Light),
            ModePreference::Explicit(LightDark::Dark),
            ModePreference::Explicit(LightDark::EyeForest),
            ModePreference::FollowNative,
        ]
    );
    // Follow-native is a preference about provenance, so it never resolves to a
    // palette of its own and never displaces a pinned choice.
    assert_eq!(ModePreference::FollowNative.explicit(), None);
    assert!(ModePreference::FollowNative.follows_native());
    assert!(!ModePreference::Explicit(LightDark::Dark).follows_native());
    assert_eq!(
        ModePreference::Explicit(LightDark::EyeForest).resolve(Some(LightDark::Dark)),
        LightDark::EyeForest
    );
    assert_eq!(
        ModePreference::FollowNative.resolve(Some(LightDark::Light)),
        LightDark::Light
    );
    assert_eq!(
        ModePreference::FollowNative.resolve(None),
        LightDark::Dark,
        "a product with no observation yet must not wake a user in a bright palette"
    );
    assert_eq!(
        ModePreference::from(LightDark::Dark).explicit(),
        Some(LightDark::Dark)
    );
}

#[test]
fn the_appearance_override_parses_one_shared_vocabulary() {
    for (value, expected) in [
        ("gnome-dark", (Skin::Gnome, LightDark::Dark)),
        ("KDE-Light", (Skin::Kde, LightDark::Light)),
        ("win-dark", (Skin::Windows, LightDark::Dark)),
        ("windows-light", (Skin::Windows, LightDark::Light)),
        ("mac-dark", (Skin::Macos, LightDark::Dark)),
        ("macos-eyeforest", (Skin::Macos, LightDark::EyeForest)),
        ("gnome-eye-forest", (Skin::Gnome, LightDark::EyeForest)),
    ] {
        let override_appearance = AppearanceOverride::parse(value, false)
            .unwrap_or_else(|| panic!("{value} must resolve"));
        assert_eq!(
            (override_appearance.skin, override_appearance.mode),
            expected
        );
        assert!(!override_appearance.high_contrast);
    }
    // A malformed or unknown request resolves nothing rather than a wrong skin.
    for value in [
        "",
        "gnome",
        "gnome-",
        "gnome-system",
        "cinnamon-dark",
        "x-y",
    ] {
        assert_eq!(AppearanceOverride::parse(value, true), None, "{value}");
    }
    assert_eq!(
        AppearanceOverride::parse("kde-light", true).map(AppearanceOverride::axes),
        Some((Skin::Kde, LightDark::Light, true))
    );
    // The override vocabulary is exactly the persisted token vocabulary.
    for skin in Skin::ALL {
        assert_eq!(
            AppearanceOverride::parse(&format!("{}-dark", skin.config_token()), false)
                .map(|forced| forced.skin),
            Some(skin)
        );
    }
}

#[test]
fn the_override_environment_names_are_public() {
    // The capture harness and every frontend read the same two variables; the
    // strings live here so a rename is a one-place edit with a mechanical proof.
    assert_eq!(SKIN_OVERRIDE_ENV, "TM_SKIN");
    assert_eq!(HIGH_CONTRAST_OVERRIDE_ENV, "TM_SKIN_HC");
}
