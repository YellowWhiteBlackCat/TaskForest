//! Performance page history replay view for Iced (GPUI parity).
//!
//! While open, this view REPLACES every device's live graphs as the
//! Performance main area — read-only persisted series over a 1h/24h/7d
//! window with peak summaries, gap counts, clock-jump notes, and
//! downsampled graphs colored per metric family. The open/close toggle
//! lives above the workspace ([`super::performance`]); the rail keeps
//! navigating while the replay is open.

use std::rc::Rc;

use iced::widget::{canvas, column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_core::core::history::{HistoryMetric, HistorySeriesKey, HistoryWindow};

use taskmanager_theme::tokens;

use crate::app::{FocusTarget, Message, history_replay::IcedHistoryReplay};
use crate::focus;
use crate::theme;
use crate::ui::device_chart;

fn history_window_label(window: HistoryWindow) -> &'static str {
    t(match window {
        HistoryWindow::OneHour => "perf.replay.window.1h",
        HistoryWindow::TwentyFourHours => "perf.replay.window.24h",
        HistoryWindow::SevenDays => "perf.replay.window.7d",
    })
}

/// Human heading for one series: the metric slug plus its device/core scope,
/// stable across locales so tests and captures can key on it (GPUI
/// `row_heading` parity — never a debug format).
#[must_use]
pub(crate) fn row_heading(key: &HistorySeriesKey) -> String {
    let mut heading = key.metric().slug().to_owned();
    if let Some(device) = key.device() {
        heading.push_str(" · ");
        heading.push_str(device.as_str());
    }
    if let Some(core) = key.core_index() {
        heading.push_str(&format!(" · core {core}"));
    }
    heading
}

/// Peak formatting keeps the persisted unit-free axis honest (GPUI parity):
/// three significant decimals, no invented unit suffix — series carry their
/// unit in the metric slug.
#[must_use]
pub(crate) fn format_peak(peak: f64) -> String {
    if peak.abs() >= 100.0 {
        format!("{peak:.0}")
    } else if peak.abs() >= 1.0 {
        format!("{peak:.1}")
    } else {
        format!("{peak:.3}")
    }
}

/// Curve color follows the series' device family, mirroring the live
/// Performance pages' palette (GPUI `series_color` parity).
fn series_color(theme_snapshot: &taskmanager_theme::Theme, metric: HistoryMetric) -> iced::Color {
    match metric {
        HistoryMetric::CpuUsagePct
        | HistoryMetric::CpuCoreUsagePct
        | HistoryMetric::CpuTemperatureC
        | HistoryMetric::CpuFrequencyMhz
        | HistoryMetric::CpuPowerW
        | HistoryMetric::ApplicationCpuUsagePct => {
            taskmanager_theme::iced::color(theme_snapshot.cpu)
        }
        HistoryMetric::MemoryUsedPct
        | HistoryMetric::SwapUsedPct
        | HistoryMetric::ApplicationMemoryBytes => {
            taskmanager_theme::iced::color(theme_snapshot.memory)
        }
        HistoryMetric::StorageActivityPct => taskmanager_theme::iced::color(theme_snapshot.disk),
        HistoryMetric::NetworkRateBps => taskmanager_theme::iced::color(theme_snapshot.network),
        HistoryMetric::GpuUsagePct
        | HistoryMetric::GpuPowerW
        | HistoryMetric::GpuTemperatureC
        | HistoryMetric::GpuFrequencyMhz => taskmanager_theme::iced::color(theme_snapshot.gpu),
        HistoryMetric::BatteryCapacityPct
        | HistoryMetric::BatteryPowerW
        | HistoryMetric::BatteryHealthPct => taskmanager_theme::iced::color(theme_snapshot.battery),
        HistoryMetric::FanRpm | HistoryMetric::FanPwmPct | HistoryMetric::FanTemperatureC => {
            taskmanager_theme::iced::color(theme_snapshot.palette().accent)
        }
        HistoryMetric::UptimeSecs
        | HistoryMetric::ProcessCount
        | HistoryMetric::ThreadCount
        | HistoryMetric::ApplicationProcessCount => {
            taskmanager_theme::iced::color(theme_snapshot.palette().fg)
        }
    }
}

/// Render the replay view that replaces the Performance main area while
/// open. Window pills and refresh mutate state through messages; the view
/// itself renders rows read-only.
pub fn render_history_replay<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    state: &'a IcedHistoryReplay,
    local_time_rules: &'a taskmanager_core::core::time::LocalTimeRulesObservation,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let window = state.window();

    let mut header = row![].spacing(8).align_y(iced::Alignment::Center);
    for candidate in HistoryWindow::ALL {
        let label = history_window_label(candidate);
        let is_active = window == candidate;
        header = header.push(focus::choice_pill(
            theme_snapshot,
            FocusTarget::HistoryReplayWindow(candidate),
            label.to_string(),
            is_active,
            Message::SelectHistoryReplayWindow(candidate),
        ));
    }
    header = header.push(focus::dynamic_button(
        theme_snapshot,
        FocusTarget::HistoryReplayRefresh,
        t("perf.replay.refresh").to_string(),
        Message::RefreshHistoryReplay,
        false,
    ));

    let mut panel = column![header].spacing(8);

    if state.is_loading() {
        panel = panel.push(
            text(t("perf.replay.loading"))
                .size(f32::from(tokens::FONT_12))
                .color(muted),
        );
    } else if let Some(error) = state.failure() {
        panel = panel.push(
            text(error.to_string())
                .size(f32::from(tokens::FONT_12))
                .color(taskmanager_theme::iced::color(
                    theme_snapshot.palette().danger,
                )),
        );
        if let Some(last_good_window) = state.rows_window()
            && last_good_window != window
        {
            panel = panel.push(
                text(format!(
                    "{}: {}",
                    t("perf.replay.last_good_window"),
                    history_window_label(last_good_window),
                ))
                .size(f32::from(tokens::FONT_11))
                .color(muted),
            );
        }
    }
    if let Some(loaded_at_ms) = state.loaded_at_ms() {
        panel = panel.push(
            text(format!(
                "{} {}",
                t("perf.replay.loaded_at"),
                taskmanager_shell::presentation::local_timestamp(loaded_at_ms, local_time_rules)
            ))
            .size(f32::from(tokens::FONT_11))
            .color(muted),
        );
    }
    if !state.is_loading() && state.rows().is_empty() {
        panel = panel.push(
            text(t("perf.replay.empty"))
                .size(f32::from(tokens::FONT_12))
                .color(muted),
        );
    } else {
        for row_item in state.rows() {
            let peak_str = row_item
                .peak_value
                .map(|value| format!("{}: {}", t("perf.replay.peak"), format_peak(value)))
                .unwrap_or_else(taskmanager_shell::presentation::missing_value);
            let gaps_str = format!(
                "{}: {}, {}: {}",
                t("perf.replay.observed"),
                row_item.observed,
                t("perf.replay.gaps"),
                row_item.gaps,
            );
            let clock_note = (row_item.clock_jumps > 0)
                .then(|| format!("{} {}", row_item.clock_jumps, t("perf.replay.clock_jumps")));

            let mut summary = row![
                text(row_heading(&row_item.key)).size(f32::from(tokens::FONT_12)),
                text(peak_str).size(f32::from(tokens::FONT_12)).color(
                    taskmanager_theme::iced::color(theme_snapshot.palette().accent)
                ),
                text(gaps_str).size(f32::from(tokens::FONT_11)).color(muted),
            ]
            .spacing(8);
            if let Some(note) = clock_note {
                summary = summary.push(text(note).size(f32::from(tokens::FONT_11)).color(muted));
            }

            let color = series_color(theme_snapshot, row_item.key.metric());
            let samples = Rc::clone(&row_item.samples);
            let max = device_chart::series_max(device_chart::DeviceMetricScale::AutoPeak, &samples);
            let graph_elem = canvas::Canvas::new(device_chart::DeviceChart {
                samples,
                color,
                max,
                grid_color: taskmanager_theme::iced::color(theme_snapshot.palette().border),
                smooth: true,
                hover: true,
                scale: device_chart::DeviceMetricScale::AutoPeak,
                readout: crate::perf_chart::ReadoutColors {
                    bg: taskmanager_theme::iced::color(theme_snapshot.palette().surface),
                    fg: taskmanager_theme::iced::color(theme_snapshot.palette().fg),
                },
            })
            .width(Length::Fill)
            .height(Length::Fixed(72.0));

            panel = panel.push(column![summary, graph_elem].spacing(4).width(Length::Fill));
        }
    }

    container(panel).padding(8).width(Length::Fill).into()
}
