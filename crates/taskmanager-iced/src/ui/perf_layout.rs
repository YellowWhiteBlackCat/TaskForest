//! GPUI-aligned layout primitives for the Iced Performance page.
//!
//! The data stays in the sibling performance modules; this file only owns the
//! renderer geometry every device detail shares: the GPUI `perf_page` slot
//! contract (title row → vital line → left graph column → pinned/stacked
//! statistics rail with an optional footer), driven by the one typed
//! [`PerformancePageBudget`] instead of a local compact flag. Statistics rows
//! consume the shared shell [`StatRow`] contract so missing values render the
//! ONE shared dash in a dim style — the same fold all three renderers read.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_shell::presentation::missing_value;
use taskmanager_shell::viewmodel::StatRow;
use taskmanager_theme::tokens;

use super::responsive::{
    DeviceNavigationPresentation, PERFORMANCE_STATS_STACK_HEIGHT, PerformanceDetailsPresentation,
    PerformancePageBudget,
};
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

/// Vertical ownership of a Performance detail surface. Strip frames own an
/// outer scrollable and need intrinsic content height; sidebar frames own a
/// fixed viewport whose charts absorb the remaining height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DetailExtent {
    Fill,
    Content,
}

impl DetailExtent {
    /// Strip navigation keeps the page-level scroll boundary: the detail
    /// column reports intrinsic height so the scrollable can measure it.
    #[must_use]
    pub(super) const fn for_scroll_parent(navigation: DeviceNavigationPresentation) -> Self {
        match navigation {
            DeviceNavigationPresentation::Strip => Self::Content,
            DeviceNavigationPresentation::Sidebar => Self::Fill,
        }
    }

    pub(super) fn length(self) -> Length {
        match self {
            Self::Fill => Length::Fill,
            Self::Content => Length::Shrink,
        }
    }
}

/// The typed title-size contract shared by every Performance detail card
/// (strip frames render the smaller heading). Statistics and rail widths are
/// NOT geometry literals anymore: they come from the frame budget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GeometryContract {
    pub title_size: f32,
    pub compact: bool,
}

#[must_use]
pub(super) const fn geometry_contract(compact: bool) -> GeometryContract {
    if compact {
        GeometryContract {
            title_size: 19.0,
            compact: true,
        }
    } else {
        GeometryContract {
            title_size: 24.0,
            compact: false,
        }
    }
}

/// Build one GPUI-shaped Performance detail card through the shared slot
/// contract: title row, undroppable vital line, left graph column, and the
/// statistics rail in the frame's presentation — Pinned beside the graphs,
/// Stacked below them, or Hidden when the frame cannot carry either.
///
/// `left` contains the header band, graph controls, primary graph, summaries
/// and secondary graphs. `stats` are pre-folded shell [`StatRow`]s; missing
/// values render the shared dash dimmed. `stats_footer` pins one element
/// (status footer, SMART button) under the statistics rail.
#[allow(clippy::too_many_arguments)]
pub(super) fn main_with_stats<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    title: String,
    subtitle: String,
    vital_line: Option<String>,
    left: Vec<Elem<'a>>,
    stats: Vec<StatRow>,
    stats_footer: Option<Elem<'a>>,
    budget: PerformancePageBudget,
    extent: DetailExtent,
) -> Elem<'a> {
    let compact = budget.device_navigation == DeviceNavigationPresentation::Strip;
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

    // The vital line is the page's undroppable one-line fact: unlike the
    // header band it renders at EVERY vertical rung, so even the Floor
    // composition still answers "how full / how fast / how healthy".
    let left_column = std::iter::once(heading.into()).chain(vital_line.map(|line| {
        text(line)
            .size(f32::from(tokens::FONT_13))
            .color(theme::muted_text_color(theme_snapshot))
            .width(Length::Fill)
            .into()
    }));
    let left = column(left_column.chain(left))
        .spacing(if compact { 8 } else { 12 })
        .width(Length::Fill)
        .height(extent.length());

    let content: Elem<'a> = match budget.details {
        PerformanceDetailsPresentation::Hidden => column![left]
            .width(Length::Fill)
            .height(extent.length())
            .into(),
        PerformanceDetailsPresentation::Pinned => {
            let stats = stats_rail(
                theme_snapshot,
                stats,
                stats_footer,
                Length::Fixed(budget.stats_width),
                RailEdge::Left,
                compact,
            );
            row![left, stats]
                .spacing(16)
                .width(Length::Fill)
                .height(extent.length())
                .into()
        }
        PerformanceDetailsPresentation::Stacked => {
            // Narrow-capacity fallback (GPUI parity): the rail stays available
            // below the main viewport with one fixed readable height instead
            // of starving the primary graph.
            let stats = stats_rail(
                theme_snapshot,
                stats,
                stats_footer,
                Length::Fill,
                RailEdge::Top,
                compact,
            );
            column![left, stats]
                .spacing(12)
                .width(Length::Fill)
                .height(extent.length())
                .into()
        }
    };

    container(content)
        .padding(if compact { 8 } else { 12 })
        .width(Length::Fill)
        .height(extent.length())
        .style(move |_| theme::panel_style(theme_snapshot))
        .into()
}

/// Which edge of the statistics rail carries the divider to the main
/// viewport: Pinned rails hang off the left edge, Stacked rails off the top.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RailEdge {
    Left,
    Top,
}

/// The one statistics surface used by both pinned and stacked modes: the
/// pre-folded rows plus the optional footer, inside the rail's own vertical
/// scroll boundary so a long inventory never clips silently (GPUI parity —
/// its stats rail scrolls through `scroll_region_with_rail`).
#[allow(clippy::too_many_arguments)]
fn stats_rail<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    stats: Vec<StatRow>,
    footer: Option<Elem<'a>>,
    width: Length,
    edge: RailEdge,
    compact: bool,
) -> Elem<'a> {
    let mut body = stats_panel(theme_snapshot, stats, compact);
    if let Some(footer) = footer {
        body = column![body, footer].spacing(12).into();
    }
    let mut rail = container(
        scrollable(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .width(4)
                    .scroller_width(4),
            )),
    )
    .width(width)
    .height(Length::Fill)
    .padding(if compact { 2.0 } else { 4.0 });
    match edge {
        RailEdge::Left => {
            rail = rail
                .style(move |_| theme::rail_divider_left(theme_snapshot))
                .padding(if compact { 8.0 } else { 12.0 });
        }
        RailEdge::Top => {
            rail = rail
                .height(Length::Fixed(PERFORMANCE_STATS_STACK_HEIGHT))
                .max_height(PERFORMANCE_STATS_STACK_HEIGHT)
                .style(move |_| theme::rail_divider_top(theme_snapshot))
                .padding(12.0);
        }
    }
    rail.into()
}

/// The fixed statistics column shared by every detail card. Rows read the
/// shell [`StatRow`] contract: the label owns the elastic side, the value
/// keeps its intrinsic width flush right, and `None` values draw the ONE
/// shared dash in the dim foreground so an uncollected field reads quieter
/// than present data.
pub(super) fn stats_panel(
    theme_snapshot: &taskmanager_theme::Theme,
    stats: Vec<StatRow>,
    compact: bool,
) -> Elem<'static> {
    let value_size = if compact { 11 } else { 13 };
    let rows: Vec<Elem<'static>> = stats
        .into_iter()
        .map(|stat| {
            let (value, missing) = match stat.value() {
                Some(value) => (value.to_owned(), false),
                None => (missing_value(), true),
            };
            row![
                text(stat.label().to_owned())
                    .size(if compact { 10 } else { 12 })
                    .color(theme::muted_text_color(theme_snapshot))
                    .width(Length::Fill),
                text(value).size(value_size).color(if missing {
                    theme::muted_text_color(theme_snapshot)
                } else {
                    theme::color(theme_snapshot.palette().fg)
                }),
            ]
            .spacing(if compact { 4 } else { 8 })
            .align_y(iced::Alignment::Start)
            .width(Length::Fill)
            .into()
        })
        .collect();

    column(rows)
        .spacing(if compact { 5 } else { 9 })
        .width(Length::Fill)
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
