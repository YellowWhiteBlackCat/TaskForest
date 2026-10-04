//! Dashboard history-card grid composition.

use super::{DashboardState, HistoryCardProps, history_card};
use crate::gpui_app::graph::{GraphCacheHandle, GraphHover};
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::responsive::DashboardBudget;
use crate::gpui_app::timeline::GraphTimelineSeries;
use gpui::{Div, Entity, ParentElement, Styled, div};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_application::i18n;
use taskmanager_application::system_timeline::TimelineMetric;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::theme_binding::definite_length;

pub(super) fn render_history_grid(
    theme: &Theme,
    series: &GraphTimelineSeries,
    state: &DashboardState,
    layout: DashboardBudget,
    entity: Entity<RootView>,
    hover_slot: Rc<RefCell<Option<GraphHover>>>,
    graph_cache: GraphCacheHandle,
) -> Div {
    let disk_max = finite_peak(&series.samples(TimelineMetric::Disk));
    let network_max = finite_peak(&series.samples(TimelineMetric::Network));
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
    let mut grid = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(definite_length(tokens::SPACE_8));
    for metric in TimelineMetric::ALL
        .into_iter()
        .skip(layout.first_metric(state.history_first_metric))
        .take(layout.metric_count)
    {
        let (label, color, max, unit) = match metric {
            TimelineMetric::Cpu => (i18n::t("common.cpu"), theme.cpu, 100.0, "%"),
            TimelineMetric::Memory => (i18n::t("common.memory"), theme.memory, 100.0, "%"),
            TimelineMetric::Disk => (i18n::t("dashboard.disk_io"), theme.disk, disk_max, "MiB/s"),
            TimelineMetric::Network => (
                i18n::t("dashboard.network_io"),
                theme.network,
                network_max,
                "MiB/s",
            ),
        };
        grid = grid.child(card(metric, label, color, max, unit, entity.clone()));
    }
    grid
}

fn finite_peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(0.0_f32, f32::max)
}
