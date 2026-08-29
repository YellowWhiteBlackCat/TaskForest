//! Service log overlay for Iced.

use iced::widget::{column, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_core::core::services::{
    ServiceLogAvailability, ServiceLogLevelFilter, ServiceLogTimeFilter,
};
use taskmanager_core::core::target::ServiceId;

use taskmanager_shell::ShellApp;
use taskmanager_theme::tokens;

use crate::app::Message;
use crate::focus;

/// Render the shared service-log feed as an Iced modal.
pub(crate) fn service_log_overlay<'a>(
    app: &'a crate::IcedApp,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let shell = &app.shell;
    let Some(open) = shell.service_log.as_ref() else {
        return column![].into();
    };
    let entries = shell
        .visible_service_log_entries(app.service_log_now_micros())
        .unwrap_or_default();
    let lines: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = if entries.is_empty() {
        vec![text(service_log_empty_message(shell)).into()]
    } else {
        entries
            .iter()
            .map(|entry| text(format!("[{:?}] {}", entry.level, entry.message)).into())
            .collect()
    };
    let feed = &open.feed;
    let follow_label = format!(
        "{}: {}",
        t("svc.logs_follow"),
        if feed.follow { "on" } else { "off" }
    );
    let pause_label = if feed.paused {
        t("svc.logs_follow")
    } else {
        t("svc.logs_pause")
    };
    let controls = row![
        focus::dynamic_button(
            theme_snapshot,
            crate::app::FocusTarget::ServiceLogFollow,
            follow_label,
            Message::ToggleLogFollow,
            false,
        ),
        focus::dynamic_button(
            theme_snapshot,
            crate::app::FocusTarget::ServiceLogPause,
            pause_label.to_owned(),
            Message::ToggleLogPaused,
            false,
        ),
        focus::dynamic_button(
            theme_snapshot,
            crate::app::FocusTarget::ServiceLogLevel,
            log_level_label(feed.level).to_owned(),
            Message::CycleLogLevel,
            false,
        ),
        focus::dynamic_button(
            theme_snapshot,
            crate::app::FocusTarget::ServiceLogTime,
            log_time_label(feed.time).to_owned(),
            Message::CycleLogTime,
            false,
        ),
        focus::dynamic_button(
            theme_snapshot,
            crate::app::FocusTarget::ServiceLogCopy,
            t("common.copy").to_owned(),
            Message::CopyServiceLog,
            false,
        ),
        focus::dynamic_button(
            theme_snapshot,
            crate::app::FocusTarget::ServiceLogExport,
            t("common.export").to_owned(),
            Message::ExportServiceLog,
            false,
        ),
    ]
    .spacing(6);
    let body = column![
        text(format!(
            "{} · {}",
            open.service_id().map_or("—", ServiceId::as_str),
            t("svc.logs")
        ))
        .size(f32::from(tokens::FONT_13)),
        controls,
        scrollable(column(lines).spacing(4))
            .height(Length::Fixed(330.0))
            .width(Length::Fill),
    ]
    .spacing(8)
    .into();
    super::modal_overlay(
        theme_snapshot,
        t("svc.logs"),
        t("svc.logs_controls"),
        body,
        app.modal_appear_progress(),
    )
}

fn service_log_empty_message(shell: &ShellApp) -> String {
    match shell
        .service_log_provider_state()
        .map(|state| state.availability)
    {
        Some(ServiceLogAvailability::Empty) => t("svc.logs_empty").to_owned(),
        Some(ServiceLogAvailability::Loading) => t("svc.logs_loading").to_owned(),
        Some(ServiceLogAvailability::CaughtUp) => t("svc.logs_time_all").to_owned(),
        Some(ServiceLogAvailability::Disconnected | ServiceLogAvailability::Unavailable) => {
            t("svc.logs_failed").to_owned()
        }
        Some(ServiceLogAvailability::Stale) => t("svc.logs_failed").to_owned(),
        Some(ServiceLogAvailability::Available) | None => "Waiting for entries…".to_owned(),
    }
}

fn log_level_label(filter: ServiceLogLevelFilter) -> &'static str {
    match filter {
        ServiceLogLevelFilter::All => t("svc.logs_level_all"),
        ServiceLogLevelFilter::Errors => t("svc.logs_level_errors"),
        ServiceLogLevelFilter::WarningsAndErrors => t("svc.logs_level_warnings"),
        ServiceLogLevelFilter::InfoAndAbove => t("svc.logs_level_info"),
    }
}

fn log_time_label(filter: ServiceLogTimeFilter) -> &'static str {
    match filter {
        ServiceLogTimeFilter::All => t("svc.logs_time_all"),
        ServiceLogTimeFilter::LastHour => t("svc.logs_time_hour"),
        ServiceLogTimeFilter::LastDay => t("svc.logs_time_day"),
    }
}
