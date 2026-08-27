use super::parse_skin;
use taskmanager_theme::{LightDark, Skin};

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
        assert_eq!(parse_skin(input), Some(expected), "input {input}");
    }
}

#[test]
fn unknown_skin_or_mode_is_not_a_valid_override() {
    assert_eq!(parse_skin("plasma-dark"), None, "unknown skin");
    assert_eq!(parse_skin("kde-vibes"), None, "unknown mode");
    assert_eq!(parse_skin("kde"), None, "missing mode separator");
    assert_eq!(parse_skin("kde-dark-extra"), None, "extra segment");
    assert_eq!(parse_skin(""), None, "empty input");
}
