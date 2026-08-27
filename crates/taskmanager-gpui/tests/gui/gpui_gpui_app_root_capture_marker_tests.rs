use super::*;

#[test]
fn capture_theme_tokens_cover_all_product_modes_without_platform_build_variants() {
    assert_eq!(theme_token(Skin::Gnome, LightDark::Light), "gnome-light");
    assert_eq!(theme_token(Skin::Gnome, LightDark::Dark), "gnome-dark");
    assert_eq!(
        theme_token(Skin::Gnome, LightDark::EyeForest),
        "gnome-eyeforest"
    );
    assert_eq!(theme_token(Skin::Kde, LightDark::Light), "kde-light");
    assert_eq!(theme_token(Skin::Kde, LightDark::Dark), "kde-dark");
    assert_eq!(
        theme_token(Skin::Windows, LightDark::Light),
        "windows-light"
    );
    assert_eq!(theme_token(Skin::Windows, LightDark::Dark), "windows-dark");
    assert_eq!(theme_token(Skin::Macos, LightDark::Light), "macos-light");
    assert_eq!(theme_token(Skin::Macos, LightDark::Dark), "macos-dark");
}
