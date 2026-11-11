//! Token-styled presentation primitives shared by Iced pages.
//!
//! Owned by the iced-primitives workstream: badge, divider, progress,
//! shared tooltip, and the unified state-panel grammar. Colors come only
//! from the `taskmanager-theme` palette snapshot; geometry only from theme
//! tokens. The family ports the GPUI component layer's SEMANTICS (badge
//! tones with luminance-picked foregrounds, the four-state panel grammar,
//! determinate-vs-unavailable progress, the panel-surfaced tooltip) onto
//! Iced's own widgets — no GPUI code is shared.
//!
//! Type sizes read the shared `FONT_*` role scale (this crate's Small
//! baseline) because the app-level renderer scale zooms the whole surface;
//! the few extents with no spacing token (the badge pill box, the
//! state-panel icon tile) stay commented cross-frontend layout contracts.

use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Element, Length};
use std::path::PathBuf;
use taskmanager_theme::color::on_accent;
use taskmanager_theme::{Color, GlassSurfaceRole, Theme, tokens};
use taskmanager_ui_contract::IconId;

use super::IcedElement;
use super::Tooltip;
use crate::theme;
use zhuge_ui_kit_iced::meter::{MeterGeometry, meter_bar};
use zhuge_ui_kit_iced::surface::{self, BadgeGeometry, StatePanelGeometry};

/// Badge / progress fill tones; each maps to one palette semantic color.
///
/// Consumed by the footer status pills (paused, active-alert) and the GPU
/// VRAM meters' fill tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BadgeTone {
    /// `palette.warning`
    Warning,
    /// `palette.danger`
    Danger,
    /// `palette.accent`
    Accent,
}

impl BadgeTone {
    /// The palette fill token for this tone.
    #[must_use]
    pub(crate) fn fill(self, theme: &Theme) -> Color {
        let palette = theme.palette();
        match self {
            Self::Warning => palette.warning,
            Self::Danger => palette.danger,
            Self::Accent => palette.accent,
        }
    }

    /// The tone's foreground on its own fill: pick the higher-contrast
    /// black/white via [`on_accent`] (WCAG contrast over the fill's
    /// luminance, so light and dark skins both stay legible).
    #[must_use]
    pub(crate) fn foreground(self, theme: &Theme) -> Color {
        on_accent(self.fill(theme))
    }
}

/// The badge pill's box height — the GPUI badge's 20px pill, kept as the
/// shared cross-frontend layout contract (no spacing token sits on the scale).
const BADGE_HEIGHT: f32 = 20.0;

/// A small status pill: one palette-tone fill, a caption-tier label, and a
/// fully rounded capsule. Foregrounds are luminance-picked per fill (see
/// [`BadgeTone::foreground`]), never a hardcoded light/dark literal. The
/// label is copied into the pill's text (the footer pills format theirs per
/// frame), so the returned element borrows only the theme.
pub(crate) fn badge<'a>(theme: &'a Theme, label: &str, tone: BadgeTone) -> IcedElement<'a> {
    let fill = crate::theme_binding::color(tone.fill(theme));
    let foreground = crate::theme_binding::color(tone.foreground(theme));
    surface::badge(
        label,
        BadgeGeometry {
            size: f32::from(tokens::FONT_CAPTION),
            font: None,
            padding: [0.0, f32::from(tokens::SPACE_8)].into(),
            height: Length::Fixed(BADGE_HEIGHT),
            centered: true,
        },
        move |_| container::Style {
            text_color: Some(foreground),
            ..container_style(fill, (BADGE_HEIGHT / 2.0).into())
        },
    )
}

/// A 1px horizontal hairline in the palette's border color — the divider
/// that separates stacked sections inside a card without a second surface.
pub(crate) fn divider<'a>(theme: &'a Theme) -> IcedElement<'a> {
    let hairline = crate::theme_binding::color(theme.palette().border);
    container(iced::widget::Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(f32::from(tokens::SPACE_1)))
        .style(move |_| container_style(hairline, 0.0.into()))
        .into()
}

/// What a progress bar actually knows: a measured ratio, or nothing at all.
///
/// `Unknown` NEVER collapses to a zero fill — an empty track under a tinted
/// fill reads as a measured 0% (product invariant: unavailable data must not
/// masquerade as any value, least of all as a healthy zero).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ProgressFill {
    /// A measured ratio, already clamped into 0..=1.
    Determinate(f32),
    /// The datum is unavailable: render the indeterminate grammar.
    Unknown,
}

/// Resolve a raw optional ratio into the bar's render grammar. `None` and
/// any non-finite sample are unavailable data — never a numeric value.
pub(crate) fn progress_fill(value: Option<f32>) -> ProgressFill {
    match value {
        Some(ratio) if ratio.is_finite() => ProgressFill::Determinate(ratio.clamp(0.0, 1.0)),
        _ => ProgressFill::Unknown,
    }
}

/// Stripes in the unavailable track — the GPUI indeterminate sweep frozen
/// into static muted stripes (layout contract; motion needs the app's
/// per-frame pump, which page-local bars do not carry).
const UNKNOWN_STRIPE_COUNT: usize = 6;

/// A horizontal progress bar. `Some(ratio)` renders the tone-tinted fill at
/// that fraction; `None` renders the indeterminate grammar (muted stripes
/// across the whole track), which is visually distinct from both a measured
/// 0% (empty track) and a completed bar.
pub(crate) fn progress<'a>(
    theme: &'a Theme,
    value: Option<f32>,
    tone: BadgeTone,
) -> IcedElement<'a> {
    let palette = theme.palette();
    let track_fill = crate::theme_binding::color(palette.border);
    let stripe_fill = crate::theme_binding::color(palette.fg_muted);
    let value_fill = crate::theme_binding::color(tone.fill(theme));
    let bar_height = f32::from(tokens::SPACE_6);
    let radius = bar_height / 2.0;

    let bar: IcedElement<'a> = match progress_fill(value) {
        ProgressFill::Determinate(ratio) => meter_bar(
            ratio,
            MeterGeometry {
                height: bar_height,
                width: Length::Fill,
                radius: radius.into(),
            },
            track_fill,
            value_fill,
        ),
        ProgressFill::Unknown => row((0..UNKNOWN_STRIPE_COUNT)
            .map(|_| {
                container(iced::widget::Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(move |_| container_style(stripe_fill, radius.into()))
                    .into()
            })
            .collect::<Vec<IcedElement<'a>>>())
        .spacing(f32::from(tokens::SPACE_2))
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
    };

    container(bar)
        .width(Length::Fill)
        .height(Length::Fixed(bar_height))
        .style(move |_| container_style(track_fill, radius.into()))
        .into()
}

/// The shared tooltip: any trigger element wrapped so its hover hint wears
/// the token-styled panel surface (small type, palette fill/border/radius)
/// and clamps inside the viewport — the replacement for pages reaching for
/// `iced::widget::tooltip` with a bare default popup. Consumed by the footer
/// status pills' simple hover hints; multi-line readouts (the perf_rail
/// sparkline cards) keep their bespoke surface on purpose — a readout is not
/// a hint.
pub(crate) fn tooltip<'a>(
    theme: &'a Theme,
    content: IcedElement<'a>,
    tip: &'a str,
) -> IcedElement<'a> {
    let hint: IcedElement<'a> = tooltip_surface(theme, text(tip));
    Tooltip::new(content, hint)
        .with_focus_visible(theme.focus_visible())
        .into()
}

/// The tooltip's own surface: the shared Card glass panel (the tier the GPUI
/// tooltip and the popup menus wear) with the muted hint text.
fn tooltip_surface<'a>(theme: &'a Theme, body: iced::widget::Text<'a>) -> IcedElement<'a> {
    let tip_fg = crate::theme_binding::color(theme.palette().fg);
    container(body.size(f32::from(tokens::FONT_11)).color(tip_fg))
        .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_8)])
        .style(move |_| theme::glass_surface_style(theme, GlassSurfaceRole::Tooltip))
        .into()
}

/// Owned variant for row projections whose detail string is created during
/// rendering. The trigger keeps its own full text content (including its
/// accessibility text) while the wrapped tooltip exposes that detail without
/// widening the containing table column.
pub(crate) fn tooltip_owned(
    theme: Theme,
    content: IcedElement<'static>,
    tip: String,
) -> IcedElement<'static> {
    tooltip_owned_with_capture(theme, content, tip, None)
}

/// Capture-only form of [`tooltip_owned`]: the same production tooltip
/// component stays open and publishes its mounted surface after layout.
pub(crate) fn tooltip_owned_for_capture(
    theme: Theme,
    content: IcedElement<'static>,
    tip: String,
    marker: PathBuf,
) -> IcedElement<'static> {
    tooltip_owned_with_capture(theme, content, tip, Some(marker))
}

fn tooltip_owned_with_capture(
    theme: Theme,
    content: IcedElement<'static>,
    tip: String,
    marker: Option<PathBuf>,
) -> IcedElement<'static> {
    let tip_fg = crate::theme_binding::color(theme.palette().fg);
    let surface = theme;
    let focus_visible = surface.focus_visible();
    let hint: IcedElement<'static> = container(
        text(tip)
            .size(f32::from(tokens::FONT_11))
            .color(tip_fg)
            .width(Length::Fixed(280.0))
            .wrapping(iced::widget::text::Wrapping::Word),
    )
    .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_8)])
    .style(move |_| theme::panel_style(&surface))
    .into();
    if let Some(marker) = marker {
        Tooltip::new(content, hint)
            .with_focus_visible(focus_visible)
            .open_for_capture(marker)
            .into()
    } else {
        Tooltip::new(content, hint)
            .with_focus_visible(focus_visible)
            .into()
    }
}

/// The four-state grammar every centered state surface speaks, aligned with
/// the GPUI `StatePanel` semantics. State meaning and localized copy stay
/// with the caller; the tone mapping is the shared grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelState {
    /// Confirmed empty, loading, or collecting: quiet neutral content that
    /// claims nothing about health (muted tone — this frontend's
    /// established message-panel grammar).
    Empty,
    /// The source did not answer: failure tone, never a success look.
    Unavailable,
    /// Degraded/partial: some rows are still usable, so caution, not alarm.
    Partial,
}

impl PanelState {
    /// The palette tone token this state renders in.
    #[must_use]
    pub(crate) fn tone(self, theme: &Theme) -> Color {
        let palette = theme.palette();
        match self {
            Self::Empty => palette.fg_muted,
            Self::Unavailable => palette.danger,
            Self::Partial => palette.warning,
        }
    }
}

/// The state tile's extent: a 36px circle around the 14px Small-baseline
/// icon — the GPUI 42/22 state tile scaled to this crate's zoomed baseline
/// (cross-frontend layout contract; no token on the scale).
const STATE_TILE: f32 = 36.0;

/// The centered state surface: a quiet tone-tinted icon tile, a muted title,
/// an optional detail line, and an optional action — one geometry for empty,
/// unavailable, partial, and recovery states. `title` is already localized
/// by the caller; the panel never invents copy or fallback data.
///
/// [`zhuge_ui_kit_iced::surface::state_panel`] is the panel's layout container:
/// it paints the padded, tone-bordered, horizontally centered surface. The
/// product grammar leads with the tone-tinted icon tile, so the kit's title and
/// message rows are `None` — absent rows are not materialized — and the owned
/// copy rides in the `action` slot. Every semantic piece stays here: the icon
/// tile and its tone border, the muted title, the optional detail, the
/// optional action, and the [`PanelState`] tone choice.
pub(crate) fn state_panel<'a>(
    theme: &'a Theme,
    state: PanelState,
    icon: IconId,
    title: &'a str,
    detail: Option<String>,
    action: Option<IcedElement<'a>>,
) -> IcedElement<'a> {
    let palette = theme.palette();
    let tone = state.tone(theme);
    let tone_fill = crate::theme_binding::color(tone);
    let tile_fill = crate::theme_binding::color(tone.with_alpha(0.12));
    let tile_border = crate::theme_binding::color(tone.with_alpha(0.30));
    let muted = crate::theme_binding::color(palette.fg_muted);
    let icon_extent = f32::from(tokens::UiSize::Small.icon_size());
    let surface = theme;

    let mut content = column![].spacing(f32::from(tokens::SPACE_8));
    content = content.push(
        container(crate::icons::icon(theme, icon, icon_extent))
            .width(Length::Fixed(STATE_TILE))
            .height(Length::Fixed(STATE_TILE))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_| bordered(tile_border, (STATE_TILE / 2.0).into(), tile_fill)),
    );
    content = content.push(text(title).size(f32::from(tokens::FONT_BODY)).color(muted));
    if let Some(detail) = detail {
        content = content.push(
            text(detail)
                .size(f32::from(tokens::FONT_HEADER))
                .color(muted),
        );
    }
    if let Some(action) = action {
        content = content.push(action);
    }

    surface::state_panel(
        None,
        None,
        Some(
            content
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .into(),
        ),
        StatePanelGeometry {
            padding: f32::from(tokens::SPACE_16).into(),
            spacing: 0.0,
            ..StatePanelGeometry::default()
        },
        move |_| state_panel_style(surface, tone_fill),
    )
}

/// A solid-fill container style with a corner radius (pre-resolved colors so
/// the style closures stay Copy and lifetime-free).
fn container_style(fill: iced::Color, radius: iced::border::Radius) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(fill)),
        border: Border {
            color: iced::Color::TRANSPARENT,
            width: 0.0,
            radius,
        },
        ..container::Style::default()
    }
}

/// Round only the outer ends of a filled segment in a connected track.
pub(crate) fn segmented_fill_style(
    fill: iced::Color,
    radius: f32,
    starts_track: bool,
    ends_track: bool,
) -> container::Style {
    let start = if starts_track { radius } else { 0.0 };
    let end = if ends_track { radius } else { 0.0 };
    container_style(
        fill,
        iced::border::Radius {
            top_left: start,
            top_right: end,
            bottom_right: end,
            bottom_left: start,
        },
    )
}

/// [`container_style`] plus a 1px border in `border_color`.
fn bordered(
    border_color: iced::Color,
    radius: iced::border::Radius,
    fill: iced::Color,
) -> container::Style {
    container::Style {
        border: Border {
            color: border_color,
            width: f32::from(tokens::SPACE_1),
            radius,
        },
        ..container_style(fill, radius)
    }
}

/// The state surface itself: the shared panel fill with the state's tone on
/// the 1px border — quiet, but never mistakable for a content card.
fn state_panel_style(theme: &Theme, tone: iced::Color) -> container::Style {
    let mut style = theme::panel_style(theme);
    style.border.color = tone;
    style
}

/// A product-styled scrollable with capsule thumb, translucent floating rail,
/// and safe margins (GPUI/Bevy parity).
#[must_use]
pub(crate) fn styled_scrollable<'a, Message: 'a>(
    content: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
) -> iced::widget::Scrollable<'a, Message, iced::Theme, iced::Renderer> {
    scrollable(content, ScrollAxis::Vertical, ScrollScale::Standard)
}

/// A compact product-styled scrollable for nested cards or tight viewports.
#[must_use]
pub(crate) fn styled_compact_scrollable<'a, Message: 'a>(
    content: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
) -> iced::widget::Scrollable<'a, Message, iced::Theme, iced::Renderer> {
    scrollable(content, ScrollAxis::Vertical, ScrollScale::Compact)
}

/// Which bounded viewport owns the overflow. A page declares ONE owner for
/// its axis; nested owners are only legal when their bounds are independent
/// (the Performance rail and detail viewport, for example).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollAxis {
    Vertical,
    Horizontal,
}

/// The shared scrollbar geometry class. The compact class lives in tight
/// nested cards; the standard class owns full-page and chrome strips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollScale {
    Standard,
    Compact,
}

/// The single Iced scrollable constructor. Direction, scrollbar geometry and
/// the glass rail style are one decision here, so a page cannot keep a native
/// scrollbar while its sibling wears the product's capsule glass rail.
#[must_use]
pub(crate) fn scrollable<'a, Message: 'a>(
    content: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
    axis: ScrollAxis,
    scale: ScrollScale,
) -> iced::widget::Scrollable<'a, Message, iced::Theme, iced::Renderer> {
    let (bar, style): (
        iced::widget::scrollable::Scrollbar,
        fn(&iced::Theme, iced::widget::scrollable::Status) -> iced::widget::scrollable::Style,
    ) = match scale {
        ScrollScale::Standard => (theme::scrollbar(), theme::scrollbar_style),
        ScrollScale::Compact => (theme::compact_scrollbar(), theme::compact_scrollbar_style),
    };
    let direction = match axis {
        ScrollAxis::Vertical => iced::widget::scrollable::Direction::Vertical(bar),
        ScrollAxis::Horizontal => iced::widget::scrollable::Direction::Horizontal(bar),
    };
    iced::widget::scrollable(content)
        .direction(direction)
        .style(style)
}

/// The one `Direction` a kit `virtual_table` may spend.
///
/// The kit builds its own `Scrollable`, so it cannot consume the shared
/// [`scrollable`] widget. Its direction and scrollbar geometry must still
/// come from the SAME owner: a page that constructs
/// `Direction::Vertical(theme::scrollbar())` itself would re-open the
/// "one page, a second rail material" gap this family closes. `Both` is the
/// Applications table's typed two-axis case and uses the same standard bar
/// on each axis.
#[must_use]
pub(crate) fn virtual_table_direction(
    axis: VirtualTableScrollAxis,
    scale: ScrollScale,
) -> iced::widget::scrollable::Direction {
    let bar = match scale {
        ScrollScale::Standard => theme::scrollbar(),
        ScrollScale::Compact => theme::compact_scrollbar(),
    };
    match axis {
        VirtualTableScrollAxis::Vertical => iced::widget::scrollable::Direction::Vertical(bar),
        VirtualTableScrollAxis::Both => iced::widget::scrollable::Direction::Both {
            vertical: bar,
            horizontal: bar,
        },
    }
}

/// The scroll axes a kit virtual table owns; one page declares one owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VirtualTableScrollAxis {
    /// A single vertical scroll owner.
    Vertical,
    /// The Applications table's bounded two-axis scroll owner.
    Both,
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/components_primitives_tests.rs"]
mod components_primitives_tests;
