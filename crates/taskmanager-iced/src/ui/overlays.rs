//! Iced modal projections for shared shell overlays.
//!
//! The shell owns precedence and state transitions; this module only maps
//! typed command help, observed threshold suggestions, and the shared
//! process-properties overlay into an opaque iced
//! layer so page input cannot leak through the modal.

use iced::widget::{column, container, opaque, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_core::core::alerts::{
    AlertMetric, InsufficientReason, SuggestedThreshold, SuggestionConfidence,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::{
    device_status_i18n_key, effective_smart_status, has_smart_fields,
};
use taskmanager_theme::{Theme, tokens};

use crate::app::Message;
use crate::focus;
use crate::theme;
use crate::ui::components::key_value_rows;

pub(crate) mod alerts;
pub(crate) use alerts::*;

pub(crate) mod help;
pub(crate) use help::*;

pub(crate) mod process_details;
pub(crate) use process_details::*;
mod process_details_projection;

pub(crate) mod run_task;
pub(crate) use run_task::*;

pub(crate) mod service_log;
pub(crate) use service_log::*;

/// Select the shell's highest-priority informational overlay.
pub(super) fn render<'a>(
    app: &'a crate::IcedApp,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let shell = &app.shell;
    let theme_snapshot = app.theme();
    if shell.service_log.is_some() {
        Some(service_log_overlay(app))
    } else if properties_target(shell).is_some() {
        Some(details_overlay(app))
    } else if shell.help_open() {
        Some(help_overlay(theme_snapshot, app.modal_appear_progress()))
    } else if shell.suggestions_open() {
        Some(suggestions_overlay(
            theme_snapshot,
            shell,
            app.modal_appear_progress(),
        ))
    } else {
        None
    }
}

/// Render the read-only SMART detail surface for one observed disk. The
/// provider snapshot is reused directly; opening the dialog never invents a
/// second health read or blocks the Iced event loop.
pub(super) fn smart_overlay<'a>(
    app: &'a crate::IcedApp,
    index: usize,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    use crate::app::FocusTarget;
    let body: Element<'a, Message, iced::Theme, iced::Renderer> =
        match app.shell.projection().snapshot.as_ref() {
            Some(snapshot) => match snapshot.disks.get(index) {
                Some(disk) => {
                    let mut col = column![
                        text(crate::ui::perf_devices::disk_title(disk))
                            .size(f32::from(tokens::FONT_14)),
                        key_value_rows(smart_rows(disk)),
                    ]
                    .spacing(8);

                    if let Some(pending) = app.shell.pending_smart_self_test()
                        && pending.device_id.as_str() == disk.device_id
                    {
                        let confirm_bar = row![
                            text(format!(
                                "{} {:?} {} {}?",
                                t("common.confirm"),
                                pending.kind,
                                t("health.smart_self_test"),
                                pending.display_name
                            ))
                            .size(f32::from(tokens::FONT_12)),
                            focus::button(
                                app.theme(),
                                FocusTarget::ConfirmSmartSelfTest,
                                t("common.confirm"),
                                Message::ConfirmSmartSelfTest,
                                true,
                            ),
                            focus::button(
                                app.theme(),
                                FocusTarget::CancelSmartSelfTest,
                                t("common.cancel"),
                                Message::DismissOverlay,
                                false,
                            ),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center);
                        col = col.push(confirm_bar);
                    } else {
                        let (smart_obs, _) = app.shell.projection().smart_projection();
                        let report = smart_obs
                            .observations()
                            .iter()
                            .find(|obs| obs.device_id.as_str() == disk.device_id)
                            .map(|obs| &obs.report);

                        let is_running = report.is_some_and(|r| {
                            r.phase == taskmanager_core::core::SmartSelfTestPhase::Running
                        });
                        let can_test = !is_running
                            && (disk.smart_availability
                                == taskmanager_core::core::metrics::SmartAvailability::Available
                                || has_smart_fields(disk));

                        let mut test_row = row![
                            text(t("health.smart_self_test")).size(f32::from(tokens::FONT_12)),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center);

                        if can_test {
                            test_row = test_row
                                .push(focus::ghost_button(
                                    app.theme(),
                                    FocusTarget::SmartSelfTestShort { index },
                                    t("health.short_test"),
                                    Message::RequestSmartSelfTest {
                                        index,
                                        kind: taskmanager_core::core::SmartSelfTestKind::Short,
                                    },
                                ))
                                .push(focus::ghost_button(
                                    app.theme(),
                                    FocusTarget::SmartSelfTestExtended { index },
                                    t("health.extended_test"),
                                    Message::RequestSmartSelfTest {
                                        index,
                                        kind: taskmanager_core::core::SmartSelfTestKind::Extended,
                                    },
                                ));
                        } else if is_running {
                            test_row = test_row.push(
                                text(t("health.phase_running")).size(f32::from(tokens::FONT_12)),
                            );
                        }

                        if let Some(r) = report {
                            let mut report_info = format!("{:?}", r.phase);
                            if let Some(pct) = r.progress_pct {
                                report_info.push_str(&format!(" ({pct:.0}%)"));
                            }
                            test_row = test_row.push(
                                text(report_info).size(f32::from(tokens::FONT_11)).style(
                                    move |_| iced::widget::text::Style {
                                        color: Some(theme::muted_text_color(app.theme())),
                                    },
                                ),
                            );
                        }

                        col = col.push(test_row);
                    }

                    col.into()
                }
                None => text(t("disk.empty")).into(),
            },
            None => text(t("common.collecting_telemetry")).into(),
        };
    modal_overlay(
        app.theme(),
        t("disk.smart_status"),
        t("disk.observed_hint"),
        body,
        app.modal_appear_progress(),
    )
}

fn smart_rows(disk: &taskmanager_core::core::metrics::DiskMetrics) -> Vec<(String, String)> {
    let mut rows = vec![(
        t("disk.smart_status").to_owned(),
        t(device_status_i18n_key(effective_smart_status(disk))).to_owned(),
    )];
    if let Some(temperature) = disk.smart_temperature_c {
        let label = if disk.smart_critical_warning == Some(true) {
            format!("{} ⚠", t("common.temperature"))
        } else {
            t("common.temperature").to_owned()
        };
        let value = match disk.smart_temp_critical_c {
            Some(critical) if critical > 0.0 => {
                format!("{temperature:.0} °C (critical: {critical:.0} °C)")
            }
            _ => format!("{temperature:.0} °C"),
        };
        rows.push((label, value));
    } else if let Some(critical) = disk.smart_temp_critical_c {
        rows.push((
            t("disk.critical_temp").to_owned(),
            format!("{critical:.0} °C"),
        ));
    }
    if let Some(percent) = disk.smart_percent_used {
        let value = if percent >= 100.0 {
            format!("{percent:.0}% ⚠ {}", t("disk.exceeded_rated_life"))
        } else {
            format!("{percent:.0}%")
        };
        rows.push((t("disk.endurance_used").to_owned(), value));
    }
    if let Some(hours) = disk.smart_power_on_hours {
        rows.push((
            t("disk.power_on_hours").to_owned(),
            format!(
                "{hours} h ({:.1} yr, {} d)",
                hours as f64 / 8_766.0,
                hours / 24
            ),
        ));
    }
    if disk.smart_critical_warning == Some(true) {
        rows.push((
            t("disk.smart_status").to_owned(),
            t("disk.warning_text").to_owned(),
        ));
    }
    rows
}

fn suggestions_overlay<'a>(
    theme_snapshot: &'a Theme,
    shell: &ShellApp,
    appear: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let rows: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = AlertMetric::ALL
        .into_iter()
        .map(|metric| suggestion_row(metric, shell))
        .collect();
    modal_overlay(
        theme_snapshot,
        t("alerts.threshold_suggestions"),
        t("alerts.observed_hint"),
        scrollable(column(rows).spacing(4))
            .height(Length::Fixed(260.0))
            .width(Length::Fill)
            .into(),
        appear,
    )
}

/// Shared modal frame: an elevated panel with a soft shadow centered over a
/// dimmed scrim. Page input cannot leak through the modal because the base
/// tree is removed from the returned element tree (see [`super::view`]).
pub(crate) fn modal_overlay<'a>(
    theme_snapshot: &'a Theme,
    title: &'static str,
    subtitle: &'static str,
    body: Element<'a, Message, iced::Theme, iced::Renderer>,
    appear: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let appear = appear.clamp(0.0, 1.0);
    // The entrance fade drives the scrim darkening and the panel lift: the
    // scrim blends toward its full token value and the panel's background
    // alpha + top padding settle as the progress reaches 1.0. All values are
    // token-derived (ADR-017); the progress itself comes from the tick, so
    // the renderer never reads a clock.
    let panel = container(
        column![
            text(title).size(f32::from(tokens::FONT_18)),
            text(subtitle).size(f32::from(tokens::FONT_12)),
            body,
            row![focus::modal_close(theme_snapshot)].width(Length::Fill)
        ]
        .spacing(8)
        .width(Length::Fill),
    )
    .style(move |_| theme::elevated_style_with(theme_snapshot, appear))
    .padding([16.0 + (1.0 - appear) * 6.0, 16.0])
    .width(Length::Fixed(680.0));

    container(opaque(panel))
        .style(move |_| theme::scrim_style_with(theme_snapshot, appear))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

fn suggestion_row<'a>(
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

pub(crate) fn suggestion_text(metric: AlertMetric, shell: &ShellApp) -> String {
    match shell.alert_suggestions.suggest(metric) {
        SuggestedThreshold::Suggested {
            threshold,
            hysteresis,
            sample_count,
            confidence,
            ..
        } => format!(
            "Suggested {:.1}{} · clear ±{hysteresis:.1} · {} · n={sample_count}",
            threshold,
            metric_unit(metric),
            confidence_label(confidence),
        ),
        SuggestedThreshold::Insufficient {
            sample_count,
            required,
            reason,
        } => format!(
            "Insufficient · {} ({sample_count}/{required})",
            insufficient_reason_label(reason)
        ),
    }
}

pub(crate) fn metric_label(metric: AlertMetric) -> &'static str {
    match metric {
        AlertMetric::CpuUsagePercent => t("alerts.metric_cpu"),
        AlertMetric::MemoryUsagePercent => t("alerts.metric_memory"),
        AlertMetric::DiskTemperatureC => t("alerts.metric_disk_temperature"),
        AlertMetric::SmartPercentUsed => t("alerts.metric_smart_used"),
        AlertMetric::SmartCriticalWarning => t("alerts.metric_smart_critical"),
    }
}

fn metric_unit(metric: AlertMetric) -> &'static str {
    match metric {
        AlertMetric::CpuUsagePercent
        | AlertMetric::MemoryUsagePercent
        | AlertMetric::SmartPercentUsed => "%",
        AlertMetric::DiskTemperatureC => "°C",
        AlertMetric::SmartCriticalWarning => "",
    }
}

fn confidence_label(confidence: SuggestionConfidence) -> &'static str {
    match confidence {
        SuggestionConfidence::Low => t("alerts.confidence_low"),
        SuggestionConfidence::High => t("alerts.confidence_high"),
    }
}

fn insufficient_reason_label(reason: InsufficientReason) -> &'static str {
    match reason {
        InsufficientReason::TooFewSamples => t("alerts.insufficient_samples"),
        InsufficientReason::UnsupportedMetric => t("alerts.insufficient_unsupported"),
    }
}
