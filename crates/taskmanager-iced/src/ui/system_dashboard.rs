//! Iced System-page Dashboard segment (GPUI System dashboard parity).
//!
//! An independent segment module: `render_system_dashboard` composes the
//! summary cards, the history-window selector, and the events center from the
//! existing shell projection and is self-sufficient headless (the System page
//! wiring lives in `ui::system_table`, owned by a parallel workflow). The
//! window vocabulary aligns to GPUI's resource history window options
//! (1m / 5m / 15m / 60m). Honesty contract: an unobserved fact renders the shared dash,
//! never a fabricated zero, and the events center lists the shell's live
//! active-alert mirror — no persisted event history exists in the shell, so
//! none is invented.

use super::components::dashboard_budget::DashboardBudget;
use super::device_chart::{DeviceChart, DeviceMetricScale, series_max};
use iced::Length;
use iced::widget::{canvas, column, container, responsive, row, text};
use taskmanager_application::i18n::t;
use taskmanager_application::system_timeline::{
    SystemHistoryWindow, SystemPageSection, TimelineMetric, TimelineSeries, TimelineStatistic,
};
use taskmanager_shell::presentation::health_review::HealthReviewSection;
use taskmanager_shell::presentation::system_timeline::{coverage, readout};

use crate::app::{FocusTarget, Message};
use crate::focus;
use crate::theme;
use taskmanager_theme::tokens;

use super::components::{IcedElement, titled_card};
use taskmanager_shell::presentation::missing_value;

// The summary fold lives in the data layer (`super::system_dashboard_model`)
// per ARCH.md §8.1; re-exported here so the segment module and its mounted
// tests read one import surface.
pub(crate) use super::system_dashboard_model::{DashboardSummaryModel, summary_model};
use taskmanager_theme::Theme;

/// The System-page dashboard segment's typed message vocabulary, carried by
/// [`crate::app::Message::SystemDashboard`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemDashboardMessage {
    /// Select the history window the segment summarizes.
    SelectWindow(SystemHistoryWindow),
    SelectSection(SystemPageSection),
    SelectHealthSection(HealthReviewSection),
    Previous,
    Next,
}

/// The label for one history window (1m, 5m, 15m, 60m), matching GPUI copy.
pub(crate) fn history_window_label(window: SystemHistoryWindow) -> &'static str {
    window.label()
}

/// Render the System-page dashboard segment from the current app projection.
/// `selected_window` is the caller-owned window state (the wiring in
/// `ui::system_table` owns where it lives); the pills publish
/// [`SystemDashboardMessage::SelectWindow`] for that lane.
pub(crate) fn render_system_dashboard(
    app: &crate::IcedApp,
    selected_window: SystemHistoryWindow,
) -> IcedElement<'_> {
    let series = app.shell.system_timeline_series(selected_window);
    responsive(move |size| {
        let theme_snapshot = app.theme();
        let budget = DashboardBudget::resolve(size.width, size.height);
        let model = summary_model(app.shell.projection(), &series);
        let summary = if budget.compact_summary {
            text(format!(
                "{} {} · {} {}",
                t("dashboard.processes"),
                model
                    .processes
                    .map_or_else(missing_value, |count| count.to_string()),
                t("dashboard.active_alerts"),
                model.active_alerts
            ))
            .size(f32::from(tokens::FONT_12))
            .into()
        } else {
            summary_card(theme_snapshot, &model)
        };
        let previous = focus::dynamic_button(
            theme_snapshot,
            FocusTarget::SystemDashboardPrevious,
            t("dashboard.previous_metrics").to_owned(),
            Message::SystemDashboard(SystemDashboardMessage::Previous),
            false,
        );
        let next = focus::dynamic_button(
            theme_snapshot,
            FocusTarget::SystemDashboardNext,
            t("dashboard.next_metrics").to_owned(),
            Message::SystemDashboard(SystemDashboardMessage::Next),
            false,
        );
        let mut body = column![summary, row![previous, next].spacing(8)]
            .spacing(8)
            .height(Length::Fill);
        if budget.count == 0 {
            return body.push(text(t("dashboard.resize"))).into();
        }
        let metrics = TimelineMetric::ALL
            .into_iter()
            .skip(app.system_dashboard_first_metric)
            .take(budget.count)
            .collect::<Vec<_>>();
        for pair in metrics.chunks(budget.columns) {
            body = body.push(
                row(pair
                    .iter()
                    .map(|metric| history_card(app, &series, *metric, budget.chart_height))
                    .collect::<Vec<_>>())
                .spacing(8)
                .width(Length::Fill),
            );
        }
        body.into()
    })
    .into()
}

fn history_card<'a>(
    app: &'a crate::IcedApp,
    series: &TimelineSeries,
    metric: TimelineMetric,
    chart_height: f32,
) -> IcedElement<'a> {
    let theme_snapshot = app.theme();
    let samples = app.system_timeline_graph(series, metric);
    let scale = match metric {
        TimelineMetric::Cpu | TimelineMetric::Memory => DeviceMetricScale::Percent,
        _ => DeviceMetricScale::AutoPeak,
    };
    let color = match metric {
        TimelineMetric::Cpu => theme_snapshot.cpu,
        TimelineMetric::Memory => theme_snapshot.memory,
        TimelineMetric::Disk => theme_snapshot.disk,
        TimelineMetric::Network => theme_snapshot.network,
    };
    let graph = canvas::Canvas::new(DeviceChart {
        max: series_max(scale, &samples),
        samples,
        color: crate::theme_binding::color(color),
        grid_color: crate::theme_binding::color(theme_snapshot.palette().border),
        smooth: false,
        hover: true,
        scale,
        readout: crate::perf_chart::ReadoutColors {
            bg: crate::theme_binding::color(theme_snapshot.palette().surface),
            fg: crate::theme_binding::color(theme_snapshot.palette().fg),
        },
    })
    .width(Length::Fill)
    .height(Length::Fixed(chart_height));
    let stats = format!(
        "{} {} · {} {}",
        t("dashboard.latest"),
        readout(series, metric, TimelineStatistic::Latest),
        t("dashboard.peak"),
        readout(series, metric, TimelineStatistic::Peak)
    );
    container(titled_card(
        theme_snapshot,
        t(metric.label_key()),
        column![
            text(stats).size(f32::from(tokens::FONT_12)),
            graph,
            text(coverage(series, metric)).size(f32::from(tokens::FONT_11))
        ]
        .spacing(4),
    ))
    .width(Length::Fill)
    .id(format!("system-dashboard-card-{}", metric.id()))
    .into()
}

/// The four summary value columns (CPU / memory / processes / active alerts),
/// GPUI `summary_card` parity in one titled card.
fn summary_card<'a>(theme_snapshot: &'a Theme, model: &DashboardSummaryModel) -> IcedElement<'a> {
    let muted = theme::muted_text_color(theme_snapshot);
    let alert_color = if model.active_alerts == 0 {
        muted
    } else {
        crate::theme_binding::color(theme_snapshot.palette().danger)
    };
    let value = |label: &'static str, display: String, color: iced::Color| {
        column![
            text(label)
                .size(f32::from(tokens::FONT_11))
                .color(theme::muted_text_color(theme_snapshot)),
            text(display)
                .size(f32::from(tokens::FONT_20))
                .color(color)
                .width(Length::Fill),
        ]
        .spacing(f32::from(tokens::SPACE_4))
        .width(Length::Fill)
    };
    let cpu = crate::theme_binding::color(theme_snapshot.cpu);
    let memory = crate::theme_binding::color(theme_snapshot.memory);
    let disk = crate::theme_binding::color(theme_snapshot.disk);
    let values = row![
        value(t("common.cpu"), model.cpu.clone(), cpu),
        value(t("common.memory"), model.memory.clone(), memory),
        value(
            t("dashboard.processes"),
            model
                .processes
                .map_or_else(|| missing_value().to_owned(), |count| count.to_string()),
            disk,
        ),
        value(
            t("dashboard.active_alerts"),
            model.active_alerts.to_string(),
            alert_color,
        ),
    ]
    .spacing(f32::from(tokens::SPACE_12))
    .width(Length::Fill);
    titled_card(theme_snapshot, t("dashboard.title"), values)
}

/// The history-window selector: one choice pill per shared window, the
/// selected window wearing the active pill.
pub(super) fn window_controls<'a>(
    theme_snapshot: &'a Theme,
    selected_window: SystemHistoryWindow,
) -> IcedElement<'a> {
    let mut pills = row![].spacing(f32::from(tokens::SPACE_4));
    for window in SystemHistoryWindow::ALL {
        pills = pills.push(focus::choice_pill(
            theme_snapshot,
            FocusTarget::SystemHistoryWindow(window),
            history_window_label(window).to_owned(),
            window == selected_window,
            Message::SystemDashboard(SystemDashboardMessage::SelectWindow(window)),
        ));
    }
    pills.width(Length::Fill).into()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/system_dashboard_tests.rs"]
mod tests;
