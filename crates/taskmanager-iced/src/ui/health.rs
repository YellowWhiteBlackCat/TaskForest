//! Iced system-health modal: per-domain device summary from the shared
//! snapshot + hardware facts, and the alert-rule view consuming the shell's
//! rolling history (`suggest_threshold` — the same source as the TUI's
//! alerts overlay, presented with iced widgets).

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_core::core::alerts::AlertMetric;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_theme::tokens;

use crate::app::Message;
use crate::i18n::{self, Key};
use crate::theme;

use super::overlays::{metric_label, modal_overlay, suggestion_text};
use taskmanager_shell::presentation::{bytes, missing_value};

mod projection;

/// Render the health modal: the device summary (or an honest empty state
/// while telemetry has not arrived) plus the alert-rule list.
pub(super) fn render(app: &crate::IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let appear = app.modal_appear_progress();
    let theme_snapshot = app.theme();
    let language = app.language();
    let shell = &app.shell;

    let summary_panel: Element<'_, Message, iced::Theme, iced::Renderer> =
        match shell.projection().snapshot.as_ref() {
            Some(snapshot) => {
                let rows = health_rows(snapshot);
                panel(
                    theme_snapshot,
                    i18n::t(language, Key::DeviceSummary),
                    column(
                        rows.into_iter()
                            .map(|row| health_row(theme_snapshot, row))
                            .collect::<Vec<_>>(),
                    )
                    .spacing(1)
                    .into(),
                )
            }
            None => panel(
                theme_snapshot,
                i18n::t(language, Key::DeviceSummary),
                text(i18n::t(language, Key::HealthWaiting))
                    .size(f32::from(tokens::FONT_14))
                    .into(),
            ),
        };

    let alert_panel = panel(
        theme_snapshot,
        i18n::t(language, Key::AlertRules),
        column(
            AlertMetric::ALL
                .into_iter()
                .map(|metric| alert_row(theme_snapshot, metric, shell))
                .collect::<Vec<_>>(),
        )
        .spacing(1)
        .into(),
    );

    let mut modal_panels: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> =
        vec![summary_panel];
    if let Some(snapshot) = shell.projection().snapshot.as_ref()
        && let Some(sensor_panel) = sensors_and_thermal_panel(snapshot, theme_snapshot)
    {
        modal_panels.push(sensor_panel);
    }
    modal_panels.push(alert_panel);

    modal_overlay(
        theme_snapshot,
        i18n::t(language, Key::Health),
        "Observed samples only · Esc closes",
        scrollable(column(modal_panels).spacing(12))
            .height(Length::Fixed(430.0))
            .width(Length::Fill)
            .into(),
        appear,
    )
}

/// One domain summary row: label, value text, and a typed health bucket
/// derived from the availability of the underlying facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HealthRow {
    pub label: String,
    pub value: String,
    pub healthy: bool,
}

/// Build the per-domain device summary from one snapshot. A domain with no
/// current reading is honest `Partial`/unhealthy rather than a fabricated
/// zero.
#[must_use]
pub(super) fn health_rows(snapshot: &SystemSnapshot) -> Vec<HealthRow> {
    let observed = projection::HealthObservation::from(snapshot);
    let mut rows = Vec::new();

    let cpu_usage = observed.cpu_usage_pct;
    // The clock piece goes through the source-aware readout so a BogoMIPS-only
    // host (VM, no cpufreq) reads "BogoMIPS", never a fake MHz clock — the
    // same rule as the Performance Speed row (perf_overview).
    let cpu_freq = observed.cpu_frequency;
    let cpu_temp = observed.cpu_temperature_c;
    let cpu_text = match (cpu_usage, cpu_freq.as_str()) {
        (Some(usage), "") | (Some(usage), "—") => format!("{usage:.1}%"),
        (Some(usage), freq) => format!("{usage:.1}% · {freq}"),
        (None, freq) if freq.is_empty() || freq == "—" => missing_value(),
        (None, freq) => freq.to_string(),
    };
    let cpu_temp_text = cpu_temp.map_or_else(missing_value, |temp| format!("{temp:.0} °C"));
    rows.push(HealthRow {
        label: "CPU".into(),
        value: format!("{cpu_text} · {cpu_temp_text}"),
        healthy: cpu_usage.is_some(),
    });

    let memory = match (observed.memory_used_bytes, observed.memory_total_bytes) {
        (Some(used), Some(total)) if total > 0 => {
            Some(format!("{} / {}", bytes(used), bytes(total)))
        }
        _ => None,
    };
    let swap = match (observed.swap_used_bytes, observed.swap_total_bytes) {
        (Some(used), Some(total)) if total > 0 => {
            Some(format!("{} / {}", bytes(used), bytes(total)))
        }
        _ => None,
    };
    let memory_healthy = memory.is_some();
    let swap_healthy = swap.is_some();
    rows.push(HealthRow {
        label: "Memory".into(),
        value: memory.unwrap_or_else(missing_value),
        healthy: memory_healthy,
    });
    rows.push(HealthRow {
        label: "Swap".into(),
        value: swap.unwrap_or_else(missing_value),
        healthy: swap_healthy,
    });

    let disk_text = if snapshot.disks.is_empty() {
        missing_value()
    } else {
        format!(
            "{} device{} · {}",
            snapshot.disks.len(),
            if snapshot.disks.len() == 1 { "" } else { "s" },
            snapshot.disks[0].model,
        )
    };
    rows.push(HealthRow {
        label: "Disks".into(),
        value: disk_text,
        healthy: !snapshot.disks.is_empty(),
    });

    let network_text = if snapshot.networks.is_empty() {
        missing_value()
    } else {
        format!(
            "{} interface{} · {}",
            snapshot.networks.len(),
            if snapshot.networks.len() == 1 {
                ""
            } else {
                "s"
            },
            snapshot.networks[0].interface_name,
        )
    };
    rows.push(HealthRow {
        label: "Networks".into(),
        value: network_text,
        healthy: !snapshot.networks.is_empty(),
    });

    let gpu_text = if snapshot.gpu.is_empty() {
        missing_value()
    } else {
        format!(
            "{} GPU{} · {}",
            snapshot.gpu.len(),
            if snapshot.gpu.len() == 1 { "" } else { "s" },
            snapshot.gpu[0].brand,
        )
    };
    rows.push(HealthRow {
        label: "GPU".into(),
        value: gpu_text,
        healthy: !snapshot.gpu.is_empty(),
    });

    rows.push(HealthRow {
        label: "System".into(),
        value: format!(
            "uptime {} · {} processes · {} threads",
            taskmanager_shell::presentation::duration(snapshot.uptime_secs),
            snapshot.processes,
            snapshot
                .threads
                .map_or_else(missing_value, |threads| threads.to_string()),
        ),
        healthy: snapshot.processes > 0,
    });

    rows
}

fn health_row<'a>(
    theme_snapshot: &taskmanager_theme::Theme,
    row: HealthRow,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    row![
        text(row.label.clone()).width(Length::Fixed(150.0)),
        text(row.value.clone()).width(Length::Fill),
        text(if row.healthy { "OK" } else { "Unavailable" })
            .size(f32::from(tokens::FONT_12))
            .color(theme::status_color(theme_snapshot, row.healthy))
            .width(Length::Fixed(110.0)),
    ]
    .spacing(8)
    .padding(4)
    .width(Length::Fill)
    .into()
}

fn alert_row<'a>(
    _theme_snapshot: &taskmanager_theme::Theme,
    metric: AlertMetric,
    shell: &taskmanager_shell::ShellApp,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    row![
        text(metric_label(metric)).width(Length::Fixed(190.0)),
        text(suggestion_text(metric, shell)).width(Length::Fill),
    ]
    .spacing(8)
    .padding(4)
    .width(Length::Fill)
    .into()
}

fn panel<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    title: &'static str,
    body: Element<'a, Message, iced::Theme, iced::Renderer>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    container(
        column![
            text(title)
                .size(f32::from(tokens::FONT_14))
                .color(theme::muted_text_color(theme_snapshot)),
            body,
        ]
        .spacing(6)
        .width(Length::Fill),
    )
    .style(move |_| theme::panel_style(theme_snapshot))
    .padding(10)
    .width(Length::Fill)
    .into()
}

/// Thermal heat-map badges and fan tachometer gauges panel.
pub(crate) fn sensors_and_thermal_panel<'a>(
    snapshot: &SystemSnapshot,
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let mut items: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();

    // 1. Thermal readings
    let temps = projection::thermal_readings(snapshot);

    if !temps.is_empty() {
        let mut temp_pills: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();
        for (label, temp) in temps {
            let (bg_color, tag) = if temp < 45.0 {
                (crate::theme_binding::color(theme_snapshot.network), "Cool")
            } else if temp < 70.0 {
                (
                    crate::theme_binding::color(theme_snapshot.palette().accent),
                    "Normal",
                )
            } else if temp < 85.0 {
                (
                    crate::theme_binding::color(theme_snapshot.palette().warning),
                    "Warm",
                )
            } else {
                (
                    crate::theme_binding::color(theme_snapshot.palette().danger),
                    "Hot",
                )
            };

            let pill = container(
                row![
                    text(format!("{label}: {:.0}°C", temp.round()))
                        .size(f32::from(tokens::FONT_11)),
                    container(text(tag).size(f32::from(tokens::FONT_9)))
                        .padding([1, 4])
                        .style(move |_| container::Style {
                            background: Some(iced::Background::Color(bg_color)),
                            border: iced::Border {
                                radius: 3.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            )
            .padding([3, 6])
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(crate::theme_binding::color(
                    theme_snapshot.shade,
                ))),
                border: iced::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: crate::theme_binding::color(theme_snapshot.palette().border),
                },
                ..Default::default()
            });

            temp_pills.push(pill.into());
        }

        items.push(
            column![
                text("Thermal Heatmap & Sensors")
                    .size(f32::from(tokens::FONT_12))
                    .style(move |_| text::Style {
                        color: Some(theme::muted_text_color(theme_snapshot)),
                    }),
                row(temp_pills).spacing(6).wrap(),
            ]
            .spacing(4)
            .into(),
        );
    }

    if items.is_empty() {
        None
    } else {
        Some(panel(
            theme_snapshot,
            "Sensors & Thermal Health",
            column(items).spacing(6).into(),
        ))
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/health_tests.rs"]
mod tests;
