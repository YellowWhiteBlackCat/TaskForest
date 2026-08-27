//! The neutral skin registry mapped onto iced colors (ADR-026, ADR-027).
//!
//! The iced frontend uses the SAME design source as the GPUI shell and the
//! TUI — `taskmanager-theme`'s skin tables — and converts at this edge.
//! The iced `Theme` is built via `Theme::custom` with the skin's palette;
//! widgets that need explicit styling read the neutral `Theme` snapshot
//! directly through [`color`] and the style helpers, never hardcoding a
//! terminal/desktop literal.

use iced::Font;
use iced::font::{Family, Weight};
use iced::widget::{button, container};
use taskmanager_theme::color::mix;
use taskmanager_theme::tokens::RowDensity;
use taskmanager_theme::{Color, FONT_MISANS_VF, Theme, tokens};

/// Map one neutral sRGB token onto an iced color (alpha preserved — iced
/// paints with alpha).
#[must_use]
pub fn color(c: Color) -> iced::Color {
    iced::Color::from_rgba(c.r, c.g, c.b, c.a)
}

/// The iced theme for one neutral theme snapshot: `Theme::custom` with the
/// skin's backdrop/text/accent/status tokens. Widget default styles then
/// inherit the skin's semantic colors; explicit styles read the same tokens
/// through the helpers below.
#[must_use]
pub fn iced_theme(theme: &Theme) -> iced::Theme {
    let palette = theme.palette();
    iced::Theme::custom(
        "taskmanager",
        iced::theme::Palette {
            background: color(palette.window_backdrop),
            text: color(palette.fg),
            primary: color(palette.accent),
            success: color(palette.success),
            warning: color(palette.warning),
            danger: color(palette.danger),
        },
    )
}

/// The bundled UI face as an iced [`Font`]. Used as the application builder's
/// `default_font` so every `text()` without an explicit font renders the same
/// MiSans VF face the GPUI shell uses for product UI.
pub const BUNDLED_UI_FONT: Font = Font::with_name(FONT_MISANS_VF);

/// The iced [`Font`] for the UI role, read from the neutral theme's resolved
/// family. Honors the persisted System/Bundled UI-font preference for any
/// `text()` that applies it explicitly. The builder's `default_font`
/// ([`BUNDLED_UI_FONT`]) covers the bulk of UI text; this is the live,
/// preference-aware handle for call sites that rebuild with the theme.
#[must_use]
pub fn ui_font(theme: &Theme) -> Font {
    Font::with_name(theme.ui_font)
}

/// The iced [`Font`] for the monospace role, read from the neutral theme's
/// resolved family. Applied to tabular numerals — tables, gauges, the per-core
/// grid, chart labels — so digits align the way GPUI's mono face does. Honors
/// the persisted mono-font preference.
#[must_use]
pub fn mono_font(theme: &Theme) -> Font {
    Font::with_name(theme.mono_font)
}

/// The resolved UI family at a specific [`Weight`] — the type-weight hierarchy
/// GPUI renders for section titles, headings and big readouts (Semibold/Bold).
/// Only the weight changes; the family stays the resolved UI family.
#[must_use]
pub fn ui_font_weight(theme: &Theme, weight: Weight) -> Font {
    Font {
        family: Family::Name(theme.ui_font),
        weight,
        ..Font::DEFAULT
    }
}

/// Panel surface style (dialog/tooltip/card family): the skin's elevated
/// surface fill with the skin's border and corner radius.
#[must_use]
pub fn panel_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(color(palette.surface).into()),
        text_color: Some(color(palette.fg)),
        border: iced::Border {
            color: color(palette.border),
            width: 1.0,
            radius: f32::from(palette.panel_radius).into(),
        },
        ..container::Style::default()
    }
}

/// The modal-scrim fill: the window backdrop darkened toward black so the
/// panel reads as the elevated layer above a dimmed page. Token-derived
/// (ADR-017 — a blend of existing tokens, never a literal).
#[must_use]
pub fn scrim_style(theme: &Theme) -> container::Style {
    scrim_style_with(theme, 1.0)
}

/// [`scrim_style`] with the modal-entrance progress (0..1): the darkening
/// blends from transparent to its full token value as the modal appears.
#[must_use]
pub fn scrim_style_with(theme: &Theme, appear: f32) -> container::Style {
    let appear = appear.clamp(0.0, 1.0);
    container::Style {
        background: Some(
            color(mix(
                theme.palette().window_backdrop,
                Color::BLACK,
                0.45 * appear,
            ))
            .into(),
        ),
        ..container::Style::default()
    }
}

/// Elevated panel style: [`panel_style`] plus a soft token-derived shadow —
/// Mission-Center-style depth for the modal panels. Iced 0.14's wgpu pipeline
/// renders container shadows as real box shadows (the `Quad` carries
/// shadow color/offset/blur into the shader), so this needs no self-drawn
/// approximation. The offset/blur are fixed layout values (the `px(...)`
/// contract); the color derives from the neutral black token.
#[must_use]
pub fn elevated_style(theme: &Theme) -> container::Style {
    elevated_style_with(theme, 1.0)
}

/// [`elevated_style`] with the modal-entrance progress (0..1): the panel
/// background fades in and the shadow softens until fully visible.
#[must_use]
pub fn elevated_style_with(theme: &Theme, appear: f32) -> container::Style {
    let appear = appear.clamp(0.0, 1.0);
    let mut style = panel_style(theme);
    style.background = style.background.map(|background| match background {
        iced::Background::Color(mut color) => {
            color.a *= appear;
            iced::Background::Color(color)
        }
        other => other,
    });
    style.shadow = iced::Shadow {
        color: color(Color::BLACK.with_alpha(0.30 * appear)),
        offset: iced::Vector::new(0.0, 6.0),
        blur_radius: 18.0,
    };
    style
}

/// Card surface style: [`panel_style`] plus a subtle shadow — the quiet lift
/// the Performance-page graph cards wear on the page backdrop. Same iced
/// shadow pipeline as [`elevated_style`], deliberately fainter so modals
/// still read as the highest elevation.
#[must_use]
pub fn card_style(theme: &Theme) -> container::Style {
    container::Style {
        shadow: iced::Shadow {
            color: color(Color::BLACK.with_alpha(0.16)),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
        ..panel_style(theme)
    }
}

/// Solid-fill container style — the composition-bar segments, the legend
/// swatches, and the bar tracks. The color is pre-resolved (an iced `Color`)
/// so the caller owns it and the style closure stays `'static`.
#[must_use]
pub fn fill_style(color: iced::Color) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(color)),
        ..container::Style::default()
    }
}

/// Whether the data row at zero-based `index` wears the zebra stripe: even
/// rows stripe, odd rows stay on the plain backdrop. The single parity seam
/// every inventory table (processes / services / startup / users) styles its
/// rows through, so the striping rule is unit-tested headlessly in one place.
#[must_use]
pub(crate) fn zebra_index(index: usize) -> bool {
    index.is_multiple_of(2)
}

/// Selected-row / hover-row surface style (accent tint from the theme).
/// `zebra` (from zebra_index) adds the theme's derived `zebra_bg` tint
/// (a faint fg wash — lightens on dark skins, darkens on light skins); a
/// selected row always wins over the stripe so selection stays the strongest
/// surface.
#[must_use]
pub fn row_style(theme: &Theme, selected: bool, zebra: bool) -> container::Style {
    let palette = theme.palette();
    let background = if selected {
        palette.selection
    } else if zebra {
        theme.zebra_bg()
    } else {
        palette.window_backdrop
    };
    container::Style {
        background: Some(color(background).into()),
        text_color: Some(color(palette.fg)),
        ..container::Style::default()
    }
}

/// Accent-filled action button (primary actions, destructive variants).
#[must_use]
pub fn button_style(theme: &Theme, destructive: bool) -> button::Style {
    button_style_status(theme, destructive, button::Status::Active)
}

/// The primary button's accent-gradient background: the shared
/// `gradient_from`→`gradient_to` token pair as a 180° (top→bottom) linear
/// gradient — the same Mission-Center primary-button wash the GPUI component
/// layer renders. Iced paints quad gradients natively on BOTH renderers (the
/// wgpu instance pipeline; tiny-skia converts to a real linear gradient), so
/// this needs no self-drawn approximation. Pointer states blend BOTH stops
/// exactly as the solid fill blended, keeping every state token-derived
/// (ADR-017: no color literals).
#[must_use]
pub fn accent_gradient(theme: &Theme, status: button::Status) -> iced::Background {
    let palette = theme.palette();
    let (from, to) = match status {
        button::Status::Hovered => (
            mix(palette.gradient_from, palette.hover, 0.28),
            mix(palette.gradient_to, palette.hover, 0.28),
        ),
        button::Status::Pressed => (
            mix(palette.gradient_from, Color::BLACK, 0.22),
            mix(palette.gradient_to, Color::BLACK, 0.22),
        ),
        _ => (palette.gradient_from, palette.gradient_to),
    };
    iced::Background::Gradient(iced::Gradient::Linear(
        iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
            .add_stop(0.0, color(from))
            .add_stop(1.0, color(to)),
    ))
}

/// The focus-ring stroke width (2px, inside the shared 1.5–2px focus-ring
/// contract the GPUI component layer draws).
pub const FOCUS_RING_WIDTH: f32 = 2.0;

/// The focus-ring stroke color for a focused control: the shared ring token
/// ([`taskmanager_theme::Palette::ring`], the accent-derived focus hue)
/// rendered fully opaque so the ring keeps its contrast over every skin
/// surface.
///
/// Contrast gap with the GPUI shell, stated honestly: GPUI encodes the
/// per-frame focus-visible decision in the ring token's alpha (alpha = 0 →
/// pointer-driven render draws no ring; its strict `InputModality` source
/// knows which input last moved focus). Iced 0.14 has no focus-visible
/// source at all — its stock button carries no focused status — so the
/// focusable wrapper treats EVERY focus as the keyboard-visible case and
/// forces the alpha opaque. `destructive` controls keep ringing in the
/// danger token, the same irreversible-action affordance the wrapper has
/// always drawn.
#[must_use]
pub fn focus_ring_color(theme: &Theme, destructive: bool) -> iced::Color {
    let palette = theme.palette();
    if destructive {
        return color(palette.danger);
    }
    let ring = color(palette.ring);
    iced::Color { a: 1.0, ..ring }
}

/// Accent-filled action button with pointer-state variants. The primary
/// (non-destructive) variant wears the accent gradient; the destructive
/// variant keeps the solid danger fill so irreversible actions read flat,
/// never decorative.
#[must_use]
pub fn button_style_status(
    theme: &Theme,
    destructive: bool,
    status: button::Status,
) -> button::Style {
    let palette = theme.palette();
    let background = if destructive {
        let fill = match status {
            button::Status::Hovered => mix(palette.danger, palette.hover, 0.28),
            button::Status::Pressed => mix(palette.danger, Color::BLACK, 0.22),
            _ => palette.danger,
        };
        iced::Background::Color(color(fill))
    } else {
        accent_gradient(theme, status)
    };
    button::Style {
        background: Some(background),
        text_color: color(theme.accent_text),
        border: iced::Border {
            color: iced::Color::TRANSPARENT,
            width: 0.0,
            radius: f32::from(palette.control_radius).into(),
        },
        ..button::Style::default()
    }
}

/// Quiet surface button (toolbar / secondary actions): transparent fill, the
/// skin's border, and a token hover/pressed fill. The text color stays the
/// skin foreground so these read as secondary next to accent-filled actions.
#[must_use]
pub fn ghost_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.palette();
    let background = match status {
        button::Status::Hovered => Some(color(palette.hover).into()),
        button::Status::Pressed => {
            Some(color(mix(palette.hover, palette.window_backdrop, 0.45)).into())
        }
        _ => None,
    };
    button::Style {
        background,
        text_color: color(palette.fg),
        border: iced::Border {
            color: color(palette.border),
            width: 1.0,
            radius: f32::from(palette.control_radius).into(),
        },
        ..button::Style::default()
    }
}

/// Performance device-rail card surface: transparent at idle, the token hover
/// fill while the pointer is over it, and the accent-tinted selection fill
/// with an accent border while the card is the active device — the rail
/// counterpart of [`row_style`]'s selected/hover surfaces, read through an
/// iced button so the pointer states stay toolkit-native. Selection always
/// wins over hover so the active device reads unambiguously.
#[must_use]
pub fn device_row_button_style(
    theme: &Theme,
    selected: bool,
    status: button::Status,
) -> button::Style {
    let palette = theme.palette();
    let background = if selected {
        Some(color(palette.selection).into())
    } else {
        match status {
            button::Status::Hovered | button::Status::Pressed => Some(color(palette.hover).into()),
            _ => None,
        }
    };
    button::Style {
        background,
        text_color: color(palette.fg),
        border: iced::Border {
            color: if selected {
                color(palette.accent)
            } else {
                iced::Color::TRANSPARENT
            },
            width: if selected { 1.0 } else { 0.0 },
            radius: f32::from(palette.control_radius).into(),
        },
        ..button::Style::default()
    }
}

/// Clickable table-header button: transparent at rest, a token hover/pressed
/// fill, and accent foreground on the active sort column so the click target
/// reads alongside the ▲/▼ marker projected by `crate::ui::sort_arrow`. All
/// states stay token-derived (ADR-017: no color literals).
#[must_use]
pub fn header_button_style(theme: &Theme, status: button::Status, active: bool) -> button::Style {
    let palette = theme.palette();
    let background = match status {
        button::Status::Hovered => Some(color(palette.hover).into()),
        button::Status::Pressed => {
            Some(color(mix(palette.hover, palette.window_backdrop, 0.45)).into())
        }
        _ => None,
    };
    button::Style {
        background,
        text_color: color(if active { palette.accent } else { palette.fg }),
        border: iced::Border {
            color: iced::Color::TRANSPARENT,
            width: 0.0,
            radius: f32::from(palette.control_radius).into(),
        },
        ..button::Style::default()
    }
}

/// Section-title text color: the skin's muted foreground (used for grouped
/// labels inside panels).
#[must_use]
pub fn muted_text_color(theme: &Theme) -> iced::Color {
    color(theme.palette().fg_muted)
}

/// Status tint for health/state text: the skin's semantic status token for
/// one health bucket. Typed here so views never hardcode a status color.
#[must_use]
pub fn status_color(theme: &Theme, healthy: bool) -> iced::Color {
    let palette = theme.palette();
    if healthy {
        color(palette.success)
    } else {
        color(palette.danger)
    }
}

/// Vertical + horizontal table-row padding for one density. The vertical
/// axis reads the SHARED `RowDensity` geometry (`row_padding_y()`, the same
/// 6.0/2.0 the GPUI table spends), so both frontends agree on row height;
/// the horizontal gutter stays a spacing-token projection (density is a
/// row-height metric, but the iced tables have always paired it with a
/// tighter horizontal gutter in compact mode).
#[must_use]
pub fn row_padding_density(density: RowDensity) -> iced::Padding {
    let horizontal = match density {
        RowDensity::Compact => tokens::SPACE_4,
        RowDensity::Comfortable => tokens::SPACE_8,
    };
    iced::Padding::from([f32::from(density.row_padding_y()), f32::from(horizontal)])
}

/// The boolean-density seam over [`row_padding_density`]: the persisted
/// `density` preference (`"Comfortable"` / `"Compact"`) as the legacy call
/// sites spell it. A thin wrapper so every existing call site keeps its
/// signature while the geometry itself flows through the shared token.
#[must_use]
pub fn row_padding(compact: bool) -> iced::Padding {
    row_padding_density(if compact {
        RowDensity::Compact
    } else {
        RowDensity::Comfortable
    })
}

/// The fixed gutter between columns in every shared table (header AND every
/// row builder — they must agree or the header drifts off its column).
/// iced's default row spacing is 0, which renders adjacent fixed-width cells
/// edge-to-edge; the same SPACE_8 gutter the gpui table uses as per-cell
/// padding restores column-by-column readability.
#[must_use]
pub fn table_column_spacing() -> f32 {
    f32::from(tokens::SPACE_8)
}

#[cfg(test)]
#[path = "../tests/gui/theme_tests.rs"]
mod tests;
