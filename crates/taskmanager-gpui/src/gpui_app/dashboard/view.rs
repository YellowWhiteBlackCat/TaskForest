//! Responsive Dashboard header, navigation summaries, and history cards.

use super::readouts::cpu_summary_readout;
use super::{DashboardPanel, DashboardState, SystemSection};
use gpui::{
    AnyElement, App, Div, Entity, InteractiveElement, IntoElement, ParentElement, ScrollHandle,
    Stateful, StatefulInteractiveElement, Styled, Window, div, px, relative,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_ui_contract::IconId;

use crate::core::SystemSnapshot;
use crate::gpui_app::elements;
use crate::gpui_app::formatting;
use crate::gpui_app::graph::{GraphHover, GraphOpts, graph_element_hover, graph_hover};
use crate::gpui_app::icons;
use crate::gpui_app::root::responsive::{SystemPageBudget, SystemSurfacePresentation};
use crate::gpui_app::root::{RootView, TopPage};
use crate::gpui_app::sidebar::SelectedDevice;
use crate::gpui_app::theme::tokens;
use crate::gpui_app::theme::{Color, Theme};
use crate::gpui_app::timeline::{
    HistoryWindow, TimelineMetric, TimelineSelection, TimelineSeries, TimelineStatistic,
};
use crate::i18n;
use taskmanager_telemetry_store::CorrelatedSystemTelemetryHistory;
use taskmanager_ui::layout::scroll_region_with_rail;
use taskmanager_ui::primitives::card_surface::CardSurface;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SummaryDestination {
    Cpu,
    Memory,
    Processes,
    Events,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SummaryNavigation {
    page: TopPage,
    device: Option<SelectedDevice>,
    panel: Option<DashboardPanel>,
}

impl SummaryDestination {
    fn id(self) -> &'static str {
        match self {
            Self::Cpu => "dashboard-summary-cpu",
            Self::Memory => "dashboard-summary-memory",
            Self::Processes => "dashboard-summary-processes",
            Self::Events => "dashboard-summary-events",
        }
    }

    fn navigation(self) -> SummaryNavigation {
        match self {
            Self::Cpu => SummaryNavigation {
                page: TopPage::Performance,
                device: Some(SelectedDevice::Cpu),
                panel: None,
            },
            Self::Memory => SummaryNavigation {
                page: TopPage::Performance,
                device: Some(SelectedDevice::Memory),
                panel: None,
            },
            Self::Processes => SummaryNavigation {
                page: TopPage::Apps,
                device: None,
                panel: None,
            },
            Self::Events => SummaryNavigation {
                page: TopPage::System,
                device: None,
                panel: Some(DashboardPanel::Events),
            },
        }
    }

    fn apply(self, view: &mut RootView) {
        let navigation = self.navigation();
        view.page = navigation.page;
        if let Some(panel) = navigation.panel {
            view.show_dashboard_panel(panel);
            view.dashboard.section = SystemSection::Dashboard;
        }
        if let Some(device) = navigation.device {
            view.select_device(device);
        }
    }
}

fn section_pill(
    theme: &Theme,
    section: SystemSection,
    active: SystemSection,
    entity: &Entity<RootView>,
) -> AnyElement {
    let (id, label, icon) = match section {
        SystemSection::Dashboard => (
            "system-dashboard-tab",
            i18n::t("dashboard.title"),
            IconId::System,
        ),
        SystemSection::Hardware => (
            "system-hardware-tab",
            i18n::t("dashboard.hardware"),
            IconId::Properties,
        ),
        SystemSection::Health => ("system-health-tab", i18n::t("health.title"), IconId::Health),
    };
    let entity = entity.clone();
    elements::Pill::new(
        id,
        label,
        move |_window, cx| {
            entity.update(cx, |view, cx| {
                view.dashboard.section = section;
                cx.notify();
            });
        },
        |_, _, _| {},
    )
    .active(section == active)
    .semantic_icon(icon)
    .render(theme)
    .into_any_element()
}

pub fn render_system_header(
    theme: &Theme,
    state: &DashboardState,
    layout: SystemPageBudget,
    entity: Entity<RootView>,
) -> Div {
    let open_panel = |panel: DashboardPanel, entity: Entity<RootView>| {
        move |_window: &mut Window, cx: &mut App| {
            entity.update(cx, |view, cx| {
                view.show_dashboard_panel(panel);
                cx.notify();
            });
        }
    };
    let unread = state.events.unread_count();
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(tokens::SPACE_8)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(tokens::SPACE_6)
                .child(section_pill(
                    theme,
                    SystemSection::Dashboard,
                    state.section,
                    &entity,
                ))
                .child(section_pill(
                    theme,
                    SystemSection::Hardware,
                    state.section,
                    &entity,
                ))
                .child(section_pill(
                    theme,
                    SystemSection::Health,
                    state.section,
                    &entity,
                )),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(match layout.surfaces {
                    SystemSurfacePresentation::SingleColumn => tokens::SPACE_4,
                    SystemSurfacePresentation::MultiColumn => tokens::SPACE_6,
                })
                .child(
                    elements::Pill::new(
                        "open-alert-rules",
                        i18n::t("alerts.manage"),
                        open_panel(DashboardPanel::AlertRules, entity.clone()),
                        |_, _, _| {},
                    )
                    .semantic_icon(IconId::Alert)
                    .render(theme),
                )
                .child(
                    elements::Pill::new(
                        "open-event-center",
                        format!("{} ({unread})", i18n::t("events.title")),
                        open_panel(DashboardPanel::Events, entity.clone()),
                        |_, _, _| {},
                    )
                    .semantic_icon(IconId::Alert)
                    .render(theme),
                )
                .child(
                    elements::Pill::new(
                        "open-saved-views",
                        i18n::t("saved_views.title"),
                        open_panel(DashboardPanel::SavedViews, entity),
                        |_, _, _| {},
                    )
                    .semantic_icon(IconId::Applications)
                    .render(theme),
                ),
        )
}

fn summary_card(
    theme: &Theme,
    label: &str,
    value: String,
    color: Color,
    icon: IconId,
    destination: SummaryDestination,
    entity: Entity<RootView>,
) -> Stateful<Div> {
    CardSurface::new(theme.palette())
        .background(theme.sidebar_card_bg)
        .padding(tokens::SPACE_10)
        .radius(tokens::card_radius(theme))
        .bordered(true)
        .child(
            div()
                .flex()
                .justify_between()
                .gap(tokens::SPACE_6)
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(tokens::SPACE_5)
                        .child(icons::icon(icon).size(px(14.0)).text_color(color))
                        .child(label.to_string()),
                )
                .child(i18n::t("dashboard.open")),
        )
        .child(
            div()
                .mt(tokens::SPACE_4)
                .text_size(tokens::FONT_20)
                .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                .text_color(color)
                .child(value),
        )
        .render()
        .id(destination.id())
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(theme))
        .cursor_pointer()
        .hover(|style| style.bg(theme.accent.with_alpha(0.08)))
        .flex_1()
        .min_w(px(132.0))
        .shadow(elements::card_shadow(theme))
        .on_click(move |_event, _window, cx| {
            entity.update(cx, |view, cx| {
                destination.apply(view);
                cx.notify();
            });
        })
}

fn readout_id(metric: TimelineMetric, statistic: TimelineStatistic) -> &'static str {
    match (metric, statistic) {
        (TimelineMetric::Cpu, TimelineStatistic::Latest) => "history-cpu-latest",
        (TimelineMetric::Cpu, TimelineStatistic::Peak) => "history-cpu-peak",
        (TimelineMetric::Memory, TimelineStatistic::Latest) => "history-memory-latest",
        (TimelineMetric::Memory, TimelineStatistic::Peak) => "history-memory-peak",
        (TimelineMetric::Disk, TimelineStatistic::Latest) => "history-disk-latest",
        (TimelineMetric::Disk, TimelineStatistic::Peak) => "history-disk-peak",
        (TimelineMetric::Network, TimelineStatistic::Latest) => "history-network-latest",
        (TimelineMetric::Network, TimelineStatistic::Peak) => "history-network-peak",
    }
}

fn format_readout(series: &TimelineSeries, selection: TimelineSelection, unit: &str) -> String {
    series.readout(selection).map_or_else(
        || i18n::t("dashboard.unavailable").to_string(),
        |readout| format!("{:.1} {unit}", readout.value),
    )
}

fn readout_pill(
    theme: &Theme,
    series: &TimelineSeries,
    metric: TimelineMetric,
    statistic: TimelineStatistic,
    unit: &str,
    active: TimelineSelection,
    entity: Entity<RootView>,
) -> AnyElement {
    let selection = TimelineSelection::new(metric, statistic);
    let title = match statistic {
        TimelineStatistic::Latest => i18n::t("dashboard.latest"),
        TimelineStatistic::Peak => i18n::t("dashboard.peak"),
    };
    let label = format!("{title} {}", format_readout(series, selection, unit));
    elements::pill(
        theme,
        readout_id(metric, statistic),
        &label,
        selection == active,
        false,
        move |_window, cx| {
            entity.update(cx, |view, cx| {
                view.dashboard.history_selection = selection;
                cx.notify();
            });
        },
        |_, _, _| {},
    )
    .into_any_element()
}

struct HistoryCardProps<'a> {
    theme: &'a Theme,
    label: &'a str,
    series: &'a TimelineSeries,
    metric: TimelineMetric,
    color: Color,
    max: f32,
    unit: &'a str,
    layout: SystemPageBudget,
    active: TimelineSelection,
    entity: Entity<RootView>,
    hover_slot: Rc<RefCell<Option<GraphHover>>>,
}

/// One history card: readout pills plus the hover graph. The sample buffer
/// flows through as the shared `Rc<[f32]>` handle from
/// `TimelineSeries::samples` (no `to_vec()` copy), so an unchanged frame
/// keeps the allocation identity `graph::scene_cache` keys its scene
/// replay on.
fn history_card(props: HistoryCardProps<'_>) -> Div {
    let HistoryCardProps {
        theme,
        label,
        series,
        metric,
        color,
        max,
        unit,
        layout,
        active,
        entity,
        hover_slot,
    } = props;
    let samples = series.samples(metric);
    div()
        .w(match layout.surfaces {
            SystemSurfacePresentation::SingleColumn => relative(1.0),
            SystemSurfacePresentation::MultiColumn => relative(0.49),
        })
        .min_w(px(match layout.surfaces {
            SystemSurfacePresentation::SingleColumn => 0.0,
            SystemSurfacePresentation::MultiColumn => 260.0,
        }))
        .h(px(match layout.surfaces {
            SystemSurfacePresentation::SingleColumn => 158.0,
            SystemSurfacePresentation::MultiColumn => 172.0,
        }))
        .flex()
        .flex_col()
        .gap(tokens::SPACE_5)
        .child(
            div()
                .font_weight(tokens::FONT_WEIGHT_SEMIBOLD.into())
                .text_size(tokens::FONT_12)
                .child(label.to_string()),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(tokens::SPACE_4)
                .child(readout_pill(
                    theme,
                    series,
                    metric,
                    TimelineStatistic::Latest,
                    unit,
                    active,
                    entity.clone(),
                ))
                .child(readout_pill(
                    theme,
                    series,
                    metric,
                    TimelineStatistic::Peak,
                    unit,
                    active,
                    entity,
                )),
        )
        .child(elements::graph_card_with_state(
            theme,
            graph_element_hover(
                history_graph_id(metric),
                history_graph_id(metric),
                std::rc::Rc::clone(&samples),
                color.into(),
                GraphOpts {
                    max: max.max(1.0),
                    gradient_fill: true,
                    ref_lines: true,
                    smooth: true,
                    ..GraphOpts::default()
                },
                metric_hover_format(unit),
                hover_slot,
            ),
            &samples,
        ))
}

/// Tooltip formatter for one dashboard history card, in the card's native unit
/// (percent graphs read `"{:.0}%"`, rate graphs `"{:.1} MiB/s"`).
fn metric_hover_format(unit: &str) -> impl Fn(f32) -> String + 'static {
    if unit == "%" {
        |value| format!("{value:.0}%")
    } else {
        |value| format!("{value:.1} MiB/s")
    }
}

/// Unique hover-graph id per history card (`&'static str` because gpui's
/// `ElementId` is built from static strings at these call sites).
fn history_graph_id(metric: TimelineMetric) -> &'static str {
    match metric {
        TimelineMetric::Cpu => "dashboard-history-cpu",
        TimelineMetric::Memory => "dashboard-history-memory",
        TimelineMetric::Disk => "dashboard-history-disk",
        TimelineMetric::Network => "dashboard-history-network",
    }
}

/// All straight-through dashboard render inputs (design-debt #1 props
/// consolidation).
pub struct DashboardViewProps<'a> {
    pub theme: &'a Theme,
    pub scroll: &'a ScrollHandle,
    pub snapshot: &'a SystemSnapshot,
    pub history: &'a CorrelatedSystemTelemetryHistory,
    pub process_count: usize,
    pub active_alert_count: usize,
    pub state: &'a DashboardState,
    pub layout: SystemPageBudget,
    pub entity: Entity<RootView>,
    pub hover_slot: Rc<RefCell<Option<GraphHover>>>,
}

pub fn render_dashboard(props: DashboardViewProps<'_>) -> impl IntoElement {
    let DashboardViewProps {
        theme,
        scroll,
        snapshot,
        history,
        process_count,
        active_alert_count,
        state,
        layout,
        entity,
        hover_slot,
    } = props;
    let series = state.timeline.series(history, state.history_window);
    let coverage_minutes = series.covered_ms as f64 / 60_000.0;
    let mut windows = div().flex().flex_row().flex_wrap().gap(tokens::SPACE_4);
    for window in HistoryWindow::ALL {
        let entity = entity.clone();
        windows = windows.child(elements::pill(
            theme,
            window.id(),
            &format!("{}m", window.minutes()),
            window == state.history_window,
            false,
            move |_window, cx| {
                entity.update(cx, |view, cx| {
                    view.dashboard.history_window = window;
                    cx.notify();
                });
            },
            |_, _, _| {},
        ));
    }
    let mut root = scroll_region_with_rail(
        "dashboard-scroll",
        "tm-dashboard-scroll",
        "dashboard-scrollbar",
        "tm-dashboard-scrollbar",
        scroll.clone(),
        theme.palette(),
        div()
            .pt(tokens::SPACE_8)
            .flex()
            .flex_col()
            .gap(tokens::SPACE_10)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(tokens::SPACE_8)
                    .child(summary_card(
                        theme,
                        i18n::t("common.cpu"),
                        cpu_summary_readout(&snapshot.cpu),
                        theme.cpu,
                        IconId::Cpu,
                        SummaryDestination::Cpu,
                        entity.clone(),
                    ))
                    .child(summary_card(
                        theme,
                        i18n::t("common.memory"),
                        snapshot
                            .memory
                            .used_percentage_observed()
                            .map_or_else(formatting::missing_value, |value| format!("{value:.1}%")),
                        theme.memory,
                        IconId::Memory,
                        SummaryDestination::Memory,
                        entity.clone(),
                    ))
                    .child(summary_card(
                        theme,
                        i18n::t("dashboard.processes"),
                        process_count.to_string(),
                        theme.disk,
                        IconId::Process,
                        SummaryDestination::Processes,
                        entity.clone(),
                    ))
                    .child(summary_card(
                        theme,
                        i18n::t("dashboard.active_alerts"),
                        active_alert_count.to_string(),
                        if active_alert_count == 0 {
                            theme.fg
                        } else {
                            theme.danger
                        },
                        IconId::Alert,
                        SummaryDestination::Events,
                        entity.clone(),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(tokens::SPACE_6)
                    .child(
                        div()
                            .font_weight(tokens::FONT_WEIGHT_SEMIBOLD.into())
                            .child(i18n::t("dashboard.history")),
                    )
                    .child(windows)
                    .child(
                        div()
                            .text_size(tokens::FONT_11)
                            .text_color(theme.fg_dim)
                            .child(
                                i18n::t("dashboard.coverage")
                                    .replace("{minutes}", &format!("{coverage_minutes:.1}")),
                            ),
                    ),
            )
            .child(render_history_grid(
                theme,
                &series,
                state,
                layout,
                entity,
                hover_slot.clone(),
            )),
    );
    // Hover tooltip: page-level singleton (one slot, one cursor). Sibling of
    // the history-card grid so the deferred+anchored label escapes
    // `overflow_hidden` (same pattern as cpu_view / perf_views).
    if let Some((pos, text)) = graph_hover(&hover_slot) {
        root = root.child(elements::tooltip_overlay(theme, &text, pos));
    }
    root
}

fn render_history_grid(
    theme: &Theme,
    series: &TimelineSeries,
    state: &DashboardState,
    layout: SystemPageBudget,
    entity: Entity<RootView>,
    hover_slot: Rc<RefCell<Option<GraphHover>>>,
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
        })
    };
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(tokens::SPACE_8)
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

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_dashboard_view_tests.rs"]
mod tests;
