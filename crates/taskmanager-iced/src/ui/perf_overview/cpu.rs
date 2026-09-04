//! Fixed CPU headline readouts, gauges and graph summary for the Iced Performance view.

use iced::widget::{column, progress_bar, row, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_application::{
    MsrReadoutRequestFailure, MsrReadoutState, RaplPowerRequestFailure, RaplPowerState,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::CpuTemperatureSource;
use taskmanager_platform_contract::{CapabilityId, CapabilityStatus};
use taskmanager_shell::presentation::graph_summary;
use taskmanager_shell::presentation::missing_value;
use taskmanager_shell::presentation::trend::TrendSeries;
use taskmanager_shell::viewmodel::StatRow;
use taskmanager_theme::tokens;

use super::projection::{CpuHeadlineKind, CpuHeadlineMetric, CpuHeadlineValue};
use crate::app::{FocusTarget, Message};
use crate::focus;
use crate::theme;

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
            let (label, value) = cpu_speed_parts(frequency, bogomips);
            (label.to_owned(), value.unwrap_or_else(missing))
        }
        CpuHeadlineKind::Temperature => {
            let temperature = match metric.value {
                Some(CpuHeadlineValue::TemperatureC(value)) => Some(value),
                _ => None,
            };
            let (label, value) = cpu_temperature_parts(temperature, temperature_source);
            (label.to_owned(), value.unwrap_or_else(missing))
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

pub(super) const HEADLINE_CHART_FLOOR: f32 = 240.0;
pub(super) const HEADLINE_CHART_PRESENCE: f32 = 300.0;

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
        |value| format!("{title} {value:.0} %"),
    )
}

pub(super) fn utilization_graph_summary_elements(
    app: &crate::IcedApp,
) -> Vec<Element<'static, Message, iced::Theme, iced::Renderer>> {
    let mut lines = Vec::new();
    let cpu_series = app.cached_metric_series(TrendSeries::CpuUsagePercent);
    let memory_series = app.cached_metric_series(TrendSeries::MemoryUsagePercent);
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

pub(crate) fn cpu_speed_parts(
    frequency_mhz: Option<u64>,
    bogomips: bool,
) -> (&'static str, Option<String>) {
    let label = if bogomips {
        t("cpu.bogomips")
    } else {
        t("common.speed")
    };
    let value = match frequency_mhz {
        Some(value) if bogomips => Some(format!("{value}.00 BogoMIPS")),
        Some(value) => Some(format!("{value} MHz")),
        None => None,
    };
    (label, value)
}

pub(crate) fn cpu_speed_row(frequency_mhz: Option<u64>, bogomips: bool) -> StatRow {
    let (label, value) = cpu_speed_parts(frequency_mhz, bogomips);
    StatRow::text(label, value)
}

pub(crate) fn cpu_frequency_readout_for_source(
    frequency_mhz: Option<u64>,
    bogomips: bool,
) -> String {
    cpu_speed_parts(frequency_mhz, bogomips)
        .1
        .unwrap_or_else(missing_value)
}

pub(crate) fn cpu_temperature_parts(
    temperature_c: Option<f32>,
    source: CpuTemperatureSource,
) -> (&'static str, Option<String>) {
    let note = match source {
        CpuTemperatureSource::PackageHwmon => Some(t("cpu.temperature_source.package_hwmon")),
        CpuTemperatureSource::ThermalZone => Some(t("cpu.temperature_source.thermal_zone")),
        _ => None,
    };
    let value = temperature_c.map(|value| format!("{value:.0} °C"));
    let value = match (note, value) {
        (Some(note), Some(value)) => Some(format!("{value} · {note}")),
        (_, value) => value,
    };
    (t("common.temperature"), value)
}

pub(crate) fn cpu_temperature_row(
    temperature_c: Option<f32>,
    source: CpuTemperatureSource,
) -> StatRow {
    let (label, value) = cpu_temperature_parts(temperature_c, source);
    StatRow::text(label, value)
}

#[must_use]
pub(crate) fn rapl_power_needs_authorization(
    state: &RaplPowerState,
    capability: Option<CapabilityStatus>,
) -> bool {
    match state {
        RaplPowerState::Failed(failed) => {
            let kind = match &failed.failure {
                RaplPowerRequestFailure::Submission(kind) => *kind,
                RaplPowerRequestFailure::Provider(failed) => failed.kind,
            };
            kind == FailureKind::RequiresEscalation
        }
        RaplPowerState::Closed => matches!(
            capability,
            Some(CapabilityStatus::Available | CapabilityStatus::PermissionRequired)
        ),
        _ => false,
    }
}

#[must_use]
pub(crate) fn msr_readout_needs_authorization(
    state: &MsrReadoutState,
    capability: Option<CapabilityStatus>,
) -> bool {
    match state {
        MsrReadoutState::Failed(failed) => {
            let kind = match &failed.failure {
                MsrReadoutRequestFailure::Submission(kind) => *kind,
                MsrReadoutRequestFailure::Provider(failed) => failed.kind,
            };
            kind == FailureKind::RequiresEscalation
        }
        MsrReadoutState::Closed => matches!(
            capability,
            Some(CapabilityStatus::Available | CapabilityStatus::PermissionRequired)
        ),
        _ => false,
    }
}

pub(crate) fn rapl_power_card<'a>(
    app: &'a crate::IcedApp,
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let state = app.shell.rapl_power_state();
    let capability = app
        .shell
        .projection()
        .capability_status(&CapabilityId::TELEMETRY_CPU_PACKAGE_POWER);

    if rapl_power_needs_authorization(state, capability) {
        let content = column![
            text(t("cpu.package_power"))
                .size(f32::from(tokens::FONT_13))
                .color(theme::muted_text_color(theme_snapshot)),
            row![
                text(t("cpu.package_power_requires_auth"))
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fill),
                focus::button(
                    theme_snapshot,
                    FocusTarget::AuthorizeRaplPower,
                    t("settings.privileges_authorize"),
                    Message::AuthorizeRaplPower,
                    false,
                ),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(6);

        return Some(
            iced::widget::container(content)
                .padding(8)
                .width(Length::Fill)
                .style(move |_| theme::card_style(theme_snapshot))
                .into(),
        );
    }

    let ready_snapshot = match state {
        RaplPowerState::Ready(ready) => Some(&ready.snapshot),
        RaplPowerState::Loading {
            last_good: Some(ready),
            ..
        } => Some(&ready.snapshot),
        _ => None,
    };

    if let Some(snapshot) = ready_snapshot {
        if snapshot.packages.is_empty() {
            return None;
        }
        let rows: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = snapshot
            .packages
            .iter()
            .map(|pkg| {
                row![
                    text(pkg.name.clone())
                        .size(f32::from(tokens::FONT_12))
                        .color(theme::muted_text_color(theme_snapshot))
                        .width(Length::Fill),
                    text(format!("{:.1} W", pkg.power_w)).size(f32::from(tokens::FONT_13)),
                ]
                .spacing(8)
                .width(Length::Fill)
                .into()
            })
            .collect();

        let content = column![
            text(t("cpu.package_power"))
                .size(f32::from(tokens::FONT_13))
                .color(theme::muted_text_color(theme_snapshot)),
            column(rows).spacing(4),
        ]
        .spacing(6);

        return Some(
            iced::widget::container(content)
                .padding(8)
                .width(Length::Fill)
                .style(move |_| theme::card_style(theme_snapshot))
                .into(),
        );
    }

    None
}

pub(crate) fn msr_readouts_card<'a>(
    app: &'a crate::IcedApp,
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let state = app.shell.msr_readout_state();
    let capability = app
        .shell
        .projection()
        .capability_status(&CapabilityId::TELEMETRY_CPU_MSR);

    if msr_readout_needs_authorization(state, capability) {
        let content = column![
            text(t("cpu.msr_readouts"))
                .size(f32::from(tokens::FONT_13))
                .color(theme::muted_text_color(theme_snapshot)),
            row![
                text(t("cpu.msr_readouts_requires_auth"))
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fill),
                focus::button(
                    theme_snapshot,
                    FocusTarget::AuthorizeMsrReadouts,
                    t("settings.privileges_authorize"),
                    Message::AuthorizeMsrReadouts,
                    false,
                ),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(6);

        return Some(
            iced::widget::container(content)
                .padding(8)
                .width(Length::Fill)
                .style(move |_| theme::card_style(theme_snapshot))
                .into(),
        );
    }

    let ready_snapshot = match state {
        MsrReadoutState::Ready(ready) => Some(&ready.snapshot),
        MsrReadoutState::Loading {
            last_good: Some(ready),
            ..
        } => Some(&ready.snapshot),
        _ => None,
    };

    if let Some(snapshot) = ready_snapshot {
        if snapshot.packages.is_empty() {
            return None;
        }
        let mut rows: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();
        for readout in &snapshot.packages {
            let node = format!("CPU {}", readout.cpu);
            if let Some(temp) = readout.temperature_c {
                rows.push(
                    row![
                        text(format!("{node} · {}", t("cpu.msr_temperature")))
                            .size(f32::from(tokens::FONT_12))
                            .color(theme::muted_text_color(theme_snapshot))
                            .width(Length::Fill),
                        text(format!("{temp:.1} °C")).size(f32::from(tokens::FONT_13)),
                    ]
                    .spacing(8)
                    .width(Length::Fill)
                    .into(),
                );
            }
            if let Some(mult) = readout.multiplier {
                rows.push(
                    row![
                        text(format!("{node} · {}", t("cpu.msr_multiplier")))
                            .size(f32::from(tokens::FONT_12))
                            .color(theme::muted_text_color(theme_snapshot))
                            .width(Length::Fill),
                        text(format!("\u{00d7}{mult:.1}")).size(f32::from(tokens::FONT_13)),
                    ]
                    .spacing(8)
                    .width(Length::Fill)
                    .into(),
                );
            }
            if let Some(vcore) = readout.vcore_v {
                rows.push(
                    row![
                        text(format!("{node} · {}", t("cpu.msr_vcore")))
                            .size(f32::from(tokens::FONT_12))
                            .color(theme::muted_text_color(theme_snapshot))
                            .width(Length::Fill),
                        text(format!("{vcore:.3} V")).size(f32::from(tokens::FONT_13)),
                    ]
                    .spacing(8)
                    .width(Length::Fill)
                    .into(),
                );
            }
        }
        if rows.is_empty() {
            return None;
        }

        let content = column![
            text(t("cpu.msr_readouts"))
                .size(f32::from(tokens::FONT_13))
                .color(theme::muted_text_color(theme_snapshot)),
            column(rows).spacing(4),
        ]
        .spacing(6);

        return Some(
            iced::widget::container(content)
                .padding(8)
                .width(Length::Fill)
                .style(move |_| theme::card_style(theme_snapshot))
                .into(),
        );
    }

    None
}

pub(crate) fn append_rapl_and_msr_stats(
    shell: &taskmanager_shell::ShellApp,
    stats: &mut Vec<StatRow>,
) {
    match shell.rapl_power_state() {
        RaplPowerState::Ready(ready) => {
            for pkg in &ready.snapshot.packages {
                let label = if pkg.name.is_empty()
                    || (ready.snapshot.packages.len() == 1
                        && (pkg.name.eq_ignore_ascii_case("package-0")
                            || pkg.name.eq_ignore_ascii_case("package 0")))
                {
                    t("cpu.package_power").to_string()
                } else {
                    format!("{} ({})", t("cpu.package_power"), pkg.name)
                };
                stats.push(StatRow::text(label, Some(format!("{:.1} W", pkg.power_w))));
            }
        }
        RaplPowerState::Loading {
            last_good: Some(ready),
            ..
        } => {
            for pkg in &ready.snapshot.packages {
                let label = if pkg.name.is_empty()
                    || (ready.snapshot.packages.len() == 1
                        && (pkg.name.eq_ignore_ascii_case("package-0")
                            || pkg.name.eq_ignore_ascii_case("package 0")))
                {
                    t("cpu.package_power").to_string()
                } else {
                    format!("{} ({})", t("cpu.package_power"), pkg.name)
                };
                stats.push(StatRow::text(label, Some(format!("{:.1} W", pkg.power_w))));
            }
        }
        _ => {}
    }
    match shell.msr_readout_state() {
        MsrReadoutState::Ready(ready) => {
            for readout in &ready.snapshot.packages {
                let node = format!("CPU {}", readout.cpu);
                if let Some(temp) = readout.temperature_c {
                    stats.push(StatRow::text(
                        format!("{node} · {}", t("cpu.msr_temperature")),
                        Some(format!("{temp:.1} °C")),
                    ));
                }
                if let Some(mult) = readout.multiplier {
                    stats.push(StatRow::text(
                        format!("{node} · {}", t("cpu.msr_multiplier")),
                        Some(format!("\u{00d7}{mult:.1}")),
                    ));
                }
                if let Some(vcore) = readout.vcore_v {
                    stats.push(StatRow::text(
                        format!("{node} · {}", t("cpu.msr_vcore")),
                        Some(format!("{vcore:.3} V")),
                    ));
                }
            }
        }
        MsrReadoutState::Loading {
            last_good: Some(ready),
            ..
        } => {
            for readout in &ready.snapshot.packages {
                let node = format!("CPU {}", readout.cpu);
                if let Some(temp) = readout.temperature_c {
                    stats.push(StatRow::text(
                        format!("{node} · {}", t("cpu.msr_temperature")),
                        Some(format!("{temp:.1} °C")),
                    ));
                }
                if let Some(mult) = readout.multiplier {
                    stats.push(StatRow::text(
                        format!("{node} · {}", t("cpu.msr_multiplier")),
                        Some(format!("\u{00d7}{mult:.1}")),
                    ));
                }
                if let Some(vcore) = readout.vcore_v {
                    stats.push(StatRow::text(
                        format!("{node} · {}", t("cpu.msr_vcore")),
                        Some(format!("{vcore:.3} V")),
                    ));
                }
            }
        }
        _ => {}
    }
}
