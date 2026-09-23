//! GPUI-aligned layout primitives for the Iced Performance page.
//!
//! The data stays in the sibling performance modules; this file only owns the
//! renderer geometry every device detail shares: the GPUI `perf_page` slot
//! contract (title row → vital line → left graph column → pinned/stacked
//! statistics rail with an optional footer), driven by the one typed
//! [`PerformancePageBudget`] instead of a local compact flag. Statistics rows
//! consume the shared shell [`StatRow`] contract so missing values render the
//! ONE shared dash in a dim style — the same fold all frontend renderers read.

use iced::alignment::Horizontal;
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
use taskmanager_theme::Theme;

type Elem<'a> = Element<'a, Message, iced::Theme, iced::Renderer>;

/// A compact, fixed-order row of simultaneous current facts. CPU and GPU use
/// this above their single aggregate chart so narrowing the viewport never
/// turns scalar facts into hidden selector state or a scrolling sub-surface.
pub(super) fn headline_readouts(
    theme_snapshot: &Theme,
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

/// The three text slots of a Performance detail card's header band, in render
/// order: the page title that owns the left edge, the right-aligned device
/// subtitle, and the undroppable one-line vital fact rendered at every
/// vertical rung.
pub(super) struct DetailHeader {
    /// The card's primary heading.
    pub title: String,
    /// The device identity shown on the heading baseline.
    pub subtitle: String,
    /// The one-line vital fact; `None` when the family has no loss fact.
    pub vital_line: Option<String>,
}

/// The body of a Performance detail card: the left graph column below the
/// header band, the pre-folded statistics rows, and the optional element
/// pinned under the statistics rail.
pub(super) struct DetailBody<'a> {
    /// The left column's graph controls, primary graph, summaries and
    /// secondary graphs.
    pub left: Vec<Elem<'a>>,
    /// The pre-folded shell [`StatRow`]s for the statistics rail.
    pub stats: Vec<StatRow>,
    /// The element pinned under the rail (status footer, SMART button).
    pub footer: Option<Elem<'a>>,
}

/// Build one GPUI-shaped Performance detail card through the shared slot
/// contract: title row, undroppable vital line, left graph column, and the
/// statistics rail in the frame's presentation — Pinned beside the graphs,
/// Stacked below them, or Hidden when the frame cannot carry either.
///
/// `header` owns the three text slots, and `body` owns the graph column, the
/// pre-folded shell [`StatRow`]s (missing values render the shared dash
/// dimmed) and the optional rail footer.
pub(super) fn main_with_stats<'a>(
    theme_snapshot: &'a Theme,
    header: DetailHeader,
    body: DetailBody<'a>,
    budget: PerformancePageBudget,
    extent: DetailExtent,
) -> Elem<'a> {
    let DetailHeader {
        title,
        subtitle,
        vital_line,
    } = header;
    let DetailBody {
        left,
        stats,
        footer,
    } = body;
    let compact = budget.device_navigation == DeviceNavigationPresentation::Strip;
    let geometry = geometry_contract(compact);
    let subtitle_size = if geometry.compact { 12 } else { 15 };
    // GPUI header hierarchy (ICED-024-6): the page title owns the left edge,
    // the device model right-aligns on the same baseline — not a left-packed
    // pair that leaves the row's right half unread.
    let heading = row![
        text(bounded_heading(&title, if compact { 28 } else { 24 }))
            .size(geometry.title_size)
            .width(Length::Fill),
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
                StatsRail {
                    stats,
                    footer,
                    width: Length::Fixed(budget.stats_width),
                    edge: RailEdge::Left,
                    compact,
                },
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
                StatsRail {
                    stats,
                    footer,
                    width: Length::Fill,
                    edge: RailEdge::Top,
                    compact,
                },
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

/// The statistics rail's content and frame placement: the pre-folded rows and
/// their optional pinned footer, the rail's resolved width, the edge that owns
/// the divider to the main viewport, and the compact typography flag.
struct StatsRail<'a> {
    stats: Vec<StatRow>,
    footer: Option<Elem<'a>>,
    width: Length,
    edge: RailEdge,
    compact: bool,
}

/// The one statistics surface used by both pinned and stacked modes: the
/// pre-folded rows plus the optional footer, inside the rail's own vertical
/// scroll boundary so a long inventory never clips silently (GPUI parity —
/// its stats rail scrolls through `scroll_region_with_rail`).
fn stats_rail<'a>(theme_snapshot: &'a Theme, rail: StatsRail<'a>) -> Elem<'a> {
    let StatsRail {
        stats,
        footer,
        width,
        edge,
        compact,
    } = rail;
    let rail_padding = match edge {
        RailEdge::Left if compact => 8.0,
        RailEdge::Left => 12.0,
        RailEdge::Top => 12.0,
    };
    // Iced container widths apply to the content box. Subtract the rail's
    // horizontal padding from a fixed budget so the outer rail stays inside
    // the slot arithmetic; otherwise long SMART labels can push the value
    // column past the window edge by exactly the padding amount.
    let content_width = match width {
        Length::Fixed(value) => Length::Fixed((value - rail_padding * 2.0).max(0.0)),
        other => other,
    };
    let mut body = stats_panel(theme_snapshot, stats, compact);
    if let Some(footer) = footer {
        body = column![body, footer].spacing(12).into();
    }
    let mut rail = container(
        scrollable(body)
            .width(content_width)
            .height(Length::Fill)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .width(4)
                    .scroller_width(4),
            )),
    )
    .width(content_width)
    .height(Length::Fill)
    .padding(rail_padding);
    match edge {
        RailEdge::Left => {
            rail = rail.style(move |_| theme::rail_divider_left(theme_snapshot));
        }
        RailEdge::Top => {
            rail = rail
                .height(Length::Fixed(PERFORMANCE_STATS_STACK_HEIGHT))
                .max_height(PERFORMANCE_STATS_STACK_HEIGHT)
                .style(move |_| theme::rail_divider_top(theme_snapshot));
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
    theme_snapshot: &Theme,
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
            let label = bounded_stat_text(stat.label(), compact, true);
            let value = bounded_stat_text(&value, compact, false);
            row![
                text(label)
                    .size(if compact { 10 } else { 12 })
                    .color(theme::muted_text_color(theme_snapshot))
                    .width(Length::FillPortion(1))
                    .wrapping(iced::widget::text::Wrapping::None),
                text(value)
                    .size(value_size)
                    .color(if missing {
                        theme::muted_text_color(theme_snapshot)
                    } else {
                        crate::theme_binding::color(theme_snapshot.palette().fg)
                    })
                    .width(Length::FillPortion(2))
                    .align_x(Horizontal::Right)
                    .wrapping(iced::widget::text::Wrapping::None),
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

/// Keep every statistic row a single measured line. Iced's text widget wraps
/// long intrinsic values when the label/value flex row becomes narrower than
/// the value; the row then grows visually without reserving a matching line
/// box in the dense rail. Bounded text plus explicit `Wrapping::None` makes
/// the truncation an intentional, stable visual contract. The full value is
/// still available from the shared projection and richer frontends.
#[must_use]
pub(super) fn bounded_stat_text(value: &str, compact: bool, label: bool) -> String {
    let max_chars = match (compact, label) {
        // The rail is split into a one-third label slot and a two-thirds
        // value slot. These budgets are deliberately below the slot's
        // smallest reference width so a no-wrap text run cannot paint into
        // its sibling even when the renderer declines to clip glyphs.
        (true, true) => 14,
        (true, false) => 16,
        (false, true) => 18,
        (false, false) => 20,
    };
    bounded_heading(value, max_chars)
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
    theme_snapshot: &'a Theme,
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
