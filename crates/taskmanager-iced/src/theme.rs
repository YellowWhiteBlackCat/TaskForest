//! The neutral skin registry assembled onto iced styles (ADR-026, ADR-027,
//! CORE-07).
//!
//! The iced frontend uses the SAME design source as the GPUI shell and the
//! TUI — `taskmanager-theme`'s skin tables — and converts at this edge
//! through the theme crate's OWN `iced` binding module (the single
//! conversion source, CORE-07): call sites import the token→value functions
//! ([`crate::theme_binding::color`], `ui_font`, `mono_font`,
//! `ui_font_weight`, `BUNDLED_UI_FONT`) from that owner path directly, and
//! this module keeps only style ASSEMBLY — the `Theme::custom` palette and
//! the container/button styles composed from converted values — which is
//! frontend policy per ADR-026 rule 3. Widgets that need explicit styling
//! read the neutral `Theme` snapshot through those helpers, never
//! hardcoding a terminal/desktop literal.

use crate::theme_binding::color;
use iced::widget::{button, container, progress_bar, scrollable};
use taskmanager_theme::color::mix;
use taskmanager_theme::theme::STANDARD_ROOT_REM_SIZE;
use taskmanager_theme::tokens::RowDensity;
use taskmanager_theme::{
    Color, GlassShadowLayer, GlassShadowProjection, GlassTier, Length, Theme, tokens,
};
use taskmanager_ui_contract::presentation::sizes::{ControlRole, ControlState};

mod control_measurement;
pub use control_measurement::{
    control_extent, control_measurement, control_padding_x, ui_size_from_root,
};

/// The control/surface visual state Iced paints from the shared presentation
/// vocabulary. Geometry never changes with state: focus, hover, press and
/// disabled alter paint only, so a ring or a disabled tint cannot consume a
/// slot budget in a compact viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlVisualState {
    Active,
    Hovered,
    Pressed,
    Focused,
    Disabled,
}

impl ControlVisualState {
    #[must_use]
    pub const fn from_contract(state: ControlState) -> Self {
        match state {
            ControlState::Normal | ControlState::Compact => Self::Active,
            ControlState::Focused => Self::Focused,
            ControlState::Active => Self::Hovered,
            ControlState::Disabled => Self::Disabled,
        }
    }

    /// The shadow's paint weight for this state. Geometry (offset, blur) is
    /// NEVER reweighted: it stays exactly the shared projection's. Only the
    /// ink alpha changes, so a pressed control sinks and a disabled one
    /// loses elevation without a second shadow ladder.
    const fn shadow_alpha(self) -> f32 {
        match self {
            Self::Active | Self::Hovered | Self::Focused => 1.0,
            Self::Pressed => 0.55,
            Self::Disabled => 0.35,
        }
    }

    const fn sheen_progress(self) -> f32 {
        match self {
            Self::Active => 0.0,
            Self::Hovered | Self::Focused => 0.45,
            Self::Pressed => 0.18,
            Self::Disabled => 0.0,
        }
    }
}

/// One token-derived control surface: the shared Card glass gradient, its
/// specular edge, and the same single-quad projection of the two-layer glass
/// shadow the floating panels consume. Status only reweights those tokens;
/// no control owns a second shadow ladder or a hand-picked hue.
#[must_use]
pub fn control_glass(
    theme: &Theme,
    role: ControlRole,
    state: ControlVisualState,
) -> container::Style {
    let mut style = card_glass_style(theme);
    if role == ControlRole::Badge {
        style.shadow = iced::Shadow::default();
    }
    let sheen = theme.glass_sheen(GlassTier::Card);
    let progress = state.sheen_progress();
    if progress > 0.0 && sheen.a > 0.0 {
        let [top, bottom] = theme.glass_gradient(GlassTier::Card);
        style.background = Some(iced::Background::Gradient(iced::Gradient::Linear(
            iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                .add_stop(0.0, color(mix(top, sheen, progress)))
                .add_stop(1.0, color(mix(bottom, sheen, progress * 0.5))),
        )));
    }
    style.shadow.color = style.shadow.color.scale_alpha(state.shadow_alpha());
    if state == ControlVisualState::Disabled {
        style.border.color = color(mix(
            theme.glass_card().edge,
            theme.palette().window_backdrop,
            0.5,
        ));
        let base = color(theme.glass_card().fill);
        style.background = Some(iced::Background::Gradient(iced::Gradient::Linear(
            iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                .add_stop(0.0, base)
                .add_stop(1.0, base),
        )));
        style.text_color = Some(color(theme.palette().fg_muted));
    }
    if state == ControlVisualState::Focused {
        // The shared ring token carries the focus-visible decision in its
        // alpha: pointer-driven focus keeps the plain edge and the resting
        // width, keyboard focus paints the ring.
        let ring = focus_ring_color(theme, false);
        if ring.a > 0.0 {
            style.border.color = ring;
            style.border.width = FOCUS_RING_WIDTH;
        }
    }
    style
}

/// The hairline divider between connected control segments: the same glass
/// edge token as the floating surfaces, never a second border color.
#[must_use]
pub fn control_divider_color(theme: &Theme) -> iced::Color {
    color(theme.glass_card().edge)
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

/// Project the theme's glass sheen into Iced's native gradient fill.
#[must_use]
fn glass_fill(theme: &Theme, tier: GlassTier) -> iced::Background {
    let [top, bottom] = theme.glass_gradient(tier);
    iced::Background::Gradient(iced::Gradient::Linear(
        iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
            .add_stop(0.0, color(top))
            .add_stop(1.0, color(bottom)),
    ))
}

fn rem(theme: &Theme, value: f32) -> f32 {
    value * theme.root_rem_size
}

pub(crate) fn scaled_radius(theme: &Theme, radius: Length) -> f32 {
    rem(theme, f32::from(radius) / STANDARD_ROOT_REM_SIZE)
}

fn glass_border(theme: &Theme, tier: GlassTier) -> iced::Border {
    let palette = theme.palette();
    iced::Border {
        color: color(theme.glass_layer(tier).edge),
        width: rem(theme, tier.edge_width_rem()),
        radius: rem(
            theme,
            f32::from(palette.panel_radius) / STANDARD_ROOT_REM_SIZE,
        )
        .into(),
    }
}

fn glass_shadow_with(theme: &Theme, layer: GlassShadowLayer) -> iced::Shadow {
    iced::Shadow {
        color: color(layer.color),
        offset: iced::Vector::new(0.0, rem(theme, layer.offset_y)),
        blur_radius: rem(theme, layer.blur_radius),
    }
}

/// The tight contact layer of one glass tier: the small lift a raised
/// element wears (the switch knob). It is the shared shadow contract's own
/// contact layer, not a second shadow ladder.
#[must_use]
pub fn glass_contact_shadow(theme: &Theme, tier: GlassTier) -> iced::Shadow {
    glass_shadow_with(
        theme,
        tier.projected_shadow(theme, GlassShadowProjection::SingleQuad)
            .contact,
    )
}

/// Collapse the theme's ambient and contact shadow layers into the one shadow
/// quad Iced can paint. Alpha-weighted geometry preserves both token
/// dimensions without claiming a second layer the toolkit cannot draw.
fn glass_shadow(theme: &Theme, tier: GlassTier) -> iced::Shadow {
    let layers = tier.projected_shadow(theme, GlassShadowProjection::SingleQuad);
    let ambient = glass_shadow_with(theme, layers.ambient);
    let contact = glass_shadow_with(theme, layers.contact);
    let ambient_alpha = ambient.color.a;
    let contact_alpha = contact.color.a;
    let total_alpha = ambient_alpha + contact_alpha;
    if total_alpha <= f32::EPSILON {
        return iced::Shadow {
            color: iced::Color::TRANSPARENT,
            ..iced::Shadow::default()
        };
    }
    let combine = |ambient_value: f32, contact_value: f32| {
        (ambient_value * ambient_alpha + contact_value * contact_alpha) / total_alpha
    };
    iced::Shadow {
        color: iced::Color {
            a: 1.0 - (1.0 - ambient_alpha) * (1.0 - contact_alpha),
            ..ambient.color
        },
        offset: iced::Vector::new(0.0, combine(ambient.offset.y, contact.offset.y)),
        blur_radius: combine(ambient.blur_radius, contact.blur_radius),
    }
}

/// Panel surface style (dialog/tooltip/card family): the theme's card glass
/// fill, specular edge, radius, and two-layer shadow. This is the shared
/// floating-surface contract, not an opaque fallback.
#[must_use]
pub fn panel_style(theme: &Theme) -> container::Style {
    card_glass_style(theme)
}

/// Persistent navigation chrome: the sidebar glass layer with its own edge
/// and quieter depth. The toolbar inside the band uses ordinary control
/// surfaces, so the chrome remains one material layer.
#[must_use]
pub fn chrome_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(glass_fill(theme, GlassTier::Sidebar)),
        border: glass_border(theme, GlassTier::Sidebar),
        shadow: glass_shadow(theme, GlassTier::Sidebar),
        text_color: Some(color(palette.fg)),
        ..container::Style::default()
    }
}

/// The modal-scrim fill: the theme's own depth-wash tier so the panel reads
/// as the elevated layer above a dimmed page.
#[must_use]
pub fn scrim_style(theme: &Theme) -> container::Style {
    scrim_style_with(theme, 1.0)
}

/// [`scrim_style`] with the modal-entrance progress (0..1): the depth wash
/// fades from transparent to its full tier alpha as the modal appears.
#[must_use]
pub fn scrim_style_with(theme: &Theme, appear: f32) -> container::Style {
    let appear = appear.clamp(0.0, 1.0);
    let scrim = theme.glass_scrim().fill;
    container::Style {
        background: Some(color(scrim.with_alpha(scrim.a * appear)).into()),
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
    elevated_glass_style(theme)
}

/// [`elevated_style`] with the modal-entrance progress (0..1): the panel
/// background fades in and the shadow softens until fully visible.
#[must_use]
pub fn elevated_style_with(theme: &Theme, appear: f32) -> container::Style {
    // GLASS-01: entrance scales the modal's shadow rather than switching to an
    // opaque interim fill, so the settled state and the final animation frame
    // honor the same glass contract.
    let appear = appear.clamp(0.0, 1.0);
    let mut style = elevated_glass_style(theme);
    style.shadow.color = style.shadow.color.scale_alpha(appear);
    style
}

/// The Iced modal glass: the neutral modal layer plus the elevated depth.
/// The toolkit has no per-element backdrop blur, so the window-level
/// appearance plus the translucent fill provide the platform's best frosted
/// read; the token keeps one authority across renderers.
#[must_use]
pub fn elevated_glass_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    let fill = glass_fill(theme, GlassTier::Modal);
    container::Style {
        background: Some(fill),
        border: glass_border(theme, GlassTier::Modal),
        shadow: glass_shadow(theme, GlassTier::Modal),
        text_color: Some(color(palette.fg)),
        ..container::Style::default()
    }
}

/// Card surface style: [`panel_style`] plus a subtle shadow — the quiet lift
/// the Performance-page graph cards wear on the page backdrop. Same iced
/// shadow pipeline as [`elevated_style`], deliberately fainter so modals
/// still read as the highest elevation.
#[must_use]
pub fn card_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    let fill = glass_fill(theme, GlassTier::Card);
    container::Style {
        background: Some(fill),
        border: glass_border(theme, GlassTier::Card),
        shadow: glass_shadow(theme, GlassTier::Card),
        text_color: Some(color(palette.fg)),
        snap: true,
    }
}

/// The Iced card glass: the neutral card layer plus the product's quiet
/// depth. Used by floating panels and page cards that sit above a translucent
/// backdrop; dense table rows stay on opaque row tokens.
#[must_use]
pub fn card_glass_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    let fill = glass_fill(theme, GlassTier::Card);
    container::Style {
        background: Some(fill),
        border: glass_border(theme, GlassTier::Card),
        shadow: glass_shadow(theme, GlassTier::Card),
        text_color: Some(color(palette.fg)),
        ..container::Style::default()
    }
}

/// The recessed inset-panel surface: a well cut INTO the page surface, used
/// for content that belongs to an enclosing scope (the container's own
/// process block). The fill is the theme's window backdrop blended toward
/// the shadow side, so the panel reads as recessed on light and dark skins
/// alike; the border keeps the skin's hairline. Token-derived (ADR-017: no
/// color literals).
#[must_use]
pub fn inset_panel_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(color(mix(palette.window_backdrop, Color::BLACK, 0.22)).into()),
        text_color: Some(color(palette.fg)),
        border: iced::Border {
            color: color(palette.border),
            width: 1.0,
            radius: scaled_radius(theme, palette.small_radius).into(),
        },
        ..container::Style::default()
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
/// ([`taskmanager_theme::Palette::ring`], the accent-derived focus hue) with
/// the focus-visible decision applied — the shared palette contract encodes
/// that decision in the ring's alpha (alpha = 0 → no ring).
///
/// The visibility source is the per-window theme snapshot's
/// `focus_visible` bit: only keyboard input keeps the ring opaque, the same
/// strict policy the GPUI shell derives from its root input-modality tracker.
/// `destructive` controls ring in the danger token under the same visibility
/// rule — the irreversible-action affordance stays, and a pointer-driven
/// focus draws no ring on it either.
#[must_use]
pub fn focus_ring_color(theme: &Theme, destructive: bool) -> iced::Color {
    let palette = theme.palette();
    let mut ring = if destructive {
        color(palette.danger)
    } else {
        color(palette.ring)
    };
    ring.a = if theme.focus_visible() { 1.0 } else { 0.0 };
    ring
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
            radius: scaled_radius(theme, palette.control_radius).into(),
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
            radius: scaled_radius(theme, palette.control_radius).into(),
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
            radius: scaled_radius(theme, palette.control_radius).into(),
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
            radius: scaled_radius(theme, palette.control_radius).into(),
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

/// The Performance statistics rail uses the same card tier in both layouts.
#[must_use]
pub fn stats_rail_style(theme: &Theme) -> container::Style {
    card_glass_style(theme)
}

pub(crate) fn progress_style(theme: &Theme) -> progress_bar::Style {
    let palette = theme.palette();
    progress_bar::Style {
        background: color(palette.border).into(),
        bar: color(palette.accent).into(),
        border: iced::Border {
            radius: scaled_radius(theme, palette.control_radius).into(),
            ..iced::Border::default()
        },
    }
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

/// Standard scrollbar for full-page and inventory views (BEVY/GPUI parity):
/// 6.0px width, 2.0px margin from outer edge, 2.0px spacing.
/// Floating, capsule-proportioned layout affordance.
#[must_use]
pub fn scrollbar() -> scrollable::Scrollbar {
    scrollable::Scrollbar::new()
        .width(6.0)
        .scroller_width(6.0)
        .margin(2.0)
        .spacing(2.0)
}

/// Compact scrollbar for nested cards (Performance panels, small embedded lists):
/// 4.0px width, 1.0px margin, 1.0px spacing.
#[must_use]
pub fn compact_scrollbar() -> scrollable::Scrollbar {
    scrollable::Scrollbar::new()
        .width(4.0)
        .scroller_width(4.0)
        .margin(1.0)
        .spacing(1.0)
}

/// Product-owned scrollable style: translucent floating track + capsule thumb.
/// Fully token-derived, matching the Bevy UI and GPUI component layers.
#[must_use]
pub fn scrollbar_style(theme: &iced::Theme, status: scrollable::Status) -> scrollable::Style {
    build_scrollbar_style(theme, status, 3.0, 0.08, 0.38)
}

/// Compact scrollable style with tighter capsule radii for dense card viewports.
#[must_use]
pub fn compact_scrollbar_style(
    theme: &iced::Theme,
    status: scrollable::Status,
) -> scrollable::Style {
    build_scrollbar_style(theme, status, 2.0, 0.08, 0.38)
}

fn build_scrollbar_style(
    theme: &iced::Theme,
    status: scrollable::Status,
    radius: f32,
    rail_alpha: f32,
    thumb_alpha: f32,
) -> scrollable::Style {
    let palette = theme.palette();
    let (is_v_disabled, is_h_disabled, is_v_interacted, is_h_interacted) = match status {
        scrollable::Status::Active {
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
        } => (
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
            false,
            false,
        ),
        scrollable::Status::Hovered {
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
            is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_hovered,
        } => (
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
            is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_hovered,
        ),
        scrollable::Status::Dragged {
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
            is_vertical_scrollbar_dragged,
            is_horizontal_scrollbar_dragged,
        } => (
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
            is_vertical_scrollbar_dragged,
            is_horizontal_scrollbar_dragged,
        ),
    };

    let v_thumb_alpha = if is_v_interacted {
        (thumb_alpha * 1.5).min(0.75)
    } else {
        thumb_alpha
    };
    let v_rail_alpha = if is_v_interacted {
        (rail_alpha * 1.6).min(0.20)
    } else {
        rail_alpha
    };

    let h_thumb_alpha = if is_h_interacted {
        (thumb_alpha * 1.5).min(0.75)
    } else {
        thumb_alpha
    };
    let h_rail_alpha = if is_h_interacted {
        (rail_alpha * 1.6).min(0.20)
    } else {
        rail_alpha
    };

    let v_rail = scrollable::Rail {
        background: if is_v_disabled {
            None
        } else {
            Some(iced::Background::Color(iced::Color {
                a: v_rail_alpha,
                ..palette.text
            }))
        },
        border: iced::Border {
            radius: radius.into(),
            ..iced::Border::default()
        },
        scroller: scrollable::Scroller {
            background: if is_v_disabled {
                iced::Background::Color(iced::Color::TRANSPARENT)
            } else {
                iced::Background::Color(iced::Color {
                    a: v_thumb_alpha,
                    ..palette.text
                })
            },
            border: iced::Border {
                radius: radius.into(),
                ..iced::Border::default()
            },
        },
    };

    let h_rail = scrollable::Rail {
        background: if is_h_disabled {
            None
        } else {
            Some(iced::Background::Color(iced::Color {
                a: h_rail_alpha,
                ..palette.text
            }))
        },
        border: iced::Border {
            radius: radius.into(),
            ..iced::Border::default()
        },
        scroller: scrollable::Scroller {
            background: if is_h_disabled {
                iced::Background::Color(iced::Color::TRANSPARENT)
            } else {
                iced::Background::Color(iced::Color {
                    a: h_thumb_alpha,
                    ..palette.text
                })
            },
            border: iced::Border {
                radius: radius.into(),
                ..iced::Border::default()
            },
        },
    };

    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: v_rail,
        horizontal_rail: h_rail,
        gap: None,
        auto_scroll: scrollable::default(theme, status).auto_scroll,
    }
}

#[cfg(test)]
#[path = "../tests/gui/theme_tests.rs"]
mod tests;
