//! Shared graph card and legend elements.

use crate::gpui_app::graph::{
    GraphCacheHandle, GraphOpts, GraphSampleState, GraphSettings, graph_element, graph_sample_state,
};
use gpui::{Div, ElementId, InteractiveElement, IntoElement, ParentElement, Styled, div, px};
use std::rc::Rc;
use taskmanager_application::i18n;
use taskmanager_theme::Color;
use taskmanager_theme::tokens;
use taskmanager_theme::{GlassTier, Theme};
use taskmanager_ui::primitives::card_surface::CardSurface;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::font_weight;
use taskmanager_ui::theme_binding::hsla;
use taskmanager_ui::theme_binding::rem_length;
use taskmanager_ui::theme_binding::rgba;

/// The recurring graph-container wrapper used by every Performance graph card
/// (the shared `render_chart` headline/secondary tiers, and cpu_view.rs per-core
/// grid plus headline). The plot is inset from the rounded surface because
/// GPUI's overflow mask is rectangular, and inline charts cast no shadow.
///
/// Returns a `Div` (not `impl IntoElement`) so callers can keep chaining layout
/// (e.g. an absolute overlay label on top of the graph, as cpu_view per-core does).
pub fn graph_card(theme: &Theme, graph: impl IntoElement) -> Div {
    CardSurface::unpadded(theme.palette())
        .glass(GlassTier::Card)
        .radius(tokens::card_radius(theme))
        .inset()
        .render()
        .flex_1()
        .min_h(px(0.0))
        .overflow_hidden()
        .child(
            div()
                .size_full()
                .p(rem_length(tokens::rounded_content_inset(
                    tokens::card_radius(theme),
                )))
                .child(graph),
        )
}

/// Graph card with an honest first-frame status overlay.
///
/// A blank canvas is ambiguous when a provider has not published its first
/// sample yet or has explicitly reported a gap for every slot. The overlay is
/// intentionally a small centered label: it preserves the grid/card geometry,
/// leaves the graph's color identity intact, and makes the state readable at
/// both wide and compact sizes without fabricating a zero trace.
pub fn graph_card_with_state(theme: &Theme, graph: impl IntoElement, samples: &[f32]) -> Div {
    graph_card_with_explicit_state(theme, graph, graph_sample_state(samples))
}

/// The two-series variant of [`graph_card_with_state`]: the first-frame state
/// is classified over the UNION of the two directions' evidence (see
/// `graph::graph_dual_sample_state`), so a measured read direction is not
/// mislabeled unavailable when the summed lane or the write direction holds
/// only gaps.
pub fn graph_card_with_dual_state(
    theme: &Theme,
    graph: impl IntoElement,
    primary: &[f32],
    secondary: &[f32],
) -> Div {
    graph_card_with_explicit_state(
        theme,
        graph,
        crate::gpui_app::graph::graph_dual_sample_state(primary, secondary),
    )
}

fn graph_card_with_explicit_state(
    theme: &Theme,
    graph: impl IntoElement,
    state: GraphSampleState,
) -> Div {
    let mut host = div().relative().size_full().child(graph);
    if state != GraphSampleState::Measured {
        let label = match state {
            GraphSampleState::Collecting => i18n::t("common.collecting_telemetry"),
            GraphSampleState::Unavailable => i18n::t("dashboard.unavailable"),
            GraphSampleState::Measured => "",
        };
        host = host.child(
            div()
                .id("tm-graph-state")
                .debug_selector(|| "tm-graph-state".to_string())
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    CardSurface::unpadded(theme.palette())
                        .glass(GlassTier::Card)
                        .radius(tokens::control_radius(theme))
                        .inset()
                        .render()
                        .px(definite_length(tokens::SPACE_12))
                        .py(definite_length(tokens::SPACE_6))
                        .text_size(font_size(tokens::FONT_12))
                        .text_color(hsla(theme.fg_dim))
                        .child(label),
                ),
        );
    }
    graph_card(theme, host)
}

/// The mini variant of the graph-card family: one density chart cell for a
/// grid surface (CPU per-core matrix, GPU engine inventory).
///
/// This is the ONE mini-cell assembly: gradient fill only (no reference
/// rules, value pill, or hover surface — a density cell reads its value from
/// the overlay label), the shared first-frame state overlay, and the single
/// absolute top-left label style. Callers chain their grid sizing
/// (`.h_full()` / `.size_full()`) and their test-support identity selector.
pub(crate) fn mini_graph_cell(
    theme: &Theme,
    id: impl Into<ElementId>,
    samples: Rc<[f32]>,
    color: Color,
    label: &str,
    settings: GraphSettings,
    cache: GraphCacheHandle,
) -> Div {
    let opts = GraphOpts {
        gradient_fill: true,
        ..GraphOpts::default()
    }
    .with_settings(settings);
    // The element tail-limits its own window; the state classification reads
    // the caller's full generation-scoped window.
    graph_card_with_state(
        theme,
        div()
            .size_full()
            .debug_selector(|| "tm-mini-graph-plot".to_string())
            .child(graph_element(
                id,
                Rc::clone(&samples),
                rgba(color),
                opts,
                cache,
            )),
        &samples,
    )
    .child(
        div()
            .absolute()
            .top(definite_length(tokens::SPACE_4))
            .left(definite_length(tokens::SPACE_6))
            .text_size(font_size(tokens::FONT_10))
            .font_weight(font_weight(tokens::FONT_WEIGHT_BOLD))
            .text_color(hsla(theme.fg_dim))
            .child(label.to_owned()),
    )
}

/// One entry of a graph legend: the series' stroke color and its localized
/// direction label ("Read"/"Write", "Receive"/"Send").
pub struct GraphLegendEntry {
    pub color: gpui::Rgba,
    pub label: String,
}

/// The mini legend above a two-series graph card: one color swatch + label
/// per series, in the caller's order (primary first). This is the GPUI
/// component-language rendering of the semantic iced draws into its canvas
/// (`device_chart::multi::draw_chart_legend`) — swatch quads and shaped text
/// come from the div/text system, not the paint closure, so legend relabels
/// never touch the tessellated graph scene.
pub fn graph_legend(theme: &Theme, entries: &[GraphLegendEntry]) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_end()
        .gap(definite_length(tokens::SPACE_12))
        .w_full()
        .min_w(px(0.0))
        .text_size(font_size(tokens::FONT_11))
        .debug_selector(|| "tm-graph-legend".to_string());
    for (index, entry) in entries.iter().enumerate() {
        row = row.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(definite_length(tokens::SPACE_4))
                .child(
                    div()
                        .size(px(8.0))
                        .rounded(px(2.0))
                        .bg(entry.color)
                        .debug_selector(move || format!("tm-graph-legend-swatch:{index}")),
                )
                .child(
                    div()
                        .text_color(hsla(theme.fg_dim))
                        .child(entry.label.clone())
                        .debug_selector(move || format!("tm-graph-legend-label:{index}")),
                ),
        );
    }
    row
}
