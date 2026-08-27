//! GPUI-aligned layout primitives for the Iced Performance page.
//!
//! The data stays in the sibling performance modules; this file only owns the
//! renderer geometry that every device detail shares: a large left graph area
//! and a fixed right-hand statistics column inside one elevated card. Keeping
//! this projection in one place prevents Disk/Network/GPU/Battery/Fan from
//! drifting back into unrelated one-column layouts.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_theme::tokens;

use crate::app::Message;
use crate::theme;

type Elem<'a> = Element<'a, Message, iced::Theme, iced::Renderer>;

/// A compact, fixed-order row of simultaneous current facts. CPU and GPU use
/// this above their single aggregate chart so narrowing the viewport never
/// turns scalar facts into hidden selector state or a scrolling sub-surface.
pub(super) fn headline_readouts(
    theme_snapshot: &taskmanager_theme::Theme,
    items: impl IntoIterator<Item = (String, String)>,
) -> Elem<'static> {
    let label_color = theme::muted_text_color(theme_snapshot);
    let cells = items
        .into_iter()
        .map(|(label, value)| {
            column![
                text(label)
                    .size(f32::from(tokens::FONT_11))
                    .color(label_color),
                text(value).size(f32::from(tokens::FONT_15))
            ]
            .spacing(2)
            .width(Length::FillPortion(1))
            .into()
        })
        .collect::<Vec<Elem<'static>>>();
    row(cells).spacing(12).width(Length::Fill).into()
}

/// Vertical ownership of a Performance detail surface. Scrollable compact
/// device pages need intrinsic content height; the compact CPU aggregate owns
/// its viewport directly and must fill it without creating a scrollbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DetailExtent {
    Fill,
    Content,
}

impl DetailExtent {
    #[must_use]
    pub(super) const fn for_scroll_parent(compact: bool) -> Self {
        if compact { Self::Content } else { Self::Fill }
    }

    pub(super) fn length(self) -> Length {
        match self {
            Self::Fill => Length::Fill,
            Self::Content => Length::Shrink,
        }
    }
}

/// The two geometry contracts shared by the Iced Performance rail and every
/// detail card. These are renderer constants, not a second source of data;
/// the headless tests pin the same values the real widgets consume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GeometryContract {
    pub sidebar_width: f32,
    pub stats_width: f32,
    pub stats_label_width: f32,
    pub title_size: f32,
    pub compact: bool,
}

#[must_use]
pub(super) const fn geometry_contract(compact: bool) -> GeometryContract {
    if compact {
        GeometryContract {
            sidebar_width: 0.0,
            stats_width: 154.0,
            stats_label_width: 62.0,
            title_size: 19.0,
            compact: true,
        }
    } else {
        GeometryContract {
            sidebar_width: 216.0,
            stats_width: 246.0,
            stats_label_width: 96.0,
            title_size: 24.0,
            compact: false,
        }
    }
}

/// Build one GPUI-shaped Performance detail card.
///
/// `left` contains the title, graph controls, primary graph, summaries and
/// optional secondary graphs. `stats` is rendered as aligned label/value rows
/// in a fixed right column, matching GPUI's `main_with_stats` projection.
pub(super) fn main_with_stats<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    title: String,
    subtitle: String,
    left: Vec<Elem<'a>>,
    stats: Vec<(String, String)>,
    compact: bool,
    extent: DetailExtent,
) -> Elem<'a> {
    let geometry = geometry_contract(compact);
    let subtitle_size = if geometry.compact { 12 } else { 15 };
    let heading = row![
        text(bounded_heading(&title, if compact { 28 } else { 64 })).size(geometry.title_size),
        text(bounded_heading(&subtitle, if compact { 32 } else { 72 }))
            .size(subtitle_size)
            .color(theme::muted_text_color(theme_snapshot)),
    ]
    .spacing(if compact { 6 } else { 10 })
    .align_y(iced::Alignment::Center);

    let left = column(std::iter::once(heading.into()).chain(left))
        .spacing(if compact { 8 } else { 12 })
        .width(Length::Fill)
        .height(extent.length());
    // The compact contract is intentionally single-column. Keeping the stats
    // column beside the graph at 720×480 makes its intrinsic rows paint over
    // the chart because Iced's row measurement cannot shrink a fixed-width
    // facts panel below its content. The facts remain available in the wide
    // layout; the narrow layout reserves the viewport for the primary graph.
    let content: Elem<'a> = if compact {
        column![left]
            .width(Length::Fill)
            .height(extent.length())
            .into()
    } else {
        let stats = stats_panel(theme_snapshot, stats, false);
        row![left, stats]
            .spacing(16)
            .width(Length::Fill)
            .height(extent.length())
            .into()
    };

    container(content)
        .padding(if compact { 8 } else { 12 })
        .width(Length::Fill)
        .height(extent.length())
        .style(move |_| theme::panel_style(theme_snapshot))
        .into()
}

/// Text in a flex heading must never be allowed to establish an unbounded
/// intrinsic width. Long GPU brands, disk models and SSIDs are still available
/// in the detail rows; the card heading keeps only a glanceable prefix and an
/// ellipsis so the stats column cannot be pushed out of the viewport. Shared
/// by the detail-card headings and the device-rail card headings.
pub(super) fn bounded_heading(value: &str, max_chars: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max_chars {
        return value.to_owned();
    }
    let take = max_chars.saturating_sub(1).max(1);
    format!("{}…", chars.into_iter().take(take).collect::<String>())
}

/// The fixed-width right-hand statistics column shared by every detail card.
fn stats_panel<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    stats: Vec<(String, String)>,
    compact: bool,
) -> Elem<'a> {
    let geometry = geometry_contract(compact);
    let value_size = if compact { 11 } else { 13 };
    let rows: Vec<Elem<'static>> = stats
        .into_iter()
        .map(|(label, value)| {
            row![
                text(label)
                    .size(if compact { 10 } else { 12 })
                    .color(theme::muted_text_color(theme_snapshot))
                    .width(Length::Fixed(geometry.stats_label_width)),
                text(value)
                    .size(value_size)
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Right),
            ]
            .spacing(if compact { 4 } else { 8 })
            .align_y(iced::Alignment::Start)
            .into()
        })
        .collect();

    container(column(rows).spacing(if compact { 5 } else { 9 }))
        .width(Length::Fixed(geometry.stats_width))
        .height(if compact {
            Length::Shrink
        } else {
            Length::Fill
        })
        .padding(if compact { [2.0, 0.0] } else { [4.0, 0.0] })
        .into()
}

/// A nested graph card keeps the main plot visually distinct from the outer
/// device card while still using the same theme surface and border tokens.
pub(super) fn graph_card<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    content: Elem<'static>,
    extent: DetailExtent,
) -> Elem<'a> {
    container(content)
        .padding(8)
        .width(Length::Fill)
        .height(extent.length())
        .style(move |_| theme::card_style(theme_snapshot))
        .into()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_layout_tests.rs"]
mod tests;
