use super::*;

#[test]
fn unknown_native_facts_use_safe_presentation_fallbacks() {
    let appearance = NativeAppearance::default();

    assert_eq!(detect_skin(appearance), Skin::Gnome);
    assert_eq!(detect_mode(appearance), LightDark::Light);
    assert_eq!(detect_high_contrast(appearance), HighContrast::Off);
}

#[test]
fn native_facts_map_without_os_commands_in_the_frontend() {
    let appearance = NativeAppearance {
        family: Some(Skin::Kde),
        scheme: Some(LightDark::Dark),
        high_contrast: Some(true),
    };

    assert_eq!(detect_skin(appearance), Skin::Kde);
    assert_eq!(detect_mode(appearance), LightDark::Dark);
    assert_eq!(detect_high_contrast(appearance), HighContrast::On);
}

#[test]
fn unobserved_facts_never_guess_a_confirmed_value() {
    // A light scheme that was NOT observed must not flip a dark-capable
    // presentation: None → the light fallback, exactly like Unknown.
    let unobserved = NativeAppearance {
        scheme: None,
        ..NativeAppearance::default()
    };
    assert_eq!(detect_mode(unobserved), LightDark::Light);

    // high_contrast None and Some(false) both mean "not confirmed on".
    assert_eq!(
        detect_high_contrast(NativeAppearance {
            high_contrast: Some(false),
            ..NativeAppearance::default()
        }),
        HighContrast::Off,
    );
}

#[test]
fn every_native_family_maps_to_its_skin() {
    for skin in Skin::ALL {
        let appearance = NativeAppearance {
            family: Some(skin),
            ..NativeAppearance::default()
        };
        assert_eq!(detect_skin(appearance), skin);
    }
}
