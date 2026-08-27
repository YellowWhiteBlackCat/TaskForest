use super::*;

#[test]
fn desktop_family_parser_handles_composite_session_names() {
    assert_eq!(parse_desktop_family("ubuntu:GNOME"), DesktopFamily::Gnome);
    assert_eq!(
        parse_desktop_family("wayland;KDE Plasma"),
        DesktopFamily::Kde
    );
    assert_eq!(parse_desktop_family("sway"), DesktopFamily::Unknown);
}

#[test]
fn color_scheme_parsers_preserve_unknown() {
    assert_eq!(
        parse_color_scheme("'prefer-dark'"),
        Some(PreferredColorScheme::Dark)
    );
    assert_eq!(
        parse_kde_color_scheme("[General]\nColorScheme=BreezeLight\n"),
        Some(PreferredColorScheme::Light)
    );
    assert_eq!(parse_color_scheme("follow-wallpaper"), None);
}

#[test]
fn high_contrast_parser_distinguishes_false_from_unknown() {
    assert_eq!(parse_bool("true"), Some(true));
    assert_eq!(parse_bool("false"), Some(false));
    assert_eq!(parse_bool("not-a-bool"), None);
}
