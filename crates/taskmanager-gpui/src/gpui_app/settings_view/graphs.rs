//! Mission Center Performance graph preferences: points, sliding, and
//! network scale policy. (Smoothing was removed as a preference — the smooth
//! curve is the product's intrinsic rendering, see `graph::GraphOpts`.)

use std::collections::HashMap;

use gpui::{AppContext, Context, Div, Entity, InteractiveElement, ParentElement, Styled, div};

use taskmanager_ui::inputs::slider::{Slider, SliderState};
use taskmanager_ui::inputs::switch::{Switch, SwitchState};

use crate::gpui_app::graph::{
    DEFAULT_GRAPH_DATA_POINTS_CONFIG, GraphSettings, MAX_GRAPH_DATA_POINTS, MIN_GRAPH_DATA_POINTS,
};
use crate::gpui_app::root::RootView;
use crate::gpui_app::theme::{Theme, tokens};
use crate::i18n;

pub(crate) fn init_data_points_slider(
    current: usize,
    cx: &mut Context<RootView>,
) -> Entity<SliderState> {
    cx.new(|cx| {
        let mut state = SliderState::new(
            u16::try_from(MIN_GRAPH_DATA_POINTS).map_or(10.0, f32::from),
            u16::try_from(MAX_GRAPH_DATA_POINTS).map_or(600.0, f32::from),
            cx,
        );
        state.set_step(1.0, cx);
        let current = u16::try_from(current).map_or(
            u16::try_from(DEFAULT_GRAPH_DATA_POINTS_CONFIG).map_or(60.0, f32::from),
            f32::from,
        );
        state.set_value(current, cx);
        state
    })
}

pub(super) fn graph_options_group(
    t: &Theme,
    ent: Entity<RootView>,
    settings: GraphSettings,
    points_slider: Entity<SliderState>,
    switches: &HashMap<&'static str, Entity<SwitchState>>,
    cx: &mut Context<RootView>,
) -> Div {
    let points_readout = settings.data_points.to_string();
    let points_entity = ent.clone();
    let points_slider =
        Slider::new(points_slider, t.palette()).on_change(move |value, _win, cx| {
            points_entity.update(cx, |view, cx| {
                view.set_graph_data_points(points_from_slider(value), cx);
            });
        });

    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(tokens::FONT_13)
                        .text_color(t.fg)
                        .child(i18n::t("settings.graph_data_points")),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_13)
                        .text_color(t.fg_dim)
                        .child(points_readout),
                ),
        )
        .child(points_slider)
        // The telemetry rings are capacity-fixed at startup (Perf-D: stores
        // are constructed to the loaded preference), so RAISING the value
        // takes effect for the live graphs only after a restart — the note
        // says so instead of the slider silently doing nothing.
        .child(
            div()
                .id("tm-graph-points-restart-note")
                .text_size(tokens::FONT_11)
                .text_color(t.fg_dim)
                .child(i18n::t("settings.graph_data_points_restart")),
        )
        .child(graph_switch_row(
            t,
            ent.clone(),
            GraphSwitchSpec {
                id: "sliding-graphs",
                label: i18n::t("settings.sliding_graphs"),
                on: settings.sliding_graphs,
                switch: GraphSwitch::Sliding,
            },
            switches,
            cx,
        ))
        .child(graph_switch_row(
            t,
            ent,
            GraphSwitchSpec {
                id: "network-dynamic-scaling",
                label: i18n::t("settings.network_dynamic_scaling"),
                on: settings.network_dynamic_scaling,
                switch: GraphSwitch::NetworkDynamicScaling,
            },
            switches,
            cx,
        ))
}

#[derive(Clone, Copy)]
enum GraphSwitch {
    Sliding,
    NetworkDynamicScaling,
}

#[derive(Clone, Copy)]
struct GraphSwitchSpec {
    id: &'static str,
    label: &'static str,
    on: bool,
    switch: GraphSwitch,
}

fn graph_switch_row(
    t: &Theme,
    ent: Entity<RootView>,
    spec: GraphSwitchSpec,
    switches: &HashMap<&'static str, Entity<SwitchState>>,
    cx: &mut Context<RootView>,
) -> Div {
    let state = switches[spec.id].clone();
    state.update(cx, |state, cx| state.set_on(spec.on, cx));
    div()
        .debug_selector(|| spec.id.to_string())
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(tokens::FONT_13)
                .text_color(t.fg)
                .child(spec.label),
        )
        .child(
            Switch::new(state, t.palette()).on_change(move |value, _win, cx| {
                ent.update(cx, |view, cx| match spec.switch {
                    GraphSwitch::Sliding => view.set_sliding_graphs(value, cx),
                    GraphSwitch::NetworkDynamicScaling => {
                        view.set_network_dynamic_scaling(value, cx)
                    }
                });
            }),
        )
}

fn points_from_slider(value: f32) -> u32 {
    if !value.is_finite() {
        return DEFAULT_GRAPH_DATA_POINTS_CONFIG;
    }
    value
        .round()
        .clamp(
            f32::from(u16::try_from(MIN_GRAPH_DATA_POINTS).unwrap_or(10)),
            f32::from(u16::try_from(MAX_GRAPH_DATA_POINTS).unwrap_or(600)),
        )
        .to_string()
        .parse::<u32>()
        .unwrap_or(DEFAULT_GRAPH_DATA_POINTS_CONFIG)
}
