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
    AnyElement, Div, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels, Styled,
    div, px,
};

use crate::gpui_app::elements;
use crate::gpui_app::formatting::GraphUnit;
use crate::gpui_app::graph::{
    GraphCacheHandle, GraphHover, GraphOpts, GraphSecondarySeries, GraphSettings,
    dual_series_colors, graph_element_hover, graph_element_hover_dual, graph_hover,
};
use crate::gpui_app::perf_views::{
    badge_pct, badge_rpm, badge_temperature, badge_watts, drive_badge_format, graph_summary_row,
    network_badge_format,
};
use crate::gpui_app::root::responsive::{
    PERFORMANCE_STATS_MIN_WIDTH, PerformanceDetailsPresentation, PerformancePageBudget,
    PerformanceVerticalRunway,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::tokens;
use taskmanager_theme::{Color, Length, Theme};

mod stats;
pub(super) use stats::stats_panel;
mod composition;
use composition::{performance_split, performance_stack};

/// The shared main graph lives in a flex viewport that owns the page height.
/// A plain intrinsic-height body would leave the graph with no definite
/// height to grow into; the tier keeps this floor at the shared layout
/// boundary so no device page can drift into a different invisible-chart fix.
const MAIN_GRAPH_MIN_HEIGHT: Length = Length(180.0);
const SECONDARY_GRAPH_MIN_HEIGHT: Length = Length(140.0);
const COMPACT_GRAPH_MIN_HEIGHT: Length = Length(72.0);
const COMPACT_GRAPH_HEIGHT: Pixels = px(72.0);
const COMPACT_GRAPH_SECTION_HEIGHT: Pixels = px(94.0);
const PERFORMANCE_TITLE_MAX_WIDTH: Length = Length(400.0);
const PERFORMANCE_STATS_STACK_HEIGHT: Pixels = px(220.0);
/// Inner trailing breathing room for the pinned statistics rail. The rail is
/// flush with the page edge by contract, but its text content is not: without
/// this inset a right-aligned value reaches the window boundary and a long
/// observation is clipped by the parent surface instead of truncating inside
/// the value column.
const PERFORMANCE_STATS_TRAILING_INSET: Pixels = px(12.0);

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
    /// A second chart below the headline (battery power, fan temperature, or disk
    /// active time): `flex_auto` with a `SECONDARY_GRAPH_MIN_HEIGHT` floor and a
    /// small caption.
    Secondary,
    /// A deliberately flat auxiliary chart (GPU memory) that sits below the
    /// engine inventory. It keeps a compact fixed height and no summary row so
    /// the GPU page has one clear bottom memory strip.
    Compact,
}

impl ChartTier {
    const fn min_height(self) -> Length {
        match self {
            Self::Headline => MAIN_GRAPH_MIN_HEIGHT,
            Self::Secondary => SECONDARY_GRAPH_MIN_HEIGHT,
            Self::Compact => COMPACT_GRAPH_MIN_HEIGHT,
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
    /// element identity). Device pages use `"main-graph"` for the aggregate
    /// headline so every page keeps the shared central surface address.
    id: ElementId,
    /// Per-series slide identity, traveling with the device (not the page
    /// slot) so switching devices cannot inherit another series' slide
    /// timing.
    slide_key: ElementId,
    /// Small caption above the card (memory "Swap", GPU memory).
    title: Option<String>,
    color: Color,
    unit: GraphUnit,
    max: f32,
    series: ChartSeries<'a>,
    tier: ChartTier,
    /// Optional ceiling for a headline card, used when the page's companion
    /// band is visible: the aggregate chart is "small by default" while the
    /// per-core matrix carries the page, and fills the viewport when the
    /// matrix hides.
    max_height: Option<Pixels>,
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
            max_height: None,
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
            max_height: None,
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
            max_height: None,
        }
    }

    /// A compact auxiliary chart with a caption and a short flat card.
    pub(crate) fn compact(
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
            tier: ChartTier::Compact,
            max_height: None,
        }
    }

    /// Override the value scale (dynamic peaks, rate ceilings).
    pub(crate) fn with_max(mut self, max: f32) -> Self {
        self.max = max;
        self
    }

    /// Attach a headline caption (for example memory "Swap").
    pub(crate) fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Cap the headline card's height (headline-stays-small contract: see
    /// [`ChartSpec::max_height`]).
    pub(crate) fn with_max_height(mut self, max_height: Pixels) -> Self {
        self.max_height = Some(max_height);
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
        GraphUnit::NetworkRate(units) => network_badge_format(units),
        GraphUnit::DriveRate(units) => drive_badge_format(units),
        GraphUnit::Rpm => badge_rpm,
        GraphUnit::Watts => badge_watts,
    }
}

fn format_graph_value(unit: GraphUnit, value: f32) -> String {
    match unit {
        GraphUnit::Percent => format!("{value:.0}%"),
        GraphUnit::NetworkRate(units) => {
            crate::gpui_app::formatting::format_network_graph_megabytes(units, value)
        }
        GraphUnit::DriveRate(units) => {
            crate::gpui_app::formatting::format_drive_graph_megabytes(units, value)
        }
        GraphUnit::Rpm => taskmanager_shell::presentation::fan_rpm(value),
        GraphUnit::Watts => taskmanager_shell::presentation::power_w(value),
        GraphUnit::Temperature => taskmanager_shell::presentation::temperature_c(value),
    }
}

/// Tail-limit a shared series to the configured window, preserving the `Rc`
/// identity when the window already fits (the one UI-only-frame copy rule).
fn limited_window(
    settings: GraphSettings,
    samples: Rc<[f32]>,
    graph_cache: &GraphCacheHandle,
) -> Rc<[f32]> {
    graph_cache
        .borrow_mut()
        .latest_samples(samples, settings.data_points, settings.sliding_graphs)
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
    graph_cache: GraphCacheHandle,
) -> Div {
    let unit = spec.unit;
    let fmt = move |value: f32| format_graph_value(unit, value);
    let graph_opts = spec
        .graph_opts(badge_formatter(unit))
        .with_settings(settings);
    let caption_font = match spec.tier {
        ChartTier::Headline => tokens::FONT_13,
        ChartTier::Secondary => tokens::FONT_12,
        ChartTier::Compact => tokens::FONT_11,
    };
    let mut section = div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .w_full()
        .min_w(px(0.0));
    section = match spec.tier {
        // A headline section contains more than its graph card: dual charts
        // also own a legend and every chart owns its summary row. Keeping the
        // section's intrinsic basis and disabling shrink makes that complete
        // contract the flex boundary. Without it, the parent can allocate a
        // zero-height section while the card keeps its 180px minimum, causing
        // the below band to be positioned on top of the card at compact sizes.
        //
        // A CAPPED headline (companion-band mode) must YIELD, not hold: when
        // the viewport runs short, the section shrinks so the card descends
        // toward its companion floor and the fixed-floor rows below are
        // never clipped by the viewport. The cap→floor span is the safety
        // margin that absorbs fit-estimate error.
        ChartTier::Headline => match spec.max_height {
            // Companion mode: the section NEVER grows — a grown section
            // around a capped card would pile dead space under the card.
            // It only shrinks, yielding the card toward its companion floor.
            Some(_) => section.flex_shrink(),
            // Headline-only pages still grow into the viewport, but the
            // section must yield when a page also admits a lower band. The
            // card's tier floor remains the hard lower bound; a non-shrinking
            // section was the source of the old bottom-overlap behaviour.
            None => section.flex_auto().flex_shrink(),
        },
        ChartTier::Secondary => section
            .flex_auto()
            .min_h(taskmanager_ui::theme_binding::length(
                spec.tier.min_height(),
            )),
        ChartTier::Compact => section.flex_none().h(COMPACT_GRAPH_SECTION_HEIGHT).min_h(
            taskmanager_ui::theme_binding::length(spec.tier.min_height()),
        ),
    };
    if let Some(title) = spec.title.as_deref() {
        section = section.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(caption_font))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(title.to_owned()),
        );
    }
    match spec.series {
        ChartSeries::Single { samples } => {
            let samples = limited_window(settings, samples, &graph_cache);
            let summary_row = graph_summary_row(theme, &samples, &fmt);
            let graph = graph_element_hover(
                spec.id.clone(),
                spec.slide_key,
                Rc::clone(&samples),
                taskmanager_ui::theme_binding::rgba(spec.color),
                graph_opts,
                fmt,
                hover_slot.clone(),
                graph_cache.clone(),
            );
            let card = elements::graph_card_with_state(theme, graph, &samples);
            let card = apply_tier_to_card(card, spec.tier, spec.max_height);
            let card = match summary_row
                .filter(|_| {
                    vertical.carries_core_stack() && !matches!(spec.tier, ChartTier::Compact)
                })
                .map(|row| summary_overlay(theme, row))
            {
                Some(overlay) => card.child(tag_summary(overlay, spec.id.clone())),
                None => card,
            };
            section = section.child(tag_card(card, spec.id.clone()));
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
            let aggregate = limited_window(settings, aggregate, &graph_cache);
            let summary_row = graph_summary_row(theme, &aggregate, &fmt);
            let (primary_color, secondary_color) =
                dual_series_colors(taskmanager_ui::theme_binding::rgba(spec.color));
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
                graph_cache.clone(),
            );
            let card = elements::graph_card_with_dual_state(theme, graph, &primary, &secondary);
            let card = apply_tier_to_card(card, spec.tier, spec.max_height);
            let card = match summary_row
                .filter(|_| {
                    vertical.carries_core_stack() && !matches!(spec.tier, ChartTier::Compact)
                })
                .map(|row| summary_overlay(theme, row))
            {
                Some(overlay) => card.child(tag_summary(overlay, spec.id.clone())),
                None => card,
            };
            section = section
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_4,
                ))
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
        }
    }
    #[cfg(any(test, feature = "test-support"))]
    {
        let selector_id = spec.id;
        let secondary = matches!(spec.tier, ChartTier::Secondary);
        let compact = matches!(spec.tier, ChartTier::Compact);
        section = section.debug_selector(move || {
            if compact {
                format!("tm-perf-compact-graph:{selector_id}")
            } else if secondary {
                format!("tm-perf-secondary-graph:{selector_id}")
            } else {
                format!("tm-perf-chart:{selector_id}")
            }
        });
    }
    section
}

/// The chart's latest/avg/peak readout as an overlay pinned to the card's
/// TOP-LEFT corner (the value badge owns the top-right): a full-width row
/// under the card spent vertical space the charts need. The pill background
/// keeps the numbers readable over the grid lines.
fn summary_overlay(theme: &Theme, row: Div) -> Div {
    div().absolute().top(px(6.0)).left(px(8.0)).child(
        row.rounded(taskmanager_ui::theme_binding::absolute(
            tokens::control_radius(theme),
        ))
        .bg(taskmanager_ui::theme_binding::fill(
            theme.card_surface().with_alpha(0.85),
        ))
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_2,
        )),
    )
}

/// Chain the tier's growth/floor contract onto a rendered card.
/// A capped headline is a headline sharing the viewport with its page's
/// companion band (the CPU per-core matrix): its floor drops accordingly.
/// Minimum readable headline height while a lower companion band is present.
/// Page-specific allocators use this shared floor to calculate a continuous
/// headline budget instead of inventing a per-page breakpoint.
pub(crate) const HEADLINE_COMPANION_FLOOR: f32 = 140.0;

fn apply_tier_to_card(card: Div, tier: ChartTier, max_height: Option<Pixels>) -> Div {
    match tier {
        ChartTier::Headline => {
            let card = card.w_full();
            match max_height {
                // Companion mode: fixed companion height, shrinkable to the
                // companion floor under viewport pressure. Never grows — the
                // matrix below owns the surplus.
                Some(max_height) => card
                    .h(max_height)
                    .min_h(px(HEADLINE_COMPANION_FLOOR))
                    .flex_shrink(),
                None => card
                    .flex_1()
                    .min_h(taskmanager_ui::theme_binding::length(tier.min_height())),
            }
        }
        ChartTier::Secondary => card
            .flex_auto()
            .min_w(px(0.0))
            .min_h(taskmanager_ui::theme_binding::length(tier.min_height()))
            .w_full(),
        ChartTier::Compact => card
            .flex_none()
            .h(COMPACT_GRAPH_HEIGHT)
            .min_h(taskmanager_ui::theme_binding::length(tier.min_height()))
            .w_full(),
    }
}

/// Tag the chart's latest/avg/peak summary overlay with its per-chart
/// identity (test-support only).
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

/// The headline surface of a page. Every Performance page declares its
/// headline charts through the shared chart contract; GPU engine inventory is
/// a lower fine-detail band and can never replace the aggregate headline.
pub(crate) enum HeadlineSurface<'a> {
    Charts(Vec<ChartSpec<'a>>),
}

/// Stateless inputs for one Performance device page.
pub(crate) struct PerfPageProps<'a> {
    pub(crate) theme: &'a Theme,
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
    pub(crate) graph_cache: GraphCacheHandle,
    pub(crate) graph_settings: GraphSettings,
    pub(crate) budget: PerformancePageBudget,
}

/// Compose one Performance device page: the fixed main viewport (title,
/// header band, headline surface, below band) beside the pinned stats rail.
/// The main column is ONE fixed viewport (never a scrolling body): headline
/// charts absorb slack through `flex_1`, while optional lower content is
/// admitted only after its complete footprint passes the page fit check. The
/// typed vertical runway drops lower bands and header summaries explicitly
/// before the headline floor is touched. The responsive budget decides the
/// stats-rail presentation; that rail is also static and the hover tooltip
/// stays a sibling of the viewport so its label is never clipped.
pub(crate) fn perf_page(props: PerfPageProps<'_>) -> Div {
    let PerfPageProps {
        theme,
        title,
        subtitle,
        vital_line,
        header_extra,
        headline,
        below,
        stats,
        stats_footer,
        hover_slot,
        graph_cache,
        graph_settings,
        budget,
    } = props;
    let runway = budget.vertical;
    let mut stats_col = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .min_w(px(0.0))
        .w_full()
        .child(stats);
    if let Some(footer) = stats_footer {
        stats_col = stats_col.child(
            div()
                .mt(taskmanager_ui::theme_binding::length(tokens::SPACE_6))
                .child(footer),
        );
    }
    let mut main_body = div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_10,
        ))
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
    //
    // The truncating text MUST live inside a flex-row wrapper (the title
    // row's proven pattern). `truncate()` applied to a bare flex-column
    // child poisons gpui's nowrap text measure: the first measure pass
    // caches a truncated width and the line paints as a bare "…" at every
    // window size (width-independent, seen on the 720x760 and 1920x1080
    // niri captures).
    if let Some(vital) = vital_line {
        let line = div()
            .w_full()
            .min_w(px(0.0))
            .flex_shrink_0()
            .flex()
            .flex_row()
            .child(
                elements::truncated_text(&vital)
                    .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                    .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim)),
            );
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
                render_chart(
                    theme,
                    spec,
                    graph_settings,
                    runway,
                    hover_slot,
                    graph_cache.clone(),
                )
                .into_any_element()
            })
            .collect::<Vec<AnyElement>>(),
    };
    main_body = main_body.children(headline_center);
    // Vertical ladder: the below band is optional content, not an overflow
    // target. It is admitted only by the full Charts runway; Core is a strict
    // headline-only composition, so the fixed viewport never clips the first
    // row of a lower band.
    if let Some(panel) = below.filter(|_| runway.carries_below()) {
        main_body = main_body.child(panel);
    }
    let main_body = main_body
        .id("perf-main-viewport")
        .flex_1()
        .h_full()
        // An owned bottom inset keeps the last row/card clear of the
        // viewport edge. It is part of the shared page contract, so every
        // Performance device receives the same pixel safety margin.
        .pb(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .overflow_hidden()
        .debug_selector(|| PERF_MAIN_VIEWPORT_SELECTOR.to_string());
    let mut left = div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_10,
        ))
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
            performance_split(theme, left, stats_col, budget.stats_width)
        }
        PerformanceDetailsPresentation::Stacked => {
            performance_stack(theme, left, stats_col, budget.stats_width)
        }
    }
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
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .child(
            elements::truncated_text(&title)
                .debug_selector(|| "tm-perf-title-text".to_string())
                // Intrinsic width, shrinkable, capped: a long model name
                // truncates inside its own slot and can never widen the
                // whole split or overlap the context slot.
                .flex_shrink()
                .max_w(taskmanager_ui::theme_binding::length(
                    PERFORMANCE_TITLE_MAX_WIDTH,
                ))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_26))
                .font_weight(taskmanager_ui::theme_binding::font_weight(
                    tokens::FONT_WEIGHT_EXTRA_BOLD,
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg)),
        )
        .child(
            elements::truncated_text(&subtitle)
                .debug_selector(|| "tm-perf-subtitle-text".to_string())
                .flex_grow()
                .flex_shrink()
                .min_w(px(0.0))
                .text_right()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_16))
                .font_weight(taskmanager_ui::theme_binding::font_weight(
                    tokens::FONT_WEIGHT_BOLD,
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim)),
        );
    // Geometry breakpoint on the page header — the render-path assertion looks
    // this up to prove a perf page paints its chrome when device data exists.
    #[cfg(any(test, feature = "test-support"))]
    let row = row.debug_selector(|| "tm-perf-title".to_string());
    row
}
