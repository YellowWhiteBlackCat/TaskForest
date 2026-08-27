//! Performance page history replay panel for Iced.
//!
//! Renders persisted historical time series over a 1h/24h/7d window with
//! peak summaries, gap indicators, and downsampled mini graphs.

use std::rc::Rc;

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::HistoryWindow;
use taskmanager_application::i18n::t;
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

pub fn history_replay_panel<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    state: &'a IcedHistoryReplay,
    local_time_rules: &'a taskmanager_application::LocalTimeRulesObservation,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let accent = theme::color(theme_snapshot.palette().accent);

    let toggle_btn = focus::dynamic_button(
        theme_snapshot,
        FocusTarget::HistoryReplayToggle,
        if state.is_open() {
            t("perf.replay.back_to_live").to_string()
        } else {
            t("perf.replay.title").to_string()
        },
        Message::ToggleHistoryReplay,
        false,
    );

    let mut header = row![toggle_btn].spacing(8).align_y(iced::Alignment::Center);

    if state.is_open() {
        for window in HistoryWindow::ALL {
            let label = history_window_label(window);
            let is_active = state.window() == window;
            let pill = focus::choice_pill(
                theme_snapshot,
                FocusTarget::HistoryReplayWindow(window),
                label.to_string(),
                is_active,
                Message::SelectHistoryReplayWindow(window),
            );
            header = header.push(pill);
        }

        let refresh_btn = focus::dynamic_button(
            theme_snapshot,
            FocusTarget::HistoryReplayRefresh,
            t("common.refresh").to_string(),
            Message::RefreshHistoryReplay,
            false,
        );
        header = header.push(refresh_btn);
    }

    let mut panel = column![header].spacing(8);

    if state.is_open() {
        if state.is_loading() {
            panel = panel.push(
                text(t("common.collecting_telemetry"))
                    .size(f32::from(tokens::FONT_12))
                    .color(muted),
            );
        } else if let Some(error) = state.failure() {
            panel = panel.push(
                text(error.to_string())
                    .size(f32::from(tokens::FONT_12))
                    .color(theme::color(theme_snapshot.palette().danger)),
            );
            if let Some(last_good_window) = state.rows_window()
                && last_good_window != state.window()
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
                    taskmanager_shell::presentation::local_timestamp(
                        loaded_at_ms,
                        local_time_rules,
                    )
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
                let key_name = format!("{:?}", row_item.key);
                let peak_str = row_item
                    .peak_value
                    .map(|value| format!("{}: {value:.1}", t("perf.replay.peak")))
                    .unwrap_or_else(taskmanager_shell::presentation::missing_value);
                let gaps_str = format!(
                    "{}: {}, {}: {}",
                    t("perf.replay.observed"),
                    row_item.observed,
                    t("perf.replay.gaps"),
                    row_item.gaps,
                );

                let summary = row![
                    text(key_name)
                        .size(f32::from(tokens::FONT_12))
                        .width(Length::Fixed(160.0)),
                    text(peak_str)
                        .size(f32::from(tokens::FONT_12))
                        .color(accent)
                        .width(Length::Fixed(100.0)),
                    text(gaps_str)
                        .size(f32::from(tokens::FONT_11))
                        .color(muted)
                        .width(Length::Fill),
                ]
                .spacing(8);

                let graph_elem = device_chart::device_mini_graph(
                    Rc::clone(&row_item.samples),
                    device_chart::DeviceMetricScale::Percent,
                    accent,
                    t("common.throughput").to_string(),
                    theme_snapshot,
                    device_chart::GraphPrefs {
                        smooth: true,
                        max_override: None,
                        hover: true,
                    },
                );

                panel = panel.push(column![summary, graph_elem].spacing(4));
            }
        }
    }

    container(panel).padding(8).width(Length::Fill).into()
}
