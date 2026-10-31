use super::*;

#[test]
fn unknown_preferences_do_not_masquerade_as_confirmed_values() {
    let appearance = DesktopAppearance::default();

    assert_eq!(appearance.family, DesktopFamily::Unknown);
    assert_eq!(appearance.color_scheme, PreferredColorScheme::Unknown);
    assert_eq!(appearance.high_contrast, None);
}
