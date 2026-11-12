//! Dashboard history-card grid composition.

use super::{DashboardState, HistoryCardProps, history_card};
use crate::gpui_app::graph::{GraphCacheHandle, GraphHover};
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::responsive::DashboardBudget;
use crate::gpui_app::timeline::GraphTimelineSeries;
use gpui::{Div, Entity, Styled, px};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_application::i18n;
use taskmanager_application::system_timeline::TimelineMetric;
use taskmanager_shell::presentation::finite_peak;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::layout::AdaptiveGrid;

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
    // The shared adaptive grid owns the row geometry: the budget's derived
    // card width is the item basis, so the row fills the measured page slot
    // exactly instead of leaving a dead band at its right edge.
    let grid = AdaptiveGrid::new(px(layout.card_basis()))
        .gap(tokens::SPACE_8)
        .debug_selector("tm-dashboard-metric-grid");
    let mut grid = grid;
    for metric in layout.metric_window(state.history_first_metric).metrics() {
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
    grid.render().h_full()
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_app/dashboard/metric_grid_bounds_tests.rs"]
mod metric_grid_bounds_tests;
