//! Startup boot timeline and waterfall block for Iced.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::boot_timeline_rows;
use taskmanager_application::i18n::t;
use taskmanager_core::core::startup::StartupBootEvidenceSnapshot;

use taskmanager_theme::tokens;

use crate::app::Message;
use crate::text_metrics::truncate_to_width;
use crate::theme;

/// Waterfall bar track width in px in standard layout (a layout contract, not a theme token).
pub(crate) const TIMELINE_BAR_WIDTH: f32 = 220.0;
/// Waterfall bar track width in px in compact layout to avoid overflowing narrow viewports.
pub(crate) const TIMELINE_BAR_WIDTH_COMPACT: f32 = 130.0;

/// Unit name column width in px in standard layout.
pub(crate) const TIMELINE_UNIT_WIDTH: f32 = 190.0;
/// Unit name column width in px in compact layout.
pub(crate) const TIMELINE_UNIT_WIDTH_COMPACT: f32 = 130.0;

/// Minimum visible bar width so a 0-duration activation is still a mark.
pub(crate) const TIMELINE_MIN_BAR_PX: f32 = 3.0;

/// Duration readout column width in px in standard layout.
pub(crate) const TIMELINE_DURATION_WIDTH: f32 = 70.0;
/// Duration readout column width in px in compact layout.
pub(crate) const TIMELINE_DURATION_WIDTH_COMPACT: f32 = 56.0;

/// One renderable waterfall row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TimelineRowKind {
    Measured {
        unit: String,
        fraction: f32,
        duration_ms: u64,
    },
    Untimed {
        count: usize,
        names: Vec<String>,
    },
    Collapsed {
        count: usize,
    },
}

/// Format an impact duration cleanly: milliseconds when under 1 second,
/// fractional seconds when under 1 minute, and minutes + seconds for longer spans.
#[must_use]
pub(crate) fn format_impact_time(duration_ms: u64) -> String {
    if duration_ms >= 60_000 {
        let mins = duration_ms / 60_000;
        let rem_s = (duration_ms % 60_000) as f64 / 1000.0;
        if duration_ms.is_multiple_of(60_000) {
            format!("{mins} min")
        } else {
            format!("{mins}m {rem_s:.1}s")
        }
    } else if duration_ms >= 1_000 {
        let secs = duration_ms as f64 / 1000.0;
        if duration_ms.is_multiple_of(1_000) {
            format!("{secs:.0} s")
        } else if duration_ms.is_multiple_of(100) {
            format!("{secs:.1} s")
        } else {
            format!("{secs:.2} s")
        }
    } else {
        format!("{duration_ms} ms")
    }
}

/// Pure waterfall projection over one typed evidence snapshot, or `None`
/// when the block must stay silent (no evidence / typed failure).
pub(crate) fn startup_timeline(
    evidence: Option<&StartupBootEvidenceSnapshot>,
) -> Option<(u64, Vec<TimelineRowKind>)> {
    let timeline = boot_timeline_rows(evidence?)?;
    let mut rows: Vec<TimelineRowKind> = timeline
        .segments
        .iter()
        .map(|segment| TimelineRowKind::Measured {
            unit: segment.unit.clone(),
            fraction: timeline.fraction_of_total(segment),
            duration_ms: segment.duration_ms,
        })
        .collect();
    if timeline.untimed_count > 0 {
        rows.push(TimelineRowKind::Untimed {
            count: timeline.untimed_count,
            names: timeline.untimed_units.clone(),
        });
    }
    if timeline.collapsed_count > 0 {
        rows.push(TimelineRowKind::Collapsed {
            count: timeline.collapsed_count,
        });
    }
    Some((timeline.total_ms, rows))
}

/// The display-only waterfall block; `None` when silent.
pub(crate) fn boot_timeline_block<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    evidence: Option<&StartupBootEvidenceSnapshot>,
    compact: bool,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let (total_ms, kinds) = startup_timeline(evidence)?;
    let muted = theme::muted_text_color(theme_snapshot);
    let accent = crate::theme_binding::color(theme_snapshot.accent);
    let warning_color = crate::theme_binding::color(theme_snapshot.palette().warning);
    let track = crate::theme_binding::color(theme_snapshot.card_surface());

    let (bar_width, unit_width, duration_width, bar_height, spacing, padding, row_spacing) =
        if compact {
            (
                TIMELINE_BAR_WIDTH_COMPACT,
                TIMELINE_UNIT_WIDTH_COMPACT,
                TIMELINE_DURATION_WIDTH_COMPACT,
                6.0,
                6.0,
                4.0,
                2.0,
            )
        } else {
            (
                TIMELINE_BAR_WIDTH,
                TIMELINE_UNIT_WIDTH,
                TIMELINE_DURATION_WIDTH,
                8.0,
                8.0,
                8.0,
                2.0,
            )
        };

    let font_size = if compact {
        f32::from(tokens::FONT_10)
    } else {
        f32::from(tokens::FONT_11)
    };

    let header_size = if compact {
        f32::from(tokens::FONT_11)
    } else {
        f32::from(tokens::FONT_12)
    };

    let mut rows: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = vec![
        row![
            text(t("startup.timeline"))
                .size(header_size)
                .wrapping(iced::widget::text::Wrapping::None),
            text(format_impact_time(total_ms))
                .size(font_size)
                .color(muted)
                .wrapping(iced::widget::text::Wrapping::None),
        ]
        .spacing(spacing)
        .align_y(iced::Alignment::Center)
        .into(),
    ];
    for kind in kinds {
        let element: Element<'a, Message, iced::Theme, iced::Renderer> = match kind {
            TimelineRowKind::Measured {
                unit,
                fraction,
                duration_ms,
            } => {
                let bar_px = (fraction * bar_width).clamp(TIMELINE_MIN_BAR_PX, bar_width);
                // Bottleneck highlight (> 1000ms duration uses warning color)
                let bar_color = if duration_ms > 1000 {
                    warning_color
                } else {
                    accent
                };
                let unit_display = truncate_to_width(&unit, unit_width, font_size);
                let duration_display = format_impact_time(duration_ms);
                row![
                    text(unit_display)
                        .size(font_size)
                        .width(Length::Fixed(unit_width))
                        .wrapping(iced::widget::text::Wrapping::None),
                    container(
                        container(text(""))
                            .width(Length::Fixed(bar_px))
                            .height(Length::Fixed(bar_height))
                            .style(move |_| theme::fill_style(bar_color)),
                    )
                    .width(Length::Fixed(bar_width))
                    .height(Length::Fixed(bar_height))
                    .style(move |_| theme::fill_style(track)),
                    text(duration_display)
                        .size(font_size)
                        .color(muted)
                        .width(Length::Fixed(duration_width))
                        .wrapping(iced::widget::text::Wrapping::None),
                ]
                .spacing(spacing)
                .align_y(iced::Alignment::Center)
                .into()
            }
            TimelineRowKind::Untimed { count, names } => {
                let untimed_label =
                    truncate_to_width(t("startup.timeline_untimed"), unit_width, font_size);
                let untimed_raw = format!("{count} · {}", names.join(" · "));
                let detail_budget = bar_width + spacing + duration_width;
                let untimed_detail = truncate_to_width(&untimed_raw, detail_budget, font_size);
                row![
                    text(untimed_label)
                        .size(font_size)
                        .color(muted)
                        .width(Length::Fixed(unit_width))
                        .wrapping(iced::widget::text::Wrapping::None),
                    text(untimed_detail)
                        .size(font_size)
                        .color(muted)
                        .width(Length::Fixed(detail_budget))
                        .wrapping(iced::widget::text::Wrapping::None),
                ]
                .spacing(spacing)
                .align_y(iced::Alignment::Center)
                .into()
            }
            TimelineRowKind::Collapsed { count } => text(format!("+{count}"))
                .size(font_size)
                .color(muted)
                .wrapping(iced::widget::text::Wrapping::None)
                .into(),
        };
        rows.push(element);
    }
    Some(column(rows).spacing(row_spacing).padding(padding).into())
}
