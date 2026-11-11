use super::*;
use crate::theme_binding::{BUNDLED_UI_FONT, mono_font, ui_font, ui_font_weight};
use iced::Font;
use taskmanager_theme::tokens::RowDensity;
use taskmanager_theme::{
    FONT_MISANS_VF, GlassSurfaceRole, GlassTier, HighContrast, LightDark, ResolvedFonts, Skin,
};
use taskmanager_ui_contract::presentation::sizes::{ControlRole, ControlState};

fn combined_shadow_alpha(ink_alpha: f32, opacity: f32) -> f32 {
    let ambient = ink_alpha * opacity * 0.6;
    let contact = ink_alpha * opacity;
    1.0 - (1.0 - ambient) * (1.0 - contact)
}

fn gradient_stops(background: Option<iced::Background>) -> Vec<(f32, iced::Color)> {
    let Some(iced::Background::Gradient(iced::Gradient::Linear(gradient))) = background else {
        panic!("control glass must paint a linear gradient");
    };
    gradient
        .stops
        .iter()
        .flatten()
        .map(|stop| (stop.offset, stop.color))
        .collect()
}

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
    // Weighted helper preserves the resolved UI family, changes only weight;
    // the neutral weight token quantizes onto iced's ladder (Bold = 700).
    let bold = ui_font_weight(&theme, tokens::FONT_WEIGHT_BOLD);
    assert_eq!(bold.family, iced::font::Family::Name("Adwaita Sans"));
    assert_eq!(bold.weight, iced::font::Weight::Bold);
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
fn performance_rail_and_controls_scale_the_theme_radii() {
    for skin in Skin::ALL {
        let theme = Theme::build(
            skin,
            LightDark::Light,
            HighContrast::Off,
            ResolvedFonts::system_for(skin),
        )
        .with_root_rem_size(21.0);
        let scale = theme.root_rem_size / STANDARD_ROOT_REM_SIZE;
        let panel = f32::from(theme.palette().panel_radius) * scale;
        let control = f32::from(theme.palette().control_radius) * scale;
        let rail = stats_rail_style(&theme);
        assert!((rail.border.radius.top_left - panel).abs() < 1e-4);
        assert_eq!(rail.background, Some(glass_fill(&theme, GlassTier::Card)));
        let chart = crate::ui::components::chart_surface(&theme);
        assert_eq!(chart.shadow, iced::Shadow::default());
        assert_eq!(chart.border, rail.border);
        assert!((progress_style(&theme).border.radius.top_left - control).abs() < 1e-4);
        assert!(
            (crate::ui::components::controls::state::button_style(
                &theme,
                ControlRole::Button,
                crate::ui::components::ButtonTone::Secondary,
                crate::ui::components::controls::state::ControlPaintState::Active,
            )
            .border
            .radius
            .top_left
                - control)
                .abs()
                < 1e-4
        );
    }
}

#[test]
fn navigation_chrome_uses_the_sidebar_glass_tier() {
    let theme = Theme::dark();
    let chrome = chrome_style(&theme);
    assert_eq!(
        chrome.background,
        Some(glass_fill(&theme, GlassTier::Sidebar))
    );
    assert_eq!(chrome.border.color, color(theme.glass_chrome().edge));
    assert_eq!(chrome.shadow, iced::Shadow::default());
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

/// The focused control rings in the shared ring token, and the ring follows
/// the shared focus-visible contract: the ring token's alpha encodes the
/// keyboard-vs-pointer decision from the per-window theme snapshot — the same
/// strict keyboard-only policy the GPUI root tracker drives. Destructive
/// controls keep the danger ring under the same visibility rule, and the
/// stroke stays inside the shared 1.5–2px focus-ring contract.
#[test]
fn focus_ring_consumes_the_shared_ring_token_and_follows_focus_visible() {
    let theme = Theme::dark();
    let ring = color(theme.palette().ring);
    // The neutral snapshot carries the pointer-focus alpha (0) — the iced
    // shell now synthesizes the visibility decision on top of it instead of
    // forcing every focus opaque.
    assert_eq!(theme.palette().ring.a, 0.0);

    // Pointer-driven focus draws no ring (alpha 0), hue unchanged.
    let pointer_theme = theme.with_focus_visible(false);
    let pointer_stroke = focus_ring_color(&pointer_theme, false);
    assert_eq!(
        (pointer_stroke.r, pointer_stroke.g, pointer_stroke.b),
        (ring.r, ring.g, ring.b),
        "the ring hue comes from palette().ring"
    );
    assert_eq!(pointer_stroke.a, 0.0, "pointer focus ⇒ no ring");

    // A keyboard press makes the next focus paint the ring opaquely.
    let keyboard_theme = theme.with_focus_visible(true);
    let stroke = focus_ring_color(&keyboard_theme, false);
    assert_eq!((stroke.r, stroke.g, stroke.b), (ring.r, ring.g, ring.b));
    assert_eq!(stroke.a, 1.0, "keyboard focus ⇒ ring is drawn opaquely");

    // Destructive controls keep the danger hue under the same gate.
    let danger = color(theme.palette().danger);
    let destructive = focus_ring_color(&keyboard_theme, true);
    assert_eq!(
        (destructive.r, destructive.g, destructive.b),
        (danger.r, danger.g, danger.b)
    );
    assert_eq!(destructive.a, 1.0);
    assert_eq!(focus_ring_color(&pointer_theme, true).a, 0.0);

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

/// The Mission-Center elevation stack is token-derived: the scrim dims the
/// window backdrop toward black, the elevated modal carries a real shadow
/// (iced 0.14 wgpu renders container shadows — no self-drawn approximation),
/// and the card shadow is strictly fainter so modals stay the highest
/// elevation. Iced can paint one shadow quad, so the projection folds the
/// theme's ambient + contact layers into one alpha-weighted shadow instead of
/// silently dropping the contact layer. Every color traces back to a theme
/// token, never a literal.
#[test]
fn scrim_elevated_and_card_styles_are_token_derived_and_layered() {
    let theme = Theme::dark();
    let palette = theme.palette();

    let scrim = scrim_style(&theme);
    assert_eq!(
        scrim.background,
        Some(color(theme.glass_scrim().fill).into()),
        "the modal mask consumes the theme's scrim tier"
    );

    let elevated = elevated_style(&theme);
    let modal_shadow = theme.glass_modal().shadow;
    let ambient_alpha = modal_shadow.a * 0.6;
    let contact_alpha = modal_shadow.a;
    assert_eq!(
        elevated.shadow.color,
        iced::Color {
            a: combined_shadow_alpha(modal_shadow.a, 1.0),
            ..color(modal_shadow)
        }
    );
    let modal_ambient = GlassTier::Modal.ambient_shadow_rem();
    let modal_contact = GlassTier::Modal.contact_shadow_rem();
    let expected_offset = (modal_ambient.0 * ambient_alpha + modal_contact.0 * contact_alpha)
        / (ambient_alpha + contact_alpha);
    let expected_blur = (modal_ambient.1 * ambient_alpha + modal_contact.1 * contact_alpha)
        / (ambient_alpha + contact_alpha);
    let expected_offset = expected_offset * theme.root_rem_size;
    let expected_blur = expected_blur * theme.root_rem_size;
    assert!((elevated.shadow.offset.y - expected_offset).abs() < 1e-5);
    assert!((elevated.shadow.blur_radius - expected_blur).abs() < 1e-5);
    assert_eq!(elevated.border.color, color(theme.glass_modal().edge));
    assert_eq!(
        elevated.border.width,
        theme.root_rem_size * GlassTier::Modal.edge_width_rem()
    );
    assert_eq!(
        elevated.border.radius,
        f32::from(palette.panel_radius).into()
    );

    let card = card_style(&theme);
    let card_shadow = theme.glass_card().shadow;
    let card_ambient_alpha = card_shadow.a * 0.6;
    let card_contact_alpha = card_shadow.a;
    assert_eq!(
        card.shadow.color,
        iced::Color {
            a: combined_shadow_alpha(card_shadow.a, 0.7),
            ..color(card_shadow)
        }
    );
    assert!(
        card.shadow.color.a < elevated.shadow.color.a,
        "cards must read as lower elevation than modals"
    );
    let card_ambient = GlassTier::Card.ambient_shadow_rem();
    let card_contact = GlassTier::Card.contact_shadow_rem();
    let expected_offset = (card_ambient.0 * card_ambient_alpha
        + card_contact.0 * card_contact_alpha)
        / (card_ambient_alpha + card_contact_alpha);
    let expected_blur = (card_ambient.1 * card_ambient_alpha + card_contact.1 * card_contact_alpha)
        / (card_ambient_alpha + card_contact_alpha);
    let expected_offset = expected_offset * theme.root_rem_size;
    let expected_blur = expected_blur * theme.root_rem_size;
    assert!((card.shadow.offset.y - expected_offset).abs() < 1e-5);
    assert!((card.shadow.blur_radius - expected_blur).abs() < 1e-5);
    assert_eq!(card.background, Some(glass_fill(&theme, GlassTier::Card)));
}

#[test]
fn glass_styles_paint_the_shared_gradient_stops() {
    for mode in LightDark::ALL {
        let theme = Theme::build(
            Skin::Gnome,
            mode,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        );
        let iced::Background::Gradient(iced::Gradient::Linear(gradient)) =
            card_style(&theme).background.expect("card background")
        else {
            panic!("the card must paint a gradient");
        };
        let stops: Vec<_> = gradient
            .stops
            .iter()
            .flatten()
            .map(|stop| (stop.offset, stop.color))
            .collect();
        let [top, bottom] = theme.glass_gradient(GlassTier::Card);
        assert_eq!(stops, vec![(0.0, color(top)), (1.0, color(bottom))]);
    }
}

#[test]
fn semantic_surface_styles_follow_the_shared_role_mapping() {
    let theme = Theme::dark();
    for role in GlassSurfaceRole::ALL {
        let style = glass_surface_style(&theme, role);
        let tier = role.tier();
        assert_eq!(
            style.background,
            Some(glass_fill(&theme, tier)),
            "{role:?} uses its shared glass tier fill"
        );
        assert_eq!(
            style.border,
            glass_border(&theme, tier),
            "{role:?} uses its shared glass tier edge"
        );
        assert_eq!(
            style.shadow,
            if role.elevation().casts_shadow() {
                glass_shadow(&theme, tier)
            } else {
                iced::Shadow::default()
            },
            "{role:?} follows the shared floating/inset contract"
        );
    }
}

#[test]
fn high_contrast_mode_strengthens_iced_borders_and_text() {
    let standard = Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    );
    let hc = Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::On,
        ResolvedFonts::system_for(Skin::Gnome),
    );
    assert!(hc.hc);
    let standard_border = panel_style(&standard).border.color;
    let hc_border = panel_style(&hc).border.color;
    assert_ne!(
        standard_border, hc_border,
        "high contrast must produce a distinct, stronger border in Iced"
    );
    assert_eq!(hc_border.a, 1.0, "high contrast border is fully opaque");
}

#[test]
fn scrollbar_geometry_contract_aligns_with_bevy_and_gpui() {
    let std_bar = scrollbar();
    // 6px width + 2px margin + 2px end spacing for full-page views
    let compact_bar = compact_scrollbar();
    // 4px width + 1px margin + 1px end spacing for nested cards
    assert_ne!(std_bar, compact_bar);
}

#[test]
fn scrollbar_style_implements_capsule_and_translucent_rail() {
    let theme = Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    );
    let iced = iced_theme(&theme);

    // Active state with overflowing content (both enabled)
    let active_status = scrollable::Status::Active {
        is_vertical_scrollbar_disabled: false,
        is_horizontal_scrollbar_disabled: false,
    };
    let style = scrollbar_style(&iced, active_status);

    // Both rails and scrollers wear the 3.0px capsule radius
    assert_eq!(style.vertical_rail.border.radius, 3.0.into());
    assert_eq!(style.vertical_rail.scroller.border.radius, 3.0.into());
    assert_eq!(style.horizontal_rail.border.radius, 3.0.into());
    assert_eq!(style.horizontal_rail.scroller.border.radius, 3.0.into());

    // Rails float with a subtle translucent wash
    let v_bg = style
        .vertical_rail
        .background
        .expect("active rail has background");
    match v_bg {
        iced::Background::Color(c) => {
            assert!(
                c.a > 0.0 && c.a <= 0.15,
                "rail must be quiet and translucent"
            );
        }
        _ => panic!("rail background should be solid color"),
    }

    // Scroller thumb is translucent and legible
    match style.vertical_rail.scroller.background {
        iced::Background::Color(c) => {
            assert!(
                c.a >= 0.25 && c.a <= 0.60,
                "thumb must have moderate translucency"
            );
        }
        _ => panic!("scroller background should be solid color"),
    }

    // Disabled state (no overflowing content) hides rail and scroller
    let disabled_status = scrollable::Status::Active {
        is_vertical_scrollbar_disabled: true,
        is_horizontal_scrollbar_disabled: true,
    };
    let disabled_style = scrollbar_style(&iced, disabled_status);
    assert!(disabled_style.vertical_rail.background.is_none());
    assert_eq!(
        disabled_style.vertical_rail.scroller.background,
        iced::Background::Color(iced::Color::TRANSPARENT)
    );

    // Compact style wears a 2.0px radius for 4px tracks
    let compact = compact_scrollbar_style(&iced, active_status);
    assert_eq!(compact.vertical_rail.border.radius, 2.0.into());
    assert_eq!(compact.vertical_rail.scroller.border.radius, 2.0.into());
}

/// The control-glass seam spends the SAME card glass tokens as the floating
/// panels: two theme gradient stops, the shared specular edge, and the
/// single-quad projection of the two-layer shadow. No control owns a second
/// palette, radius ladder, or shadow authority.
#[test]
fn control_glass_reads_the_shared_card_glass_and_shadow_projection() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = Theme::build(
                skin,
                mode,
                HighContrast::Off,
                ResolvedFonts::system_for(skin),
            );
            let active = control_glass(&theme, ControlRole::Input, ControlVisualState::Active);
            let panel = card_glass_style(&theme);
            assert_eq!(active.background, panel.background);
            assert_eq!(active.border, panel.border);
            assert_eq!(active.shadow, panel.shadow);
            assert_eq!(active.text_color, panel.text_color);
            assert_eq!(
                gradient_stops(active.background),
                stops_from_theme(&theme, GlassTier::Card)
            );
            assert_eq!(control_divider_color(&theme), panel.border.color);
        }
    }
}

fn stops_from_theme(theme: &Theme, tier: GlassTier) -> Vec<(f32, iced::Color)> {
    let [top, bottom] = theme.glass_gradient(tier);
    vec![(0.0, color(top)), (1.0, color(bottom))]
}

/// Hover/focus/pressed/disabled change PAINT only: the shared
/// `ControlState` contract says geometry is invariant, and every state still
/// resolves its colors through theme tokens. A focused control paints the
/// shared ring; a pressed one sinks; a disabled one dims and never keeps the
/// focused ring.
#[test]
fn control_glass_states_are_token_derived_and_geometry_invariant() {
    let theme = Theme::dark().with_focus_visible(true);
    let active = control_glass(&theme, ControlRole::Input, ControlVisualState::Active);
    let hovered = control_glass(&theme, ControlRole::Input, ControlVisualState::Hovered);
    let pressed = control_glass(&theme, ControlRole::Input, ControlVisualState::Pressed);
    let focused = control_glass(&theme, ControlRole::Input, ControlVisualState::Focused);
    let disabled = control_glass(&theme, ControlRole::Input, ControlVisualState::Disabled);
    let pointer_theme = theme.with_focus_visible(false);
    let pointer_focused = control_glass(
        &pointer_theme,
        ControlRole::Input,
        ControlVisualState::Focused,
    );

    assert_eq!(hovered.border, active.border);
    assert_eq!(pressed.border, active.border);
    assert_eq!(disabled.border.radius, active.border.radius);
    assert_ne!(
        gradient_stops(hovered.background),
        gradient_stops(active.background)
    );
    assert_ne!(
        gradient_stops(pressed.background),
        gradient_stops(active.background)
    );
    assert!(
        hovered.shadow.blur_radius == active.shadow.blur_radius
            && hovered.shadow.offset.y == active.shadow.offset.y,
        "the shadow geometry stays exactly the shared projection's"
    );
    assert!(
        pressed.shadow.color.a < active.shadow.color.a,
        "a pressed control sinks visually"
    );

    assert_eq!(focused.border.color, focus_ring_color(&theme, false));
    assert_eq!(focused.border.width, FOCUS_RING_WIDTH);
    // Pointer focus keeps the resting glass edge and the resting width: the
    // ring token's alpha is zero, so no ring is painted at all.
    assert_eq!(pointer_focused.border.color, active.border.color);
    assert_eq!(pointer_focused.border.width, active.border.width);

    assert_eq!(disabled.text_color, Some(color(theme.palette().fg_muted)));
    assert!(
        disabled.shadow.color.a < active.shadow.color.a,
        "disabled controls lose elevation"
    );
    assert_ne!(disabled.border.color, focused.border.color);
    assert_ne!(
        gradient_stops(disabled.background),
        gradient_stops(active.background)
    );

    assert_eq!(
        ControlVisualState::from_contract(ControlState::Compact),
        ControlVisualState::Active
    );
    assert_eq!(
        ControlVisualState::from_contract(ControlState::Disabled),
        ControlVisualState::Disabled
    );
    assert!(!ControlState::Focused.changes_measurement());
    assert!(!ControlState::Disabled.changes_measurement());
}

/// Badges and tooltips are non-interactive presentation roles: they keep the
/// glass material but cast no elevation shadow, so compact slots do not
/// accumulate overlapping depth washes.
#[test]
fn non_interactive_control_glass_keeps_the_material_without_elevation() {
    let theme = Theme::dark();
    let badge = control_glass(&theme, ControlRole::Badge, ControlVisualState::Active);
    let tooltip = control_glass(&theme, ControlRole::Tooltip, ControlVisualState::Active);
    assert_eq!(badge.shadow, iced::Shadow::default());
    assert!(tooltip.shadow.color.a > 0.0);
    assert_eq!(badge.background, card_glass_style(&theme).background);
}
