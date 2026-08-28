//! The ONE Performance-page composition root.
//!
//! Every device page (CPU / Memory / Disk / Network / GPU / Battery / Fan)
//! assembles through [`perf_page`] and declares its charts as [`ChartSpec`]
//! values. A chart's tier derives its entire contract — height floor,
//! interaction layer, first-frame state overlay, aesthetic injection, summary
//! row — so no page can hand-roll a fourth card/height/scroll variant. The
//! raw building blocks (`performance_split`, `stats_panel`, the card
//! assembly) stay module-private on purpose: the only way out is this module.

/// Debug-selector identity of the ONE Performance composition root.
///
/// Shared with the page-family render guard (ADR-039/042) so the chart
/// assertion can never spell a drifted root.
pub const PERF_MAIN_VIEWPORT_SELECTOR: &str = "tm-perf-main-viewport";

use gpui::{
    AnyElement, Div, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    ScrollHandle, Stateful, Styled, div, px,
};

use crate::gpui_app::elements;
use crate::gpui_app::formatting::GraphUnit;
use crate::gpui_app::graph::{
    GraphHover, GraphOpts, GraphSecondarySeries, GraphSettings, dual_series_colors,
    graph_element_hover, graph_element_hover_dual, graph_hover, latest_samples_rc,
    latest_samples_rc_for_slide,
};
use crate::gpui_app::perf_views::{
    badge_mhz, badge_pct, badge_rpm, badge_temperature, badge_watts, drive_badge_format,
    graph_summary_row, network_badge_format,
};
use crate::gpui_app::root::responsive::{
    PERFORMANCE_STATS_MAX_WIDTH, PerformanceDetailsPresentation, PerformancePageBudget,
    PerformanceVerticalRunway,
};
use crate::gpui_app::theme::{Color, Length, Theme, tokens};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_shell::viewmodel::StatRow;
use taskmanager_ui::data::key_value_row::KeyValueRow;
use taskmanager_ui::layout::scroll_region_with_rail;

/// The shared main graph lives in a flex viewport that owns the page height.
/// A plain intrinsic-height body would leave the graph with no definite
/// height to grow into; the tier keeps this floor at the shared layout
/// boundary so no device page can drift into a different invisible-chart fix.
const MAIN_GRAPH_MIN_HEIGHT: Length = Length(180.0);
const SECONDARY_GRAPH_MIN_HEIGHT: Length = Length(140.0);
const PERFORMANCE_TITLE_MAX_WIDTH: Length = Length(400.0);
const PERFORMANCE_STATS_STACK_HEIGHT: Pixels = px(220.0);

// ── chart specification ─────────────────────────────────────────────────────

/// Semantic chart tier. Derives the height floor, growth contract, and
/// chrome (caption style) of a rendered chart card; the interaction layer
/// and aesthetic injection are uniform across tiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChartTier {
    /// The page's headline surface(s): `flex_1` growth sharing the main
    /// viewport's free space, `MAIN_GRAPH_MIN_HEIGHT` floor, legend when the
    /// series is dual.
    Headline,
    /// A second chart below the headline (battery power, fan temperature,
    /// disk active time, GPU metric families): `flex_auto` with a
    /// `SECONDARY_GRAPH_MIN_HEIGHT` floor and a small caption.
    Secondary,
}

impl ChartTier {
    const fn min_height(self) -> Length {
        match self {
            Self::Headline => MAIN_GRAPH_MIN_HEIGHT,
            Self::Secondary => SECONDARY_GRAPH_MIN_HEIGHT,
        }
    }
}

/// The series a chart strokes. Dual lanes (disk read/write, NIC rx/tx) keep
/// the summed aggregate lane separately: it is the summary row's authority
/// while the two directions share one `max` for comparability.
enum ChartSeries<'a> {
    Single {
        samples: Rc<[f32]>,
    },
    Dual {
        aggregate: Rc<[f32]>,
        primary: Rc<[f32]>,
        primary_label: &'a str,
        secondary: Rc<[f32]>,
        secondary_label: &'a str,
    },
}

/// One declared Performance chart. Constructed through the tier
/// constructors; `max` defaults to the percent scale (100) and is overridden
/// with [`ChartSpec::with_max`] for rate/temperature families.
pub(crate) struct ChartSpec<'a> {
    /// Stable element/hover identity (`tm-graph:{id}` and the state overlay's
    /// scroll identity). Device pages use `"main-graph"` for the aggregate
    /// headline so every page keeps the shared central surface address.
    id: ElementId,
    /// Per-series slide identity, traveling with the device (not the page
    /// slot) so switching devices cannot inherit another series' slide
    /// timing.
    slide_key: ElementId,
    /// Small caption above the card (memory "Swap", GPU primary engine).
    title: Option<String>,
    color: Color,
    unit: GraphUnit,
    max: f32,
    series: ChartSeries<'a>,
    tier: ChartTier,
}

/// A two-direction headline series (read/write, rx/tx): the directions share
/// one max for comparability; `aggregate` is the summed lane that owns the
/// summary row.
pub(crate) struct DualLanes<'a> {
    pub(crate) aggregate: Rc<[f32]>,
    pub(crate) primary: Rc<[f32]>,
    pub(crate) primary_label: &'a str,
    pub(crate) secondary: Rc<[f32]>,
    pub(crate) secondary_label: &'a str,
}

impl<'a> ChartSpec<'a> {
    /// One single-series headline chart on the percent scale.
    pub(crate) fn headline(
        id: impl Into<ElementId>,
        slide_key: impl Into<ElementId>,
        samples: Rc<[f32]>,
        color: Color,
        unit: GraphUnit,
    ) -> Self {
        Self {
            id: id.into(),
            slide_key: slide_key.into(),
            title: None,
            color,
            unit,
            max: 100.0,
            series: ChartSeries::Single { samples },
            tier: ChartTier::Headline,
        }
    }

    /// A two-direction headline (read/write, rx/tx) under one shared max.
    pub(crate) fn dual_headline(
        id: impl Into<ElementId>,
        slide_key: impl Into<ElementId>,
        lanes: DualLanes<'a>,
        color: Color,
        unit: GraphUnit,
    ) -> Self {
        let DualLanes {
            aggregate,
            primary,
            primary_label,
            secondary,
            secondary_label,
        } = lanes;
        Self {
            id: id.into(),
            slide_key: slide_key.into(),
            title: None,
            color,
            unit,
            max: 100.0,
            series: ChartSeries::Dual {
                aggregate,
                primary,
                primary_label,
                secondary,
                secondary_label,
            },
            tier: ChartTier::Headline,
        }
    }

    /// A secondary chart with its own caption below the headline surface.
    pub(crate) fn secondary(
        id: impl Into<ElementId>,
        slide_key: impl Into<ElementId>,
        title: String,
        samples: Rc<[f32]>,
        color: Color,
        unit: GraphUnit,
    ) -> Self {
        Self {
            id: id.into(),
            slide_key: slide_key.into(),
            title: Some(title),
            color,
            unit,
            max: 100.0,
            series: ChartSeries::Single { samples },
            tier: ChartTier::Secondary,
        }
    }

    /// Override the value scale (dynamic peaks, rate ceilings).
    pub(crate) fn with_max(mut self, max: f32) -> Self {
        self.max = max;
        self
    }

    /// Attach a headline caption (memory "Swap", GPU primary engine name).
    pub(crate) fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The one aesthetic injection: Batch-8 refinements (gradient fill,
    /// emphasized reference rules, top-right value pill) plus the caller's
    /// scale, applied identically for every tier.
    fn graph_opts(&self, badge_fmt: fn(f32) -> String) -> GraphOpts {
        GraphOpts {
            max: self.max,
            gradient_fill: true,
            ref_lines: true,
            value_badge: true,
            badge_fmt: Some(badge_fmt),
            ..GraphOpts::default()
        }
    }
}

/// The single value-formatter table for a typed graph unit — hover tooltips,
/// value badges, and summary rows all read one mapping.
fn badge_formatter(unit: GraphUnit) -> fn(f32) -> String {
    match unit {
        GraphUnit::Percent => badge_pct,
        GraphUnit::Temperature => badge_temperature,
        GraphUnit::Megahertz => badge_mhz,
        GraphUnit::NetworkRate(units) => network_badge_format(units),
        GraphUnit::DriveRate(units) => drive_badge_format(units),
        GraphUnit::Rpm => badge_rpm,
        GraphUnit::Watts => badge_watts,
    }
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

/// Tail-limit a shared series to the configured window, preserving the `Rc`
/// identity when the window already fits (the one UI-only-frame copy rule).
fn limited_window(settings: GraphSettings, samples: Rc<[f32]>) -> Rc<[f32]> {
    if settings.sliding_graphs {
        latest_samples_rc_for_slide(samples, settings.data_points)
    } else {
        latest_samples_rc(samples, settings.data_points)
    }
}

/// Render one declared chart: card, tier height contract, caption, legend,
/// first-frame state overlay, hover surface, and summary row — assembled in
/// exactly one place for every Performance page. The typed vertical runway
/// decides whether the summary row still composes (it drops, explicitly,
/// before the headline floor is ever touched).
pub(crate) fn render_chart(
    theme: &Theme,
    spec: ChartSpec<'_>,
    settings: GraphSettings,
    vertical: PerformanceVerticalRunway,
    hover_slot: &Rc<RefCell<Option<GraphHover>>>,
) -> Div {
    let unit = spec.unit;
    let fmt = move |value: f32| format_graph_value(unit, value);
    let graph_opts = spec
        .graph_opts(badge_formatter(unit))
        .with_settings(settings);
    let caption_font = match spec.tier {
        ChartTier::Headline => tokens::FONT_13,
        ChartTier::Secondary => tokens::FONT_12,
    };
    let mut section = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_6)
        .w_full()
        .min_w(px(0.0));
    section = match spec.tier {
        ChartTier::Headline => section.flex_1().min_h(px(0.0)),
        ChartTier::Secondary => section.flex_auto().min_h(spec.tier.min_height()),
    };
    if let Some(title) = spec.title.as_deref() {
        section = section.child(
            div()
                .text_size(caption_font)
                .text_color(theme.fg_dim)
                .child(title.to_owned()),
        );
    }
    let summary;
    match spec.series {
        ChartSeries::Single { samples } => {
            let samples = limited_window(settings, samples);
            let summary_row = graph_summary_row(theme, &samples, &fmt);
            let graph = graph_element_hover(
                spec.id.clone(),
                spec.slide_key,
                Rc::clone(&samples),
                spec.color.into(),
                graph_opts,
                fmt,
                hover_slot.clone(),
            );
            let card = elements::graph_card_with_state(theme, graph, &samples);
            let card = apply_tier_to_card(card, spec.tier);
            section = section.child(tag_card(card, spec.id.clone()));
            summary = summary_row;
        }
        ChartSeries::Dual {
            aggregate,
            primary,
            primary_label,
            secondary,
            secondary_label,
        } => {
            // First-frame state comes from the directions' UNION of evidence:
            // the summed lane can be all-gap while one direction is measured.
            let aggregate = limited_window(settings, aggregate);
            let summary_row = graph_summary_row(theme, &aggregate, &fmt);
            let (primary_color, secondary_color) = dual_series_colors(spec.color.into());
            let graph = graph_element_hover_dual(
                spec.id.clone(),
                spec.slide_key,
                Rc::clone(&primary),
                primary_color,
                primary_label.to_owned(),
                GraphSecondarySeries {
                    samples: Rc::clone(&secondary),
                    base: secondary_color,
                    label: secondary_label.to_owned(),
                },
                graph_opts,
                fmt,
                hover_slot.clone(),
            );
            let card = elements::graph_card_with_dual_state(theme, graph, &primary, &secondary);
            let card = apply_tier_to_card(card, spec.tier);
            section = section
                .gap(tokens::SPACE_4)
                .child(elements::graph_legend(
                    theme,
                    &[
                        elements::GraphLegendEntry {
                            color: primary_color,
                            label: primary_label.to_owned(),
                        },
                        elements::GraphLegendEntry {
                            color: secondary_color,
                            label: secondary_label.to_owned(),
                        },
                    ],
                ))
                .child(tag_card(card, spec.id.clone()));
            summary = summary_row;
        }
    }
    if let Some(summary_row) = summary.filter(|_| vertical.carries_core_stack()) {
        section = section.child(tag_summary(summary_row, spec.id.clone()));
    }
    #[cfg(any(test, feature = "test-support"))]
    {
        let selector_id = spec.id;
        let secondary = matches!(spec.tier, ChartTier::Secondary);
        section = section.debug_selector(move || {
            if secondary {
                format!("tm-perf-secondary-graph:{selector_id}")
            } else {
                format!("tm-perf-chart:{selector_id}")
            }
        });
    }
    section
}

/// Chain the tier's growth/floor contract onto a rendered card, then (in test
/// support) tag the summary row that follows the card.
fn apply_tier_to_card(card: Div, tier: ChartTier) -> Div {
    match tier {
        ChartTier::Headline => card.flex_1().min_h(tier.min_height()).w_full(),
        ChartTier::Secondary => card
            .flex_auto()
            .min_w(px(0.0))
            .min_h(tier.min_height())
            .w_full(),
    }
}

/// Tag the chart's latest/avg/peak summary row with its per-chart identity
/// (test-support only).
#[cfg(any(test, feature = "test-support"))]
fn tag_summary(summary: Div, id: ElementId) -> Div {
    summary.debug_selector(move || format!("tm-perf-chart-summary:{id}"))
}

#[cfg(not(any(test, feature = "test-support")))]
fn tag_summary(summary: Div, _id: ElementId) -> Div {
    summary
}

/// Tag the chart card itself — the tier height-contract holder — with its
/// per-chart identity (test-support only).
#[cfg(any(test, feature = "test-support"))]
fn tag_card(card: Div, id: ElementId) -> Div {
    card.debug_selector(move || format!("tm-perf-chart-card:{id}"))
}

#[cfg(not(any(test, feature = "test-support")))]
fn tag_card(card: Div, _id: ElementId) -> Div {
    card
}

// ── page composition root ───────────────────────────────────────────────────

/// The headline surface of a page. Most pages declare one or two stacked
/// headline charts; a standard multi-engine GPU page REPLACES the aggregate
/// chart with the complete engine inventory — the enum prevents an aggregate
/// graph and an engine inventory from being composed accidentally.
pub(crate) enum HeadlineSurface<'a> {
    Charts(Vec<ChartSpec<'a>>),
    Custom(AnyElement),
}

/// Stateless inputs for one Performance device page.
pub(crate) struct PerfPageProps<'a> {
    pub(crate) theme: &'a Theme,
    pub(crate) stats_scroll: ScrollHandle,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    /// One-line distilled fact that keeps the page's meaning when every
    /// other band has degraded (disk capacity, VRAM totals, link state).
    /// Never gated by the vertical ladder — it survives the Floor rung.
    pub(crate) vital_line: Option<String>,
    /// Band under the title row: CPU readouts, Memory composition.
    pub(crate) header_extra: Option<AnyElement>,
    pub(crate) headline: HeadlineSurface<'a>,
    /// Content below the headline charts (per-core matrix, disk panels,
    /// secondary charts) — already inventory-gated by the page.
    pub(crate) below: Option<AnyElement>,
    /// The pinned statistics column body (already composed).
    pub(crate) stats: Div,
    pub(crate) stats_footer: Option<AnyElement>,
    pub(crate) hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
    pub(crate) graph_settings: GraphSettings,
    pub(crate) budget: PerformancePageBudget,
}

/// Compose one Performance device page: the fixed main viewport (title,
/// header band, headline surface, below band) beside the pinned stats rail.
/// The main column is ONE fixed viewport (never a scrolling body): headline
/// charts absorb slack through `flex_1`, secondary content compresses to its
/// tier floor. The typed vertical runway degrades the page's fixed
/// obligations in order — below band and header band/summaries drop
/// explicitly before the headline floor is ever touched — so clipping is the
/// ordered last resort of the overflow-tolerant tail, never a silent
/// default. The responsive budget decides the stats rail presentation; the
/// hover tooltip stays a sibling of the viewport so its label is never
/// clipped.
pub(crate) fn perf_page(props: PerfPageProps<'_>) -> Div {
    let PerfPageProps {
        theme,
        stats_scroll,
        title,
        subtitle,
        vital_line,
        header_extra,
        headline,
        below,
        stats,
        stats_footer,
        hover_slot,
        graph_settings,
        budget,
    } = props;
    let runway = budget.vertical;
    let mut stats_col = stats;
    if let Some(footer) = stats_footer {
        stats_col = stats_col.child(div().mt(tokens::SPACE_6).child(footer));
    }
    let mut main_body = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_10)
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        // Internal breathing band before the pinned stats rail; the rail
        // itself owns the divider and its left padding.
        .pr(px(budget.main_trailing_inset))
        .child(performance_title_row(theme, title, subtitle));
    // The vital line is the page's undroppable one-line fact: unlike the
    // header band it renders at EVERY rung, so even the Floor composition
    // (title + headline) still answers "how full / how fast / how healthy".
    if let Some(vital) = vital_line {
        let line = div()
            .text_size(tokens::FONT_13)
            .text_color(theme.fg_dim)
            .w_full()
            .min_w(px(0.0))
            .truncate()
            .child(vital);
        #[cfg(any(test, feature = "test-support"))]
        let line = line.debug_selector(|| "tm-perf-vital-line".to_string());
        main_body = main_body.child(line);
    }
    // Vertical ladder, Floor rung: the header band (readouts / composition)
    // drops before the headline card is squeezed.
    if let Some(extra) = header_extra.filter(|_| runway.carries_core_stack()) {
        main_body = main_body.child(extra);
    }
    let headline_center = match headline {
        HeadlineSurface::Charts(specs) => specs
            .into_iter()
            .map(|spec| {
                render_chart(theme, spec, graph_settings, runway, hover_slot).into_any_element()
            })
            .collect::<Vec<AnyElement>>(),
        HeadlineSurface::Custom(inventory) => vec![
            div()
                .flex_1()
                .min_w(px(0.0))
                .min_h(MAIN_GRAPH_MIN_HEIGHT)
                .w_full()
                .child(inventory)
                .into_any_element(),
        ],
    };
    main_body = main_body.children(headline_center);
    // Vertical ladder: the below band is the overflow-tolerant tail — it
    // renders from the Core rung up and yields first; anything it cannot
    // fit under Floor is clipped by the fixed viewport as the ordered last
    // resort (ADR-039).
    if let Some(panel) = below.filter(|_| runway.carries_core_stack()) {
        main_body = main_body.child(panel);
    }
    let main_body = main_body
        .id("perf-main-viewport")
        .flex_1()
        .h_full()
        .overflow_hidden()
        .debug_selector(|| PERF_MAIN_VIEWPORT_SELECTOR.to_string());
    let mut left = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_10)
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .child(main_body);
    if let Some((pos, text)) = graph_hover(hover_slot) {
        left = left.child(elements::tooltip_overlay(theme, &text, pos));
    }
    match budget.details {
        PerformanceDetailsPresentation::Hidden => left,
        PerformanceDetailsPresentation::Pinned => {
            performance_split(theme, left, stats_col, stats_scroll, budget.stats_width)
        }
        PerformanceDetailsPresentation::Stacked => {
            performance_stack(theme, left, stats_col, stats_scroll, budget.stats_width)
        }
    }
}

/// Canonical Performance page split: a shrinkable main column and a pinned,
/// non-shrinking scrolling statistics column.
fn performance_split(
    theme: &Theme,
    left: Div,
    stats: Div,
    stats_scroll: ScrollHandle,
    stats_width: f32,
) -> Div {
    let stats = performance_stats_surface(theme, stats, stats_scroll, px(stats_width), true)
        .h_full()
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

/// Narrow-width fallback for the statistics rail. The rail remains available,
/// but it moves below the primary viewport so the graph keeps its minimum
/// readable width instead of being squeezed by two fixed columns.
fn performance_stack(
    theme: &Theme,
    left: Div,
    stats: Div,
    stats_scroll: ScrollHandle,
    stats_width: f32,
) -> Div {
    let stats = performance_stats_surface(theme, stats, stats_scroll, px(stats_width), false)
        .flex_none()
        .w_full()
        .h(PERFORMANCE_STATS_STACK_HEIGHT)
        .max_h(PERFORMANCE_STATS_STACK_HEIGHT)
        .border_t_1()
        .border_color(theme.border)
        .pt(tokens::SPACE_10)
        .debug_selector(|| "tm-perf-stats-surface".to_string());
    div()
        .flex()
        .flex_col()
        .flex_1()
        .w_full()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .size_full()
        .bg(theme.window_bg)
        .child(
            left.flex_1()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .debug_selector(|| "tm-perf-main-surface".to_string()),
        )
        .child(stats)
}

/// Build the one statistics surface used by both pinned and stacked modes.
fn performance_stats_surface(
    theme: &Theme,
    stats: Div,
    stats_scroll: ScrollHandle,
    width: Pixels,
    pinned: bool,
) -> Stateful<Div> {
    // Keep a definite readable width here. A percentage max-width can resolve
    // against an indefinite flex measurement pass as zero in GPUI, collapsing
    // the detail column before the parent receives its final width. The graph
    // remains elastic because the sibling owns the remaining flex space.
    let mut surface = scroll_region_with_rail(
        "perf-stats-scroll",
        "tm-perf-stats-scroll",
        "perf-stats-scrollbar",
        "tm-perf-stats-scrollbar",
        stats_scroll,
        theme.palette(),
        stats,
    )
    // `auto_scroll_region_fill` is a flex-1 viewport by default. A
    // pinned stats column must clear that growth contract before its
    // explicit width is applied; otherwise Taffy can split the available
    // row between the graph and stats column and leave unused space.
    .flex_none()
    .flex_basis(width)
    .w(width)
    .h_full()
    // The split is one continuous workspace. A real divider plus padding on
    // the stats surface replaces a transparent parent gap that exposed the
    // window background as a visual crack between sibling components.
    .bg(theme.window_bg);
    if pinned {
        surface = surface
            .border_l_1()
            .border_color(theme.border)
            .pl(tokens::SPACE_16);
    }
    surface
}

/// Semantic Performance heading with a leading identity slot and a trailing
/// model/context slot. The leading label owns its intrinsic width (capped,
/// shrinkable); the trailing slot grows through `flex_grow` with an auto
/// basis so a short label never strands a large band, and the row wraps
/// instead of clipping when device identity + context cannot share one line
/// on a narrow chart column.
pub(crate) fn performance_title_row(theme: &Theme, title: String, subtitle: String) -> Div {
    let row = div()
        .flex()
        .flex_wrap()
        .items_center()
        .w_full()
        .min_w(px(0.0))
        .gap(tokens::SPACE_12)
        .child(
            elements::truncated_text(&title)
                .debug_selector(|| "tm-perf-title-text".to_string())
                // Intrinsic width, shrinkable, capped: a long model name
                // truncates inside its own slot and can never widen the
                // whole split or overlap the context slot.
                .flex_shrink()
                .max_w(PERFORMANCE_TITLE_MAX_WIDTH)
                .text_size(tokens::FONT_26)
                .font_weight(tokens::FONT_WEIGHT_EXTRA_BOLD.into())
                .text_color(theme.fg),
        )
        .child(
            elements::truncated_text(&subtitle)
                .debug_selector(|| "tm-perf-subtitle-text".to_string())
                .flex_grow()
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

/// The shared Performance stat panel body. One rendering of the
/// missing-value contract for every device page: `None` values draw the
/// shared dash in the dim foreground so an uncollected field reads as quiet,
/// present data reads in the full foreground.
pub(super) fn stats_panel(theme: &Theme, stats: Vec<StatRow>) -> Div {
    let mut col = div()
        .w_full()
        .max_w(px(PERFORMANCE_STATS_MAX_WIDTH))
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
        //
        // Row contract matches the CPU details panel: the label owns the
        // elastic side and truncates; the value keeps its intrinsic width
        // flush-right. A reserved label column would hand long observations
        // (serial numbers, temperature trends) a too-narrow value slot whose
        // right-aligned text clips mid-string at the window edge.
        let label = row.label().to_owned();
        let (missing, v) = match row.value() {
            Some(v) => (false, v.to_owned()),
            None => (true, crate::gpui_app::formatting::missing_value()),
        };
        let row = KeyValueRow::new(label, v, theme.palette())
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
