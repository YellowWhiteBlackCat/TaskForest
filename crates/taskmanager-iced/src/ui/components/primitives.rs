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
use iced::{Alignment, Border, Length};
use taskmanager_theme::color::on_accent;
use taskmanager_theme::{Color, Theme, tokens};
use taskmanager_ui_contract::IconId;

use super::IcedElement;
use crate::theme;

/// Badge / progress fill tones; each maps to one palette semantic color.
///
/// Consumed by the footer status pills (paused, active-alert) and the GPU
/// VRAM meters' fill tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BadgeTone {
    /// Quiet neutral chip: `palette.surface` fill, `palette.fg_muted` text.
    /// Grammar-complete: no page constructs it yet (no neutral-chip call site).
    #[allow(dead_code)]
    Neutral,
    /// `palette.success`. Grammar-complete: no page constructs it yet (the
    /// success-look call site lands with a success-state surface).
    #[allow(dead_code)]
    Success,
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
            Self::Neutral => palette.surface,
            Self::Success => palette.success,
            Self::Warning => palette.warning,
            Self::Danger => palette.danger,
            Self::Accent => palette.accent,
        }
    }

    /// The tone's foreground on its own fill: tinted tones pick the
    /// higher-contrast black/white via [`on_accent`] (WCAG contrast over
    /// the fill's luminance, so light and dark skins both stay legible);
    /// the neutral chip keeps the muted foreground so it stays quiet.
    #[must_use]
    pub(crate) fn foreground(self, theme: &Theme) -> Color {
        match self {
            Self::Neutral => theme.palette().fg_muted,
            tinted => on_accent(tinted.fill(theme)),
        }
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
    let fill = taskmanager_theme::iced::color(tone.fill(theme));
    let foreground = taskmanager_theme::iced::color(tone.foreground(theme));
    container(
        text(label.to_owned())
            .size(f32::from(tokens::FONT_CAPTION))
            .color(foreground),
    )
    .padding([0.0, f32::from(tokens::SPACE_8)])
    .height(Length::Fixed(BADGE_HEIGHT))
    .align_y(Alignment::Center)
    .style(move |_| container_style(fill, (BADGE_HEIGHT / 2.0).into()))
    .into()
}

/// A 1px horizontal hairline in the palette's border color — the divider
/// that separates stacked sections inside a card without a second surface.
pub(crate) fn divider<'a>(theme: &'a Theme) -> IcedElement<'a> {
    let hairline = taskmanager_theme::iced::color(theme.palette().border);
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
    let track_fill = taskmanager_theme::iced::color(palette.border);
    let stripe_fill = taskmanager_theme::iced::color(palette.fg_muted);
    let value_fill = taskmanager_theme::iced::color(tone.fill(theme));
    let radius = f32::from(palette.xsmall_radius);
    let bar_height = f32::from(tokens::SPACE_6);

    let bar: IcedElement<'a> = match progress_fill(value) {
        ProgressFill::Determinate(ratio) => {
            // FillPortion is a u16 ratio pair; 0.1% resolution over a
            // clamped 0..=1 ratio can never overflow the 1000 budget.
            let filled = (ratio * 1000.0).round() as u16;
            row![
                container(iced::widget::Space::new())
                    .width(Length::FillPortion(filled))
                    .height(Length::Fill)
                    .style(move |_| container_style(value_fill, radius.into())),
                container(iced::widget::Space::new())
                    .width(Length::FillPortion(1000 - filled))
                    .height(Length::Fill),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
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
    let tip_fg = taskmanager_theme::iced::color(theme.palette().fg);
    let surface = theme;
    let hint: IcedElement<'a> = container(text(tip).size(f32::from(tokens::FONT_11)).color(tip_fg))
        .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_8)])
        .style(move |_| theme::panel_style(surface))
        .into();
    iced::widget::tooltip(content, hint, iced::widget::tooltip::Position::FollowCursor)
        .snap_within_viewport(true)
        .gap(f32::from(tokens::SPACE_4))
        .into()
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
    /// Recovering after a failure (e.g. the first post-retry frames).
    /// Grammar-complete today; source panels adopt it with the retry lane.
    #[allow(dead_code)]
    Recovery,
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
            Self::Recovery => palette.success,
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
    let tone_fill = taskmanager_theme::iced::color(tone);
    let tile_fill = taskmanager_theme::iced::color(tone.with_alpha(0.12));
    let tile_border = taskmanager_theme::iced::color(tone.with_alpha(0.30));
    let muted = taskmanager_theme::iced::color(palette.fg_muted);
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

    container(content.width(Length::Fill).align_x(Alignment::Center))
        .style(move |_| state_panel_style(surface, tone_fill))
        .padding(f32::from(tokens::SPACE_16))
        .center_x(Length::Fill)
        .width(Length::Fill)
        .into()
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

#[cfg(test)]
#[path = "../../../tests/gui/ui/components_primitives_tests.rs"]
mod components_primitives_tests;
