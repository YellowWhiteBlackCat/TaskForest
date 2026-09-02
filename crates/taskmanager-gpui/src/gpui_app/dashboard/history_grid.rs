//! Dashboard history-card grid composition.

use super::{DashboardState, HistoryCardProps, history_card};
use crate::gpui_app::graph::{GraphCacheHandle, GraphHover};
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::responsive::SystemPageBudget;
use crate::gpui_app::timeline::{TimelineMetric, TimelineSeries};
use gpui::{Div, Entity, ParentElement, Styled, div};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_application::i18n;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

pub(super) fn render_history_grid(
    theme: &Theme,
    series: &TimelineSeries,
    state: &DashboardState,
    layout: SystemPageBudget,
    entity: Entity<RootView>,
    hover_slot: Rc<RefCell<Option<GraphHover>>>,
    graph_cache: GraphCacheHandle,
) -> Div {
    let disk_max = finite_peak(&series.disk_mib_per_sec);
    let network_max = finite_peak(&series.network_mib_per_sec);
    let card = |metric, label, color, max, unit, entity| {
        history_card(HistoryCardProps {
            theme,
            label,
            series,
            metric,
            color,
            max,
            unit,
            layout,
            active: state.history_selection,
            entity,
            hover_slot: hover_slot.clone(),
            graph_cache: graph_cache.clone(),
        })
    };
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(card(
            TimelineMetric::Cpu,
            i18n::t("common.cpu"),
            theme.cpu,
            100.0,
            "%",
            entity.clone(),
        ))
        .child(card(
            TimelineMetric::Memory,
            i18n::t("common.memory"),
            theme.memory,
            100.0,
            "%",
            entity.clone(),
        ))
        .child(card(
            TimelineMetric::Disk,
            i18n::t("dashboard.disk_io"),
            theme.disk,
            disk_max,
            "MiB/s",
            entity.clone(),
        ))
        .child(card(
            TimelineMetric::Network,
            i18n::t("dashboard.network_io"),
            theme.network,
            network_max,
            "MiB/s",
            entity,
        ))
}

fn finite_peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(0.0_f32, f32::max)
}
