//! Startup boot timeline and waterfall block for Iced.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_application::{StartupBootEvidenceSnapshot, boot_timeline_rows};
use taskmanager_theme::tokens;

use crate::app::Message;
use crate::theme;

/// Waterfall bar track width in px (a layout contract, not a theme token).
const TIMELINE_BAR_WIDTH: f32 = 220.0;
/// Minimum visible bar width so a 0-duration activation is still a mark.
const TIMELINE_MIN_BAR_PX: f32 = 3.0;

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
pub(super) fn boot_timeline_block<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    evidence: Option<&StartupBootEvidenceSnapshot>,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let (total_ms, kinds) = startup_timeline(evidence)?;
    let muted = theme::muted_text_color(theme_snapshot);
    let accent = theme::color(theme_snapshot.accent);
    let warning_color = theme::color(theme_snapshot.palette().warning);
    let track = theme::color(theme_snapshot.card_surface());
    let mut rows: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = vec![
        row![
            text(t("startup.timeline")).size(f32::from(tokens::FONT_12)),
            text(format!("{total_ms} ms"))
                .size(f32::from(tokens::FONT_11))
                .color(muted),
        ]
        .spacing(8)
        .into(),
    ];
    for kind in kinds {
        let element: Element<'a, Message, iced::Theme, iced::Renderer> = match kind {
            TimelineRowKind::Measured {
                unit,
                fraction,
                duration_ms,
            } => {
                let bar_px =
                    (fraction * TIMELINE_BAR_WIDTH).clamp(TIMELINE_MIN_BAR_PX, TIMELINE_BAR_WIDTH);
                // Bottleneck highlight (> 1000ms duration uses warning color)
                let bar_color = if duration_ms > 1000 {
                    warning_color
                } else {
                    accent
                };
                row![
                    text(unit)
                        .size(f32::from(tokens::FONT_11))
                        .width(Length::Fixed(190.0)),
                    container(
                        container(text(""))
                            .width(Length::Fixed(bar_px))
                            .height(Length::Fixed(8.0))
                            .style(move |_| theme::fill_style(bar_color)),
                    )
                    .width(Length::Fixed(TIMELINE_BAR_WIDTH))
                    .height(Length::Fixed(8.0))
                    .style(move |_| theme::fill_style(track)),
                    text(format!("{duration_ms} ms"))
                        .size(f32::from(tokens::FONT_11))
                        .color(muted),
                ]
                .spacing(8)
                .into()
            }
            TimelineRowKind::Untimed { count, names } => row![
                text(t("startup.timeline_untimed"))
                    .size(f32::from(tokens::FONT_11))
                    .color(muted)
                    .width(Length::Fixed(190.0)),
                text(format!("{count} · {}", names.join(" · ")))
                    .size(f32::from(tokens::FONT_11))
                    .color(muted),
            ]
            .spacing(8)
            .into(),
            TimelineRowKind::Collapsed { count } => text(format!("+{count}"))
                .size(f32::from(tokens::FONT_11))
                .color(muted)
                .into(),
        };
        rows.push(element);
    }
    Some(column(rows).spacing(2).padding(8).into())
}
