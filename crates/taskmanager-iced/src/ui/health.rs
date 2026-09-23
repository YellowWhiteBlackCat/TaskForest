//! Iced system-health modal: per-domain device summary from the shared
//! snapshot + hardware facts, and the alert-rule view consuming the shell's
//! rolling history (`suggest_threshold` — the same source as the TUI's
//! alerts overlay, presented with iced widgets).

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_core::core::alerts::AlertMetric;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::sensors::SensorCenterSnapshot;
use taskmanager_theme::tokens;

use crate::app::Message;
use crate::i18n::{self, Key, Language};
use crate::theme;

use super::overlays::{metric_label, modal_overlay, suggestion_text};
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::duration;
use taskmanager_shell::presentation::{
    bytes, health_score_for_snapshot, health_score_summary, missing_value,
};
use taskmanager_theme::Theme;

mod projection;

/// Stable widget id of the health modal's scrollable body. Production never
/// addresses it; the capture lane uses it to bound one evidence frame (see
/// [`bound_body_to_end`]).
pub(crate) const BODY_SCROLL_ID: &str = "health-modal-body";

/// Bound the health modal body to its end for one capture frame. The absolute
/// offset is the widget's own maximum (Iced clamps a requested offset to the
/// real content height), so the fixed 430px viewport reaches the panels below
/// the summary - including the thermal-zone panel - without a magic pixel
/// target that would rot when a panel's height changes. Capture-only: the
/// caller gates on the evidence-runner marker path, so a production modal
/// keeps the user's own scroll position.
pub(crate) fn bound_body_to_end() -> iced::Task<Message> {
    iced::widget::operation::scroll_to(
        iced::advanced::widget::Id::new(BODY_SCROLL_ID),
        iced::widget::operation::AbsoluteOffset {
            x: None,
            y: Some(f32::MAX),
        },
    )
}

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
                let rows = health_rows(snapshot, language);
                panel(
                    theme_snapshot,
                    i18n::t(language, Key::DeviceSummary),
                    column(
                        rows.into_iter()
                            .map(|row| health_row(theme_snapshot, row, language))
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
        && let Some(sensor_panel) = sensors_and_thermal_panel(snapshot, theme_snapshot, language)
    {
        modal_panels.push(sensor_panel);
    }
    if let Some(sensors) = shell.projection().sensors.as_ref()
        && let Some(zone_panel) = thermal_zone_sensor_panel(sensors, theme_snapshot, language)
    {
        modal_panels.push(zone_panel);
    }
    modal_panels.push(alert_panel);

    // The body scrollbar is EMBEDDED, not floated: iced's default rail floats
    // over the content, so inside this panel-wrapped body it painted over each
    // panel's right inner padding and left the content's left/right insets
    // asymmetric. `Scrollable::spacing` reserves the rail's gutter with the
    // shared spacing token, so the panels keep symmetric padding and the rail
    // sits outside them (the same "content never touches the rail" rule the
    // UI charter states for pinned rails).
    modal_overlay(
        theme_snapshot,
        i18n::t(language, Key::Health),
        i18n::t(language, Key::HealthObservedHint),
        scrollable(column(modal_panels).spacing(12))
            .id(BODY_SCROLL_ID)
            .height(Length::Fixed(430.0))
            .width(Length::Fill)
            .spacing(f32::from(tokens::SPACE_8))
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
/// zero. Every label and composed value resolves through the active
/// [`Language`] so the iced summary reads in the same tongue as the rest of
/// the modal.
#[must_use]
pub(super) fn health_rows(snapshot: &SystemSnapshot, language: Language) -> Vec<HealthRow> {
    let observed = projection::HealthObservation::from(snapshot);
    let mut rows = Vec::new();

    if let Some(score) = health_score_for_snapshot(snapshot) {
        rows.push(HealthRow {
            label: i18n::t(language, Key::HealthScore).to_owned(),
            value: health_score_summary(&score),
            healthy: score.score >= 80,
        });
    }

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
        label: i18n::t(language, Key::Cpu).to_owned(),
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
        label: i18n::t(language, Key::Memory).to_owned(),
        value: memory.unwrap_or_else(missing_value),
        healthy: memory_healthy,
    });
    rows.push(HealthRow {
        label: i18n::t(language, Key::Swap).to_owned(),
        value: swap.unwrap_or_else(missing_value),
        healthy: swap_healthy,
    });

    let disk_text = if snapshot.disks.is_empty() {
        missing_value()
    } else {
        count_value(
            language,
            Key::HealthDeviceCountOne,
            Key::HealthDeviceCountMany,
            snapshot.disks.len(),
            &snapshot.disks[0].model,
        )
    };
    rows.push(HealthRow {
        label: i18n::t(language, Key::Disk).to_owned(),
        value: disk_text,
        healthy: !snapshot.disks.is_empty(),
    });

    let network_text = if snapshot.networks.is_empty() {
        missing_value()
    } else {
        count_value(
            language,
            Key::HealthInterfaceCountOne,
            Key::HealthInterfaceCountMany,
            snapshot.networks.len(),
            &snapshot.networks[0].interface_name,
        )
    };
    rows.push(HealthRow {
        label: i18n::t(language, Key::Network).to_owned(),
        value: network_text,
        healthy: !snapshot.networks.is_empty(),
    });

    let gpu_text = if snapshot.gpu.is_empty() {
        missing_value()
    } else {
        count_value(
            language,
            Key::HealthGpuCountOne,
            Key::HealthGpuCountMany,
            snapshot.gpu.len(),
            &snapshot.gpu[0].brand,
        )
    };
    rows.push(HealthRow {
        label: i18n::t(language, Key::Gpu).to_owned(),
        value: gpu_text,
        healthy: !snapshot.gpu.is_empty(),
    });

    rows.push(HealthRow {
        label: i18n::t(language, Key::SystemDomain).to_owned(),
        value: i18n::t(language, Key::HealthSystemValue)
            .replace("{uptime}", &duration(snapshot.uptime_secs))
            .replace("{processes}", &snapshot.processes.to_string())
            .replace(
                "{threads}",
                &snapshot
                    .threads
                    .map_or_else(missing_value, |threads| threads.to_string()),
            ),
        healthy: snapshot.processes > 0,
    });

    rows
}

/// Resolve one "count + noun + first model" domain value: pick the
/// singular/plural template for the active locale, then substitute the shared
/// named placeholders. Chinese uses the same template for both counts (no
/// plural inflection), so only the selected key differs.
fn count_value(language: Language, one: Key, many: Key, count: usize, model: &str) -> String {
    let key = if count == 1 { one } else { many };
    i18n::t(language, key)
        .replace("{count}", &count.to_string())
        .replace("{model}", model)
}

fn health_row<'a>(
    theme_snapshot: &Theme,
    row: HealthRow,
    language: Language,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    row![
        text(row.label.clone()).width(Length::Fixed(150.0)),
        text(row.value.clone()).width(Length::Fill),
        text(if row.healthy {
            i18n::t(language, Key::HealthVerdictOk)
        } else {
            i18n::t(language, Key::HealthUnavailable)
        })
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
    _theme_snapshot: &Theme,
    metric: AlertMetric,
    shell: &ShellApp,
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
    theme_snapshot: &'a Theme,
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
    theme_snapshot: &'a Theme,
    language: Language,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let mut items: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();

    // 1. Thermal readings
    let temps = projection::thermal_readings(snapshot, language);

    if !temps.is_empty() {
        let mut temp_pills: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();
        for (label, temp) in temps {
            let (bg_color, tag) = if temp < 45.0 {
                (
                    crate::theme_binding::color(theme_snapshot.network),
                    i18n::t(language, Key::HealthThermalCool),
                )
            } else if temp < 70.0 {
                (
                    crate::theme_binding::color(theme_snapshot.palette().accent),
                    i18n::t(language, Key::HealthThermalNormal),
                )
            } else if temp < 85.0 {
                (
                    crate::theme_binding::color(theme_snapshot.palette().warning),
                    i18n::t(language, Key::HealthThermalWarm),
                )
            } else {
                (
                    crate::theme_binding::color(theme_snapshot.palette().danger),
                    i18n::t(language, Key::HealthThermalHot),
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
                text(i18n::t(language, Key::HealthThermalHeatmap))
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
            i18n::t(language, Key::HealthSensorsThermal),
            column(items).spacing(6).into(),
        ))
    }
}

/// The thermal-zone surface: one row per shared `SensorCenterSnapshot`
/// temperature reading, named by the reading's own source label (GPUI
/// health-page parity over the same projection). A failed/unread zone keeps its
/// row with the shared dash — never a fabricated `0.0 °C`. `None` when the
/// snapshot carries no temperature channel at all.
pub(super) fn thermal_zone_sensor_panel<'a>(
    sensors: &SensorCenterSnapshot,
    theme_snapshot: &'a Theme,
    language: Language,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let rows = projection::thermal_zone_rows(sensors);
    if rows.is_empty() {
        return None;
    }
    Some(panel(
        theme_snapshot,
        i18n::t(language, Key::ThermalZones),
        column(
            rows.into_iter()
                .map(|row| thermal_zone_row(theme_snapshot, row))
                .collect::<Vec<_>>(),
        )
        .spacing(1)
        .into(),
    ))
}

/// One thermal-zone row: the reading's own source label owns the bounded left
/// slot, the value the elastic right slot. A present measurement keeps the
/// normal foreground; a typed absence takes the status tint so an unread zone
/// cannot read as a real temperature.
fn thermal_zone_row<'a>(
    theme_snapshot: &'a Theme,
    row: projection::ThermalZoneRow,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let value = text(row.value).width(Length::Fill);
    let value = if row.present {
        value
    } else {
        value.color(theme::status_color(theme_snapshot, false))
    };
    row![text(row.label).width(Length::Fixed(150.0)), value]
        .spacing(8)
        .padding(4)
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/health_tests.rs"]
mod tests;
