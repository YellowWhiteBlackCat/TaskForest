//! Shared GPUI layout projections for Performance graphs and stat panels.

use gpui::{
    AnyElement, Div, ElementId, InteractiveElement, IntoElement, ParentElement, ScrollHandle,
    Styled, div, px,
};

use crate::gpui_app::elements;
use crate::gpui_app::formatting::{GraphUnit, missing_value};
use crate::gpui_app::graph::{
    GraphHover, GraphOpts, GraphSecondarySeries, GraphSettings, dual_series_colors,
    graph_element_hover, graph_element_hover_dual, graph_hover, latest_samples_rc,
    latest_samples_rc_for_slide,
};
use crate::gpui_app::perf_views::{
    badge_mhz, badge_pct, badge_rpm, badge_temperature, badge_watts, drive_badge_format,
    graph_summary_row, network_badge_format,
};
use crate::gpui_app::theme::{Color, Length, Theme, tokens};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_shell::viewmodel::StatRow;
use taskmanager_ui::data::key_value_row::KeyValueRow;
use taskmanager_ui::layout::scroll_region_with_rail;

/// The shared main graph is inside an intrinsic-height scroll body.  A plain
/// `flex_1().min_h(0)` therefore has no definite height to grow into and GPUI
/// is allowed to resolve it to the graph border only.  Keep this contract at
/// the shared device-page layout boundary so disk, network, GPU, battery and
/// fan pages cannot each drift into a different invisible-chart fix.
const MAIN_GRAPH_MIN_HEIGHT: Length = Length(180.0);
const SECONDARY_GRAPH_MIN_HEIGHT: Length = Length(140.0);
const PERFORMANCE_STATS_WIDTH: Length = Length(280.0);
const PERFORMANCE_TITLE_MAX_WIDTH: Length = Length(400.0);
const PERFORMANCE_STATS_LABEL_WIDTH: Length = Length(112.0);

/// One perf-page stat panel: title, graph, stats rows (design-debt #1 props).
pub(super) struct MainWithStatsProps<'a> {
    pub(super) theme: &'a Theme,
    pub(super) left_scroll: ScrollHandle,
    pub(super) stats_scroll: ScrollHandle,
    pub(super) title: String,
    pub(super) subtitle: String,
    /// Stable per-series slide/tooltip identity. The id travels with the
    /// device, not the page slot, so switching Disk A → Network B → Disk A
    /// cannot make one graph inherit another series' slide timing.
    pub(super) graph_id: ElementId,
    /// Shared, generation-cached series. Passing the `Rc` (not a slice) lets
    /// the tail-limit below keep the identity on a UI-only frame.
    pub(super) graph_samples: Rc<[f32]>,
    pub(super) graph_color: Color,
    pub(super) graph_opts: GraphOpts,
    pub(super) graph_settings: GraphSettings,
    pub(super) graph_unit: GraphUnit,
    /// Optional two-series main graph (a disk's read/write, a NIC's rx/tx):
    /// the primary direction wears the family token, the secondary the same
    /// token lifted toward white. When set, the main graph strokes BOTH
    /// directions through one shared max while `graph_samples` (the summed
    /// lane) stays the aggregate summary's and first-frame state's authority.
    pub(super) graph_dual: Option<MainGraphDualSeries<'a>>,
    pub(super) main_content: MainContent,
    pub(super) main_column: MainColumnLayout,
    /// Optional control row rendered between the title and the graph — the
    /// GPU page's chart-metric pill selector (ADR-034). `None` on every page
    /// whose graph family is fixed.
    pub(super) graph_controls: Option<AnyElement>,
    /// Stat rows as the typed shell ViewModel (ARCH.md §4.0). `value: None`
    /// marks an applicable but uncollected observation — [`stats_panel`]
    /// renders the ONE shared dash in a dimmed style. Producers that want a
    /// fact absent from the panel (not applicable on this host/platform)
    /// simply omit the row.
    pub(super) stats: Vec<StatRow>,
    pub(super) stats_footer: Option<AnyElement>,
    pub(super) left_footer: Option<AnyElement>,
    pub(super) hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
}

/// Explicit ownership of the central chart surface.
///
/// Most pages render their fixed aggregate graph. A standard multi-engine GPU
/// page replaces that one surface with the complete engine inventory; this
/// enum prevents an aggregate graph and an engine main graph from being
/// composed accidentally as two independent children.
pub(super) enum MainContent {
    AggregateGraph,
    EngineInventory(AnyElement),
}

/// One direction of a two-series main graph: the (already store-cached)
/// window and its localized legend/tooltip label.
pub(super) struct MainGraphDualSeries<'a> {
    /// The PRIMARY direction (family-token color): disk read / NIC receive.
    pub(super) primary_samples: Rc<[f32]>,
    pub(super) primary_label: &'a str,
    /// The SECONDARY direction (same token lifted toward white): disk write /
    /// NIC send.
    pub(super) secondary_samples: Rc<[f32]>,
    pub(super) secondary_label: &'a str,
}

/// Vertical extent ownership for a Performance main column.
///
/// Inventory-heavy pages may need an independent scrollbar. A responsive
/// chart surface instead owns the viewport itself so its graph can grow and
/// shrink without a misleading rail beside the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MainColumnLayout {
    Scrollable,
    Viewport,
}

/// Canonical Performance page split: a shrinkable scrolling main column and a
/// pinned, non-shrinking statistics column.
pub(crate) fn performance_split(
    theme: &Theme,
    left: Div,
    stats: Div,
    stats_scroll: ScrollHandle,
) -> Div {
    // Keep a definite readable width here. A percentage max-width can resolve
    // against an indefinite flex measurement pass as zero in GPUI, collapsing
    // the detail column before the parent receives its final width. The graph
    // remains elastic because the sibling owns the remaining flex space.
    let stats = scroll_region_with_rail(
        "perf-stats-scroll",
        "tm-perf-stats-scroll",
        "perf-stats-scrollbar",
        "tm-perf-stats-scrollbar",
        stats_scroll,
        theme.palette(),
        stats,
    )
    // `auto_scroll_region_fill` is a flex-1 viewport by default.  A
    // pinned stats column must clear that growth contract before its
    // explicit width is applied; otherwise Taffy can split the available
    // row between the graph and stats column and leave unused space.
    .flex_none()
    .flex_basis(PERFORMANCE_STATS_WIDTH)
    .w(PERFORMANCE_STATS_WIDTH)
    .h_full()
    // The split is one continuous workspace. A real divider plus padding on
    // the stats surface replaces a transparent parent gap that exposed the
    // window background as a visual crack between sibling components.
    .border_l_1()
    .border_color(theme.border)
    .pl(tokens::SPACE_16)
    .bg(theme.window_bg)
    .debug_selector(|| "tm-perf-stats-surface".to_string());
    div()
        .flex()
        .flex_row()
        .flex_1()
        .w_full()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .size_full()
        .bg(theme.window_bg)
        .child(
            left.flex_grow()
                .flex_shrink()
                .w_full()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .debug_selector(|| "tm-perf-main-surface".to_string()),
        )
        .child(stats)
}

pub(super) fn main_with_stats(props: MainWithStatsProps<'_>) -> Div {
    let MainWithStatsProps {
        theme,
        left_scroll,
        stats_scroll,
        title,
        subtitle,
        graph_id,
        graph_samples,
        graph_color,
        graph_opts,
        graph_settings,
        graph_unit,
        graph_dual,
        main_content,
        main_column,
        graph_controls,
        stats,
        stats_footer,
        left_footer,
        hover_slot,
    } = props;
    // Value-at-cursor formatter for the hover tooltip, keyed to the typed
    // graph family. Network history samples are decimal MB/s coordinates and
    // are projected back through the selected unit preference here.
    let fmt = move |value| format_graph_value(graph_unit, value);
    let summary_fmt = move |value| format_graph_value(graph_unit, value);
    // Value-badge formatter for the top-right pill, in the same unit. `None` for
    // an unknown unit leaves the badge off but keeps gradient_fill + ref_lines.
    let badge_fmt: Option<fn(f32) -> String> = match graph_unit {
        GraphUnit::Percent => Some(badge_pct),
        GraphUnit::Temperature => Some(badge_temperature),
        GraphUnit::Megahertz => Some(badge_mhz),
        GraphUnit::NetworkRate(units) => Some(network_badge_format(units)),
        GraphUnit::DriveRate(units) => Some(drive_badge_format(units)),
        GraphUnit::Rpm => Some(badge_rpm),
        GraphUnit::Watts => Some(badge_watts),
    };
    // Batch-8 aesthetics for every big perf graph routed through this helper
    // (disk / network / GPU main graphs): vertical gradient fill, emphasized
    // 25/50/75/100% reference rules, and a top-right current-value pill.
    // Additive over whatever `graph_opts` the caller supplied (e.g. an auto-scaled
    // `max`); the caller's `max` is preserved by struct-update order.
    let graph_opts = GraphOpts {
        gradient_fill: true,
        ref_lines: true,
        value_badge: true,
        badge_fmt,
        ..graph_opts
    }
    .with_settings(graph_settings);
    let graph_samples = if graph_opts.sliding {
        latest_samples_rc_for_slide(graph_samples, graph_opts.data_points)
    } else {
        latest_samples_rc(graph_samples, graph_opts.data_points)
    };
    // Two-series graphs (disk read/write, NIC rx/tx): the family token strokes
    // the primary direction, the tinted companion the secondary, and a mini
    // legend names the pairing above the card. Both directions pass through
    // ONE shared `graph_opts.max` so their amplitudes stay comparable; each
    // keeps its own gap evidence inside the element.
    let dual_state_samples = graph_dual.as_ref().map(|dual| {
        (
            Rc::clone(&dual.primary_samples),
            Rc::clone(&dual.secondary_samples),
        )
    });
    let (graph, legend) = match graph_dual {
        Some(dual) => {
            let (primary_color, secondary_color) = dual_series_colors(graph_color.into());
            let graph = graph_element_hover_dual(
                "main-graph",
                graph_id.clone(),
                Rc::clone(&dual.primary_samples),
                primary_color,
                dual.primary_label.to_owned(),
                GraphSecondarySeries {
                    samples: Rc::clone(&dual.secondary_samples),
                    base: secondary_color,
                    label: dual.secondary_label.to_owned(),
                },
                graph_opts,
                fmt,
                hover_slot.clone(),
            );
            let legend = elements::graph_legend(
                theme,
                &[
                    elements::GraphLegendEntry {
                        color: primary_color,
                        label: dual.primary_label.to_owned(),
                    },
                    elements::GraphLegendEntry {
                        color: secondary_color,
                        label: dual.secondary_label.to_owned(),
                    },
                ],
            );
            (graph, Some(legend))
        }
        None => (
            graph_element_hover(
                "main-graph",
                graph_id,
                Rc::clone(&graph_samples),
                graph_color.into(),
                graph_opts,
                fmt,
                hover_slot.clone(),
            ),
            None,
        ),
    };
    let mut col = stats_panel(theme, stats);
    if let Some(f) = stats_footer {
        col = col.child(div().mt(tokens::SPACE_6).child(f));
    }
    let show_aggregate_summary = matches!(&main_content, MainContent::AggregateGraph);
    let main_center = match main_content {
        MainContent::AggregateGraph => {
            // First-frame state comes from the directions' UNION of evidence
            // on a two-series graph: the summed lane can be all-gap while one
            // direction is measured (its per-tick sum requires both
            // directions), and that must not blanket a readable curve.
            let card = match &dual_state_samples {
                Some((primary, secondary)) => {
                    elements::graph_card_with_dual_state(theme, graph, primary, secondary)
                }
                None => elements::graph_card_with_state(theme, graph, &graph_samples),
            }
            .min_h(MAIN_GRAPH_MIN_HEIGHT)
            .w_full();
            match legend {
                // The wrapper carries the same flex-1 growth the card had as
                // a direct child, so the legend is paid for out of the
                // free-space budget, not out of the card's minimum height.
                Some(legend) => div()
                    .flex()
                    .flex_col()
                    .gap(tokens::SPACE_4)
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .min_w(px(0.0))
                    .child(legend)
                    .child(card.flex_1())
                    .into_any_element(),
                None => card.into_any_element(),
            }
        }
        MainContent::EngineInventory(inventory) => div()
            .flex_1()
            .min_w(px(0.0))
            .min_h(MAIN_GRAPH_MIN_HEIGHT)
            .w_full()
            .child(inventory)
            .into_any_element(),
    };
    // The main column scrolls independently of the pinned stats panel so a
    // tall device page (disk partitions, SMART footer, …) never paints past
    // the window edge. The hover tooltip stays a sibling of the scroll
    // container so its deferred+anchored label is not clipped.
    let mut main_body = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_10)
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        // Keep a real internal breathing band before the pinned stats rail.
        // The rail itself owns the divider and its left padding; this inset
        // keeps the chart card from visually touching that boundary.
        .pr(tokens::SPACE_16)
        .child(performance_title_row(theme, title, subtitle))
        .children(graph_controls)
        .child(main_center);
    if show_aggregate_summary
        && let Some(summary) = graph_summary_row(theme, &graph_samples, &summary_fmt)
    {
        #[cfg(any(test, feature = "test-support"))]
        let summary = summary.debug_selector(|| "tm-perf-aggregate-graph-summary".to_string());
        main_body = main_body.child(summary);
    }
    if let Some(panel) = left_footer {
        main_body = main_body.child(panel);
    }
    let main_body = match main_column {
        MainColumnLayout::Scrollable => scroll_region_with_rail(
            "perf-left-scroll",
            "tm-perf-left-scroll",
            "perf-left-scrollbar",
            "tm-perf-left-scrollbar",
            left_scroll,
            theme.palette(),
            main_body.flex_auto().flex_shrink_0(),
        ),
        MainColumnLayout::Viewport => main_body
            .id("perf-main-viewport")
            .flex_1()
            .h_full()
            .debug_selector(|| "tm-perf-main-viewport".to_string()),
    };
    let mut left = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_10)
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .child(main_body);
    // Hover tooltip: page-level singleton (one slot, one cursor). Sibling of the
    // graph card so the deferred+anchored label escapes `overflow_hidden`.
    if let Some((pos, text)) = graph_hover(hover_slot) {
        left = left.child(elements::tooltip_overlay(theme, &text, pos));
    }
    performance_split(theme, left, col, stats_scroll)
}

/// Render a second typed graph below a primary Performance graph.
///
/// Battery power and fan temperature are optional upstream metrics. Keeping
/// this projection at the GPUI edge lets the application/history layers remain
/// renderer-neutral while preserving explicit gaps when the provider cannot
/// measure the optional channel.
pub(super) struct SecondaryGraphCardProps<'a> {
    pub(super) theme: &'a Theme,
    pub(super) id: ElementId,
    /// Per-series slide identity, independent of the shared debug/state id.
    pub(super) slide_key: ElementId,
    pub(super) title: String,
    /// Shared, generation-cached series (see `MainWithStatsProps`).
    pub(super) samples: Rc<[f32]>,
    pub(super) color: Color,
    pub(super) graph_opts: GraphOpts,
    pub(super) graph_settings: GraphSettings,
    pub(super) graph_unit: GraphUnit,
    pub(super) hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
}

pub(super) fn secondary_graph_card(props: SecondaryGraphCardProps<'_>) -> Div {
    let SecondaryGraphCardProps {
        theme,
        id,
        slide_key,
        title,
        samples,
        color,
        graph_opts,
        graph_settings,
        graph_unit,
        hover_slot,
    } = props;
    let fmt = move |value| format_graph_value(graph_unit, value);
    let badge_fmt: Option<fn(f32) -> String> = match graph_unit {
        GraphUnit::Percent => Some(badge_pct),
        GraphUnit::Temperature => Some(badge_temperature),
        GraphUnit::Megahertz => Some(badge_mhz),
        GraphUnit::NetworkRate(units) => Some(network_badge_format(units)),
        GraphUnit::DriveRate(units) => Some(drive_badge_format(units)),
        GraphUnit::Rpm => Some(badge_rpm),
        GraphUnit::Watts => Some(badge_watts),
    };
    let graph_opts = GraphOpts {
        gradient_fill: true,
        ref_lines: true,
        value_badge: true,
        badge_fmt,
        ..graph_opts
    }
    .with_settings(graph_settings);
    let graph_samples = if graph_opts.sliding {
        latest_samples_rc_for_slide(samples, graph_opts.data_points)
    } else {
        latest_samples_rc(samples, graph_opts.data_points)
    };
    let graph = graph_element_hover(
        id.clone(),
        slide_key,
        Rc::clone(&graph_samples),
        color.into(),
        graph_opts,
        fmt,
        hover_slot.clone(),
    );
    let graph_card = elements::graph_card_with_state(theme, graph, &graph_samples)
        .flex_auto()
        .flex_shrink_0()
        .min_w(px(0.0))
        .min_h(SECONDARY_GRAPH_MIN_HEIGHT)
        .w_full();
    let mut section = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_6)
        .flex_auto()
        .flex_shrink_0()
        .min_w(px(0.0))
        .min_h(SECONDARY_GRAPH_MIN_HEIGHT)
        .w_full()
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(title),
        )
        .child(graph_card);
    if let Some(summary) = graph_summary_row(theme, &graph_samples, &|value| {
        format_graph_value(graph_unit, value)
    }) {
        section = section.child(summary);
    }
    #[cfg(any(test, feature = "test-support"))]
    {
        section = section.debug_selector(move || format!("tm-perf-secondary-graph:{id}"));
    }
    section
}

fn format_graph_value(unit: GraphUnit, value: f32) -> String {
    match unit {
        GraphUnit::Percent => format!("{value:.0}%"),
        GraphUnit::NetworkRate(units) => units.format_network_graph_megabytes(value),
        GraphUnit::DriveRate(units) => units.format_drive_graph_megabytes(value),
        GraphUnit::Rpm => taskmanager_shell::presentation::fan_rpm(value),
        GraphUnit::Watts => taskmanager_shell::presentation::power_w(value),
        GraphUnit::Temperature => taskmanager_shell::presentation::temperature_c(value),
        GraphUnit::Megahertz => taskmanager_shell::presentation::megahertz(value),
    }
}

/// Semantic Performance heading with a leading identity slot and a trailing
/// model/context slot. The trailing slot owns every remaining pixel, so a
/// short leading label cannot strand a large unused band in a wide viewport.
pub(crate) fn performance_title_row(theme: &Theme, title: String, subtitle: String) -> Div {
    let row = div()
        .flex()
        .items_center()
        .w_full()
        .min_w(px(0.0))
        .gap(tokens::SPACE_12)
        .child(
            elements::truncated_text(&title)
                .debug_selector(|| "tm-perf-title-text".to_string())
                // Let long model/interface names truncate inside the title
                // slot. The slot may grow on wide pages, but it must yield to
                // the graph column rather than widening the whole split.
                .flex_1()
                .min_w(px(0.0))
                .max_w(PERFORMANCE_TITLE_MAX_WIDTH)
                .text_size(tokens::FONT_26)
                .font_weight(tokens::FONT_WEIGHT_EXTRA_BOLD.into())
                .text_color(theme.fg),
        )
        .child(
            elements::truncated_text(&subtitle)
                .debug_selector(|| "tm-perf-subtitle-text".to_string())
                .flex_1()
                .flex_shrink()
                .min_w(px(0.0))
                .text_right()
                .text_size(tokens::FONT_16)
                .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                .text_color(theme.fg_dim),
        );
    // Geometry breakpoint on the page header — the render-path assertion looks
    // this up to prove a perf page paints its chrome when device data exists.
    #[cfg(any(test, feature = "test-support"))]
    let row = row.debug_selector(|| "tm-perf-title".to_string());
    row
}

/// The Performance stat panel. One rendering of the missing-value contract
/// for every device page: `None` values draw the shared dash in the dim
/// foreground so an uncollected field reads as quiet, present data reads in
/// the full foreground.
pub(super) fn stats_panel(theme: &Theme, stats: Vec<StatRow>) -> Div {
    let mut col = div()
        .w_full()
        .max_w(PERFORMANCE_STATS_WIDTH)
        .min_w(px(0.0))
        .h_full()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_10);
    // Geometry breakpoint on the stats column root.
    #[cfg(any(test, feature = "test-support"))]
    {
        col = col.debug_selector(|| "tm-perf-stats-panel".to_string());
    }
    for (i, row) in stats.into_iter().enumerate() {
        // One missing-value rendering for every producer: the shared dash,
        // dimmed so an uncollected field reads quieter than present data.
        // Text and Pair draw identically here — the variant types the
        // producer's folding semantics (plain vs used/total), not the ink.
        let label = row.label().to_owned();
        let (missing, v) = match row.value() {
            Some(v) => (false, v.to_owned()),
            None => (true, missing_value()),
        };
        let row = KeyValueRow::new(label, v, theme.palette())
            .label_width(PERFORMANCE_STATS_LABEL_WIDTH)
            .value_color(if missing { theme.fg_dim } else { theme.fg })
            .value_debug_selector(format!("tm-perf-stat-value:{i}"))
            .selectable_value(("perf-stat-value", i))
            .render();
        col = col.child(stat_row_with_selector(row, i));
    }
    col
}

/// Geometry breakpoint per data-driven stats row — the render-path assertion
/// counts these to prove the stats column grows with the snapshot (e.g.
/// committed/zram/zswap rows only exist with data). Noop outside test support.
#[cfg(any(test, feature = "test-support"))]
fn stat_row_with_selector(row: Div, i: usize) -> Div {
    row.debug_selector(move || format!("tm-perf-stat:{i}"))
}

#[cfg(not(any(test, feature = "test-support")))]
fn stat_row_with_selector(row: Div, _i: usize) -> Div {
    row
}
