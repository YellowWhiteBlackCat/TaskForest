//! Durable application-history page.

use super::*;
use crate::app::AppHistoryRowModel;
use crate::app_history_chart::{SPARK_HEIGHT, Sparkline};
use crate::{focus, theme};
use iced::Length;
use iced::widget::{canvas, column, container, row, scrollable, text};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use taskmanager_application::{ApplicationHistoryRow, ApplicationHistoryStatus, HistoryWindow};
use taskmanager_theme::tokens;

#[derive(Clone)]
struct AppHistoryTableModel {
    rows: Rc<Vec<AppHistoryRowModel>>,
    accent: iced::Color,
    window: VirtualWindow,
}

pub(crate) const APP_HISTORY_HEADER_HEIGHT: f32 = 32.0;
pub(crate) const APP_HISTORY_ROW_HEIGHT: f32 = 32.0;

fn app_history_table_body(
    model: &AppHistoryTableModel,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    virtual_table_body(model.window, Length::Fill, |start, end| {
        model
            .rows
            .get(start..end)
            .unwrap_or(&[])
            .iter()
            .map(|row| app_history_row(&row.row, &row.samples, model.accent))
            .collect()
    })
}

fn app_history_table_key(generation: u64, theme_snapshot: &taskmanager_theme::Theme) -> u64 {
    let mut hasher = DefaultHasher::new();
    generation.hash(&mut hasher);
    theme_snapshot.skin.label().hash(&mut hasher);
    theme_snapshot.mode.label().hash(&mut hasher);
    theme_snapshot.dark.hash(&mut hasher);
    theme_snapshot.hc.hash(&mut hasher);
    theme_snapshot.ui_font.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn app_history_page(
    app: &crate::IcedApp,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let projection = app.application_history_projection();
    let history_model = app.projected_app_history_model();
    let title = row![
        // Type scale is px tokens on the Small baseline; the application-wide
        // `renderer_scale` provides the UiSize product scaling (single track).
        text(t("history.application.title")).size(f32::from(tokens::FONT_18)),
        iced::widget::Space::new().width(Length::Fill),
        history_window_controls(theme_snapshot, projection.selected_window),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    if projection.status != ApplicationHistoryStatus::Ready {
        let (heading, detail) = history_state_copy(projection.status);
        let detail = projection.unavailable_reason.map_or_else(
            || detail.to_owned(),
            |reason| format!("{detail} ({})", reason.stable_code()),
        );
        return column![
            title,
            text(heading).size(f32::from(tokens::FONT_14)),
            text(detail).size(f32::from(tokens::FONT_12))
        ]
        .spacing(8)
        .into();
    }

    let window = VirtualWindow::for_rows(
        history_model.rows.len(),
        app.app_history_scroll_y(),
        app.app_history_virtual_viewport_height(),
        APP_HISTORY_ROW_HEIGHT,
        APP_HISTORY_HEADER_HEIGHT,
    );
    let model = AppHistoryTableModel {
        rows: Rc::clone(&history_model.rows),
        accent: theme::color(theme_snapshot.palette().accent),
        window,
    };
    let key = virtual_table_key(
        app_history_table_key(history_model.generation, theme_snapshot),
        window,
    );
    let body = iced::widget::lazy(key, move |_| app_history_table_body(&model));
    let panel = container(
        column![
            container(app_history_header_row())
                .height(Length::Fixed(APP_HISTORY_HEADER_HEIGHT))
                .width(Length::Fill),
            body,
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .style(move |_| theme::panel_style(theme_snapshot));
    let panel = scrollable(panel)
        .id(app.app_history_scroll_id())
        .height(Length::Fill)
        .width(Length::Fill)
        .on_scroll(Message::AppHistoryScrolled);
    column![title, panel].spacing(8).height(Length::Fill).into()
}

fn history_window_controls<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    selected: HistoryWindow,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let mut controls = row![].spacing(4);
    for window in HistoryWindow::ALL {
        controls = controls.push(focus::choice_pill(
            theme_snapshot,
            crate::app::FocusTarget::HistoryReplayWindow(window),
            t(history_window_key(window)).to_owned(),
            selected == window,
            Message::SelectHistoryReplayWindow(window),
        ));
    }
    controls.into()
}

fn history_window_key(window: HistoryWindow) -> &'static str {
    match window {
        HistoryWindow::OneHour => "perf.replay.window.1h",
        HistoryWindow::TwentyFourHours => "perf.replay.window.24h",
        HistoryWindow::SevenDays => "perf.replay.window.7d",
    }
}

fn history_state_copy(status: ApplicationHistoryStatus) -> (&'static str, &'static str) {
    match status {
        ApplicationHistoryStatus::Disabled => (
            t("history.application.disabled"),
            t("history.application.disabled_detail"),
        ),
        ApplicationHistoryStatus::Unavailable => (
            t("history.application.unavailable"),
            t("history.application.unavailable_detail"),
        ),
        ApplicationHistoryStatus::Connecting => (
            t("history.application.connecting"),
            t("history.application.connecting_detail"),
        ),
        ApplicationHistoryStatus::Collecting => (
            t("history.application.collecting"),
            t("history.application.collecting_detail"),
        ),
        ApplicationHistoryStatus::Ready => (t("history.application.title"), ""),
    }
}

const APP_HISTORY_CPU_WIDTH: f32 = 90.0;
const APP_HISTORY_MEM_WIDTH: f32 = 110.0;
const APP_HISTORY_PROCESS_WIDTH: f32 = 110.0;
const APP_HISTORY_SPARK_WIDTH: f32 = 140.0;

fn app_history_header_row() -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let size = f32::from(tokens::FONT_12);
    row![
        text(t("common.name")).size(size).width(Length::Fill),
        text(t("history.application.peak_cpu"))
            .size(size)
            .width(Length::Fixed(APP_HISTORY_CPU_WIDTH)),
        text(t("history.application.peak_memory"))
            .size(size)
            .width(Length::Fixed(APP_HISTORY_MEM_WIDTH)),
        text(t("history.application.peak_processes"))
            .size(size)
            .width(Length::Fixed(APP_HISTORY_PROCESS_WIDTH)),
        text(t("proc.trend"))
            .size(size)
            .width(Length::Fixed(APP_HISTORY_SPARK_WIDTH)),
    ]
    .spacing(8)
    .into()
}

fn app_history_row(
    row_model: &ApplicationHistoryRow,
    samples: &Rc<[f32]>,
    accent: iced::Color,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let provenance = t(if row_model.identity.is_verified() {
        "history.application.verified"
    } else {
        "history.application.unverified"
    });
    let process_peak = row_model
        .peak_process_count()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or_else(missing, |value| format!("{:.0}", value));
    let body_size = f32::from(tokens::FONT_14);
    let caption_size = f32::from(tokens::FONT_11);
    let name = row![
        text(row_model.display_name().to_owned()).size(body_size),
        text(provenance).size(caption_size),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);
    let cpu = row_model
        .peak_cpu_usage_pct()
        .filter(|value| value.is_finite())
        .map_or_else(missing, |value| format!("{value:.1}%"));
    let memory = row_model
        .peak_memory_bytes()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or_else(missing, |value| bytes(value.min(u64::MAX as f64) as u64));
    let spark: Element<'static, Message, iced::Theme, iced::Renderer> =
        if samples.iter().filter(|sample| sample.is_finite()).count() >= 2 {
            canvas::Canvas::new(Sparkline::new(Rc::clone(samples), accent))
                .width(Length::Fixed(APP_HISTORY_SPARK_WIDTH))
                .height(Length::Fixed(SPARK_HEIGHT))
                .into()
        } else {
            text(missing())
                .size(f32::from(tokens::FONT_11))
                .width(Length::Fixed(APP_HISTORY_SPARK_WIDTH))
                .into()
        };
    container(
        row![
            name,
            text(cpu)
                .size(body_size)
                .width(Length::Fixed(APP_HISTORY_CPU_WIDTH)),
            text(memory)
                .size(body_size)
                .width(Length::Fixed(APP_HISTORY_MEM_WIDTH)),
            text(process_peak)
                .size(body_size)
                .width(Length::Fixed(APP_HISTORY_PROCESS_WIDTH)),
            spark,
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .height(Length::Fixed(APP_HISTORY_ROW_HEIGHT))
    .width(Length::Fill)
    .into()
}

fn missing() -> String {
    taskmanager_shell::presentation::missing_value()
}
