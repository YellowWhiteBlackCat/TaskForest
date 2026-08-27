use super::*;
use taskmanager_theme::tokens::RowDensity;
use taskmanager_theme::{FONT_MISANS_VF, HighContrast, LightDark, ResolvedFonts, Skin};

/// Font helpers read the resolved family names off the neutral theme, the
/// weighted helper preserves the family, and the bundled const pins the
/// builder default to the bundled face.
#[test]
fn font_helpers_read_resolved_theme_families() {
    let theme = Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts {
            ui: "Adwaita Sans",
            mono: "Adwaita Mono",
        },
    );
    assert_eq!(ui_font(&theme), Font::with_name("Adwaita Sans"));
    assert_eq!(mono_font(&theme), Font::with_name("Adwaita Mono"));
    // The builder default is the bundled MiSans VF face.
    assert_eq!(BUNDLED_UI_FONT, Font::with_name(FONT_MISANS_VF));
    // Weighted helper preserves the resolved UI family, changes only weight.
    let bold = ui_font_weight(&theme, Weight::Bold);
    assert_eq!(bold.family, Family::Name("Adwaita Sans"));
    assert_eq!(bold.weight, Weight::Bold);
    assert_eq!(bold.stretch, Font::DEFAULT.stretch);
    assert_eq!(bold.style, Font::DEFAULT.style);
}

/// Every skin × mode resolves a usable iced theme with the semantic
/// distinctions the view relies on.
#[test]
fn every_skin_variant_resolves_an_iced_theme() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = Theme::build(
                skin,
                mode,
                HighContrast::Off,
                ResolvedFonts::system_for(skin),
            );
            let iced = iced_theme(&theme);
            let palette = iced.palette();
            let neutral = theme.palette();
            assert_eq!(palette.primary, color(neutral.accent));
            assert_eq!(palette.success, color(neutral.success));
            assert_eq!(palette.danger, color(neutral.danger));
        }
    }
}

#[test]
fn color_conversion_is_channel_exact() {
    assert_eq!(
        color(Color::from_hex(0x3584e4)),
        iced::Color::from_rgb(
            0x35 as f32 / 255.0,
            0x84 as f32 / 255.0,
            0xe4 as f32 / 255.0
        )
    );
    assert_eq!(color(Color::BLACK), iced::Color::BLACK);
    assert_eq!(color(Color::WHITE), iced::Color::WHITE);
    let tinted = Color::from_hex(0x222226).with_alpha(0.55);
    assert!((color(tinted).a - 0.55).abs() < 1e-4);
}

#[test]
fn panel_and_button_styles_read_theme_tokens() {
    let theme = Theme::dark();
    let panel = panel_style(&theme);
    assert_eq!(
        panel.background,
        Some(color(theme.palette().surface).into())
    );
    assert_eq!(panel.border.color, color(theme.palette().border));
    // The primary button wears the shared accent-gradient token pair (the
    // same wash the GPUI component layer renders); the destructive variant
    // keeps the flat danger fill.
    let primary = button_style(&theme, false);
    assert_eq!(
        primary.background,
        Some(accent_gradient(&theme, button::Status::Active))
    );
    let destructive = button_style(&theme, true);
    assert_eq!(
        destructive.background,
        Some(color(theme.palette().danger).into())
    );
}

#[test]
fn button_states_stay_token_derived() {
    let theme = Theme::dark();
    let palette = theme.palette();
    let hovered = button_style_status(&theme, false, button::Status::Hovered);
    let pressed = button_style_status(&theme, false, button::Status::Pressed);
    let active = button_style_status(&theme, false, button::Status::Active);

    // The active gradient is the 180° (top→bottom) sweep of the two shared
    // accent-gradient tokens — verified stop by stop, not by calling back
    // into the same builder.
    let iced::Background::Gradient(iced::Gradient::Linear(active_linear)) =
        active.background.expect("primary button has a background")
    else {
        panic!("primary button background must be the accent gradient");
    };
    assert_eq!(active_linear.angle, iced::Radians(std::f32::consts::PI));
    let active_stops: Vec<(f32, iced::Color)> = active_linear
        .stops
        .iter()
        .flatten()
        .map(|stop| (stop.offset, stop.color))
        .collect();
    assert_eq!(
        active_stops,
        vec![
            (0.0, color(palette.gradient_from)),
            (1.0, color(palette.gradient_to)),
        ],
        "gradient stops come from the shared token pair"
    );

    // Pointer states blend BOTH stops with the same token mixes the solid
    // fill used (ADR-017: token-derived, never literals).
    let expected_hover = iced::Background::Gradient(iced::Gradient::Linear(
        iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
            .add_stop(0.0, color(mix(palette.gradient_from, palette.hover, 0.28)))
            .add_stop(1.0, color(mix(palette.gradient_to, palette.hover, 0.28))),
    ));
    assert_eq!(hovered.background, Some(expected_hover));
    let expected_pressed = iced::Background::Gradient(iced::Gradient::Linear(
        iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
            .add_stop(0.0, color(mix(palette.gradient_from, Color::BLACK, 0.22)))
            .add_stop(1.0, color(mix(palette.gradient_to, Color::BLACK, 0.22))),
    ));
    assert_eq!(pressed.background, Some(expected_pressed));

    let ghost = ghost_button_style(&theme, button::Status::Active);
    assert_eq!(ghost.background, None);
    assert_eq!(ghost.text_color, color(palette.fg));
    let ghost_hovered = ghost_button_style(&theme, button::Status::Hovered);
    assert_eq!(ghost_hovered.background, Some(color(palette.hover).into()));
    // The ghost border is the skin border token, never a literal.
    assert_eq!(ghost.border.color, color(palette.border));
}

#[test]
fn status_and_density_helpers_use_only_theme_tokens() {
    let theme = Theme::dark();
    let palette = theme.palette();
    assert_eq!(status_color(&theme, true), color(palette.success));
    assert_eq!(status_color(&theme, false), color(palette.danger));
    assert_eq!(muted_text_color(&theme), color(palette.fg_muted));

    // Row padding maps through the shared RowDensity geometry (see
    // `row_padding_maps_the_shared_density_tokens` below).
    let comfortable = row_padding(false);
    let compact = row_padding(true);
    assert!(compact.top < comfortable.top);
}

/// Row padding consumes the SHARED density geometry: the vertical axis is
/// `RowDensity::row_padding_y()` (the same 6.0/2.0 the GPUI table spends), the
/// horizontal gutter stays the iced spacing-token projection, and the legacy
/// boolean seam is a thin wrapper over the density entry point.
#[test]
fn row_padding_maps_the_shared_density_tokens() {
    for density in RowDensity::ALL {
        let padding = row_padding_density(density);
        assert_eq!(
            padding.top,
            f32::from(density.row_padding_y()),
            "vertical padding comes from the shared density token"
        );
        assert_eq!(padding.bottom, padding.top);
    }
    let comfortable = row_padding_density(RowDensity::Comfortable);
    assert_eq!(comfortable.left, f32::from(tokens::SPACE_8));
    assert_eq!(comfortable.right, comfortable.left);
    let compact = row_padding_density(RowDensity::Compact);
    assert_eq!(compact.left, f32::from(tokens::SPACE_4));
    assert_eq!(compact.right, compact.left);

    // The legacy boolean seam forwards to the density entry point unchanged.
    assert_eq!(row_padding(false), comfortable);
    assert_eq!(row_padding(true), compact);
}

/// The focused control rings in the shared ring token: the hue is
/// `palette().ring` (the accent-derived focus color) rendered opaque — iced
/// has no focus-visible source, so EVERY focus draws the ring, unlike GPUI
/// where the ring token's alpha encodes the keyboard-vs-pointer decision.
/// Destructive controls keep the danger ring, and the stroke stays inside the
/// shared 1.5–2px focus-ring contract.
#[test]
fn focus_ring_consumes_the_shared_ring_token() {
    let theme = Theme::dark();
    let ring = color(theme.palette().ring);
    // The theme snapshot carries the pointer-focus alpha (0) — the iced
    // frontend consciously overrides it: focused ⇒ visible ring.
    assert_eq!(theme.palette().ring.a, 0.0);
    let stroke = focus_ring_color(&theme, false);
    assert_eq!(
        (stroke.r, stroke.g, stroke.b),
        (ring.r, ring.g, ring.b),
        "the ring hue comes from palette().ring"
    );
    assert_eq!(stroke.a, 1.0, "focused ⇒ ring is drawn opaquely");
    assert_eq!(
        focus_ring_color(&theme, true),
        color(theme.palette().danger)
    );
    assert!(
        (1.5..=2.0).contains(&FOCUS_RING_WIDTH),
        "the ring stroke stays inside the shared 1.5-2px contract"
    );
}

/// Zebra striping is the parity seam every inventory table (processes /
/// services / startup / users) styles its rows through: even 0-based rows
/// stripe, odd rows stay plain, and a selected row always beats the stripe
/// so selection remains the strongest surface.
#[test]
fn zebra_index_stripes_even_rows_and_selection_beats_the_stripe() {
    assert!(zebra_index(0), "the first data row stripes");
    assert!(!zebra_index(1));
    assert!(zebra_index(2));
    assert!(!zebra_index(3));
    assert!(zebra_index(1024));
    assert!(!zebra_index(1025));

    let theme = Theme::dark();
    let plain = row_style(&theme, false, false);
    assert_eq!(
        plain.background,
        Some(color(theme.palette().window_backdrop).into())
    );
    let striped = row_style(&theme, false, true);
    assert_eq!(
        striped.background,
        Some(color(theme.zebra_bg()).into()),
        "the stripe is the theme's derived zebra surface, never a literal"
    );
    let selected = row_style(&theme, true, true);
    assert_eq!(
        selected.background,
        Some(color(theme.palette().selection).into()),
        "selection overrides the stripe"
    );
    assert_eq!(
        striped.text_color, plain.text_color,
        "the stripe must not change the row foreground"
    );
}

/// The Mission-Center elevation stack is token-derived and layered: the
/// scrim dims the window backdrop toward black, the elevated modal carries
/// a real shadow (iced 0.14 wgpu renders container shadows — no
/// self-drawn approximation), and the card shadow is strictly fainter so
/// modals stay the highest elevation. Every color traces back to a theme
/// token, never a literal.
#[test]
fn scrim_elevated_and_card_styles_are_token_derived_and_layered() {
    let theme = Theme::dark();
    let palette = theme.palette();

    let scrim = scrim_style(&theme);
    assert_eq!(
        scrim.background,
        Some(color(mix(palette.window_backdrop, Color::BLACK, 0.45)).into())
    );

    let elevated = elevated_style(&theme);
    assert_eq!(elevated.shadow.color, color(Color::BLACK.with_alpha(0.30)));
    assert_eq!(elevated.shadow.offset, iced::Vector::new(0.0, 6.0));
    assert_eq!(elevated.shadow.blur_radius, 18.0);
    // The elevated panel keeps the card family's border + radius contract.
    assert_eq!(elevated.border.color, color(palette.border));
    assert_eq!(elevated.border.width, 1.0);
    assert_eq!(
        elevated.border.radius,
        f32::from(palette.panel_radius).into()
    );

    let card = card_style(&theme);
    assert_eq!(card.shadow.color, color(Color::BLACK.with_alpha(0.16)));
    assert!(
        card.shadow.color.a < elevated.shadow.color.a,
        "cards must read as lower elevation than modals"
    );
    assert!(card.shadow.blur_radius < elevated.shadow.blur_radius);
    assert_eq!(card.background, elevated.background);
}
