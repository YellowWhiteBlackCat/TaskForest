use super::*;
use crate::tokens::{DURATION_HOVER, MotionPolicy};
use crate::{HighContrast, LightDark, ResolvedFonts, Skin};
use gpui::WindowBackgroundAppearance;
use gpui::{DefiniteLength, Length};
use std::time::Duration;

fn color() -> Color {
    Color::from_hex(0x3584e4)
}

#[test]
fn color_maps_onto_gpui_color_types() {
    let rgba: Rgba = color().into();
    assert_eq!(rgba.r, 0x35 as f32 / 255.0);
    assert_eq!(rgba.g, 0x84 as f32 / 255.0);
    assert_eq!(rgba.b, 0xe4 as f32 / 255.0);
    assert_eq!(rgba.a, 1.0);

    let hsla: Hsla = color().into();
    assert_eq!(hsla, Hsla::from(rgba));

    let fill: Fill = color().into();
    assert!(fill.color().is_some());
}

#[test]
fn alpha_survives_into_gpui() {
    let rgba: Rgba = Color::from_hex(0x222226).with_alpha(0.55).into();
    assert_eq!(rgba.a, 0.55);
}

#[test]
fn length_maps_onto_gpui_length_types() {
    let length = Length(8.0);
    assert_eq!(Pixels::from(length), px(8.0));
    assert_eq!(
        AbsoluteLength::from(length),
        AbsoluteLength::Pixels(px(8.0))
    );
    assert_eq!(
        DefiniteLength::from(length),
        DefiniteLength::Absolute(AbsoluteLength::Pixels(px(8.0)))
    );
    assert_eq!(
        Length::from(length),
        Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(8.0))))
    );
}

#[test]
fn ratio_maps_to_relative_definite_length() {
    assert_eq!(
        DefiniteLength::from(Ratio(1.4)),
        DefiniteLength::Fraction(1.4)
    );
}

#[test]
fn weight_maps_to_font_weight() {
    #[cfg(target_os = "windows")]
    assert_eq!(FontWeight::from(Weight(450.0)), FontWeight(500.0));
    #[cfg(not(target_os = "windows"))]
    assert_eq!(FontWeight::from(Weight(450.0)), FontWeight(450.0));
    assert_eq!(FontWeight::from(Weight(600.0)), FontWeight(600.0));
}

/// The decision order pinned (same semantics as the pre-ADR-026
/// `Theme::background_appearance`): material > CSD transparency > opaque.
#[test]
fn background_appearance_tracks_material_and_window_transparency() {
    for skin in [Skin::Gnome, Skin::Kde] {
        let t = Theme::build(
            skin,
            LightDark::Dark,
            HighContrast::Off,
            ResolvedFonts::system_for(skin),
        );
        assert_eq!(
            background_appearance(&t),
            WindowBackgroundAppearance::Opaque
        );
        assert_eq!(t.material, Material::Opaque);
    }

    let mut linux_csd = Theme::dark();
    linux_csd.window_transparent = true;
    assert_eq!(
        background_appearance(&linux_csd),
        WindowBackgroundAppearance::Transparent
    );

    for skin in [Skin::Windows, Skin::Macos] {
        let t = Theme::build(
            skin,
            LightDark::Dark,
            HighContrast::Off,
            ResolvedFonts::system_for(skin),
        );
        assert_eq!(
            background_appearance(&t),
            WindowBackgroundAppearance::Blurred
        );
        assert_ne!(t.material, Material::Opaque);
    }
}

/// The CSD transparency flag must not downgrade a Mica/Vibrancy skin to
/// plain Transparent: the material keeps priority (startup decision order).
#[test]
fn background_appearance_skips_transparency_flag_on_material_skins() {
    let mut win = Theme::build(
        Skin::Windows,
        LightDark::Light,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Windows),
    );
    win.window_transparent = true;
    assert_eq!(
        background_appearance(&win),
        WindowBackgroundAppearance::Blurred
    );
}

#[test]
fn builders_use_the_theme_durations() {
    assert_eq!(fade_in().duration, DURATION_FAST);
    assert_eq!(appear().duration, DURATION_MEDIUM);
}

#[test]
fn policy_animation_never_constructs_zero_duration_gpui_animation() {
    assert_eq!(
        motion_animation(MotionPolicy::Normal, DURATION_HOVER).map(|animation| animation.duration),
        Some(DURATION_HOVER)
    );
    assert_eq!(
        motion_animation(MotionPolicy::Reduced, DURATION_MEDIUM)
            .map(|animation| animation.duration),
        Some(DURATION_FAST)
    );
    assert!(motion_animation(MotionPolicy::NoMotion, DURATION_MEDIUM).is_none());
    assert!(motion_animation(MotionPolicy::Normal, Duration::ZERO).is_none());
    assert_eq!(
        fade_in_for(MotionPolicy::Reduced).map(|animation| animation.duration),
        Some(DURATION_FAST)
    );
    assert!(appear_for(MotionPolicy::NoMotion).is_none());
}
