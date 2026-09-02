//! Shared graph card and legend elements.

use super::{CARD_SHADOW_AMBIENT_ALPHA, CARD_SHADOW_AMBIENT_BLUR, CARD_SHADOW_AMBIENT_DROP};
use crate::gpui_app::graph::{
    GraphCacheHandle, GraphOpts, GraphSampleState, GraphSettings, graph_element, graph_sample_state,
};
use gpui::{
    BoxShadow, Div, ElementId, InteractiveElement, IntoElement, ParentElement, Point, Styled, div,
    px,
};
use std::rc::Rc;
use taskmanager_application::i18n;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

/// The card shadow's ambient layer: the share of the ink alpha, the drop, and
/// the blur radius. 2026-08 稳固效果 policy (owner: "很深的 blur 效果很糟糕")
/// — the pre-change y4/blur16 halo read as heavy blur on EVERY graph card at
/// once and muddied the gaps between adjacent cards; separation now comes
/// from the tone ladder (window → sidebar → card fills + 1px border) and the
/// shadow is only a whisper of lift. Shared with the shadow test so the
/// contract lives in one place.
/// The soft two-layer card shadow: a low, low-opacity ambient blur plus a
/// tight, high-opacity edge blur — both painted in the theme's single
/// `card_shadow` ink (the edge layer carries the full ink alpha, the ambient
/// layer a fixed share of it — see the `CARD_SHADOW_AMBIENT_*` constants).
/// Cards and dashboard tiles read
/// through this helper. Dense list rows (the 10k-process table) NEVER do —
/// per-row shadows would add a shadow pass per row on every scroll frame
/// (performance discipline, see tokens.rs motion policy).
pub fn card_shadow(t: &Theme) -> Vec<BoxShadow> {
    let ink = t.card_shadow();
    vec![
        BoxShadow {
            color: taskmanager_ui::theme_binding::hsla(
                ink.with_alpha(ink.a * CARD_SHADOW_AMBIENT_ALPHA),
            ),
            offset: Point::new(px(0.0), px(CARD_SHADOW_AMBIENT_DROP)),
            blur_radius: px(CARD_SHADOW_AMBIENT_BLUR),
            spread_radius: px(0.0),
        },
        BoxShadow {
            color: taskmanager_ui::theme_binding::hsla(ink),
            offset: Point::new(px(0.0), px(1.0)),
            blur_radius: px(4.0),
            spread_radius: px(0.0),
        },
    ]
}

/// The recurring graph-container wrapper used by every Performance graph card
/// (the shared `render_chart` headline/secondary tiers, and cpu_view.rs per-core
/// grid plus headline). Pure layout helper — a flex-filling, rounded, 1px-bordered
/// card surfaced in the theme's elevated card fill (`Theme::card_surface`) that
/// clips its graph to the rounded corners via `overflow_hidden`. Carries the
/// two-layer [`card_shadow`]. Collapses the
/// 4 inline copies into one call.
///
/// Returns a `Div` (not `impl IntoElement`) so callers can keep chaining layout
/// (e.g. an absolute overlay label on top of the graph, as cpu_view per-core does).
pub fn graph_card(theme: &Theme, graph: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_h(px(0.0))
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::card_radius(theme),
        ))
        .border(px(1.0))
        .border_color(taskmanager_ui::theme_binding::hsla(theme.border))
        .bg(taskmanager_ui::theme_binding::fill(theme.card_surface()))
        .shadow(card_shadow(theme))
        .overflow_hidden()
        .child(graph)
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
                    div()
                        .px(taskmanager_ui::theme_binding::definite_length(
                            tokens::SPACE_12,
                        ))
                        .py(taskmanager_ui::theme_binding::definite_length(
                            tokens::SPACE_6,
                        ))
                        .rounded(taskmanager_ui::theme_binding::absolute(
                            tokens::control_radius(theme),
                        ))
                        .border_1()
                        .border_color(taskmanager_ui::theme_binding::hsla(theme.border))
                        .bg(taskmanager_ui::theme_binding::fill(theme.card_surface()))
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
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
    color: taskmanager_theme::Color,
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
        graph_element(
            id,
            Rc::clone(&samples),
            taskmanager_ui::theme_binding::rgba(color),
            opts,
            cache,
        ),
        &samples,
    )
    .child(
        div()
            .absolute()
            .top(px(4.0))
            .left(px(6.0))
            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_10))
            .font_weight(taskmanager_ui::theme_binding::font_weight(
                tokens::FONT_WEIGHT_BOLD,
            ))
            .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
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
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .w_full()
        .min_w(px(0.0))
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
        .debug_selector(|| "tm-graph-legend".to_string());
    for (index, entry) in entries.iter().enumerate() {
        row = row.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_4,
                ))
                .child(
                    div()
                        .size(px(8.0))
                        .rounded(px(2.0))
                        .bg(entry.color)
                        .debug_selector(move || format!("tm-graph-legend-swatch:{index}")),
                )
                .child(
                    div()
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                        .child(entry.label.clone())
                        .debug_selector(move || format!("tm-graph-legend-label:{index}")),
                ),
        );
    }
    row
}
