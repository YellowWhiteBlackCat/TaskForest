//! Fixed CPU headline readouts, gauges and graph summary for the Iced Performance view.

use iced::widget::{column, progress_bar, text};
use iced::{Element, Length};
use taskmanager_application::CpuTemperatureSource;
use taskmanager_application::i18n::t;
use taskmanager_shell::history::MetricSeries;
use taskmanager_shell::presentation::graph_summary;
use taskmanager_shell::presentation::missing_value;
use taskmanager_theme::tokens;

use super::projection::{CpuHeadlineKind, CpuHeadlineMetric, CpuHeadlineValue};
use crate::app::Message;
use crate::perf_chart::CHART_HEIGHT;

/// Render every current CPU fact at once in the shared product order. These
/// replace the retired graph-selector pills without multiplying the number of
/// charts or changing the fixed headline-graph hierarchy.
pub(super) fn cpu_headline_readouts(
    metrics: &[CpuHeadlineMetric],
    bogomips: bool,
    temperature_source: CpuTemperatureSource,
    theme_snapshot: &taskmanager_theme::Theme,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    crate::ui::perf_layout::headline_readouts(
        theme_snapshot,
        metrics
            .iter()
            .map(|metric| cpu_headline_label_value(*metric, bogomips, temperature_source)),
    )
}

pub(super) fn cpu_headline_label_value(
    metric: CpuHeadlineMetric,
    bogomips: bool,
    temperature_source: CpuTemperatureSource,
) -> (String, String) {
    let missing = || missing_value();
    match metric.kind {
        CpuHeadlineKind::Utilization => (
            t("common.utilization").to_owned(),
            match metric.value {
                Some(CpuHeadlineValue::UsagePercent(value)) => format!("{value:.0}%"),
                _ => missing(),
            },
        ),
        CpuHeadlineKind::Frequency => {
            let frequency = match metric.value {
                Some(CpuHeadlineValue::FrequencyMhz(value)) => Some(value),
                _ => None,
            };
            super::cpu_speed_row(frequency, bogomips)
        }
        CpuHeadlineKind::Temperature => {
            let temperature = match metric.value {
                Some(CpuHeadlineValue::TemperatureC(value)) => Some(value),
                _ => None,
            };
            super::cpu_temperature_row(temperature, temperature_source)
        }
        CpuHeadlineKind::Power => (
            t("common.power").to_owned(),
            match metric.value {
                Some(CpuHeadlineValue::PowerW(value)) => format!("{value:.1} W"),
                _ => missing(),
            },
        ),
    }
}

pub(super) fn chart_height(compact: bool) -> f32 {
    if compact { 80.0 } else { CHART_HEIGHT }
}

pub(super) fn gauge(
    title: &'static str,
    value: Option<f32>,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let progress: Element<'static, Message, iced::Theme, iced::Renderer> = match value {
        Some(value) => progress_bar(0.0..=100.0, value).into(),
        None => text(t("dashboard.unavailable")).into(),
    };
    column![text(percent_text(title, value)), progress,]
        .spacing(4)
        .width(Length::FillPortion(1))
        .into()
}

pub(crate) fn percent_text(title: &str, value: Option<f32>) -> String {
    value.map_or_else(
        || format!("{title} —"),
        |value| format!("{title} {value:>5.1}%"),
    )
}

/// Summary lines for the retained single utilization graph used by both the
/// CPU and Memory overview pages.
pub(super) fn utilization_graph_summary_elements(
    app: &crate::IcedApp,
) -> Vec<Element<'static, Message, iced::Theme, iced::Renderer>> {
    let mut lines = Vec::new();
    let cpu_series = app.cached_metric_series(MetricSeries::CpuUsagePercent);
    let memory_series = app.cached_metric_series(MetricSeries::MemoryUsagePercent);
    push_graph_summary(&mut lines, t("common.cpu"), &cpu_series, |value| {
        format!("{value:.0}%")
    });
    push_graph_summary(&mut lines, t("common.memory"), &memory_series, |value| {
        format!("{value:.0}%")
    });
    lines
}

pub(crate) fn push_graph_summary(
    lines: &mut Vec<Element<'static, Message, iced::Theme, iced::Renderer>>,
    label: &str,
    samples: &[f32],
    format_value: impl Fn(f32) -> String,
) {
    let Some(summary) = graph_summary(samples) else {
        return;
    };
    lines.push(
        text(format!(
            "{label} · {} {} · {} {} · {} {}",
            t("common.latest"),
            format_value(summary.latest),
            t("common.avg"),
            format_value(summary.average),
            t("common.peak"),
            format_value(summary.maximum),
        ))
        .size(f32::from(tokens::FONT_12))
        .into(),
    );
}
