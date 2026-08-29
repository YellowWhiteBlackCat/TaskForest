//! Iced service-details modal: lifecycle facts plus typed dependency state.
//!
//! The modal owns no provider access. It projects the selected `ServiceItem`
//! and the shared typed dependency lifecycle, while retry and log actions
//! publish ordinary `Message` values back through the application edge.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_core::core::services::{
    ServiceItem, ServiceLogLevelFilter, ServiceLogState, ServiceLogTimeFilter, ServiceRelationKind,
};

use taskmanager_shell::presentation::{control_error_detail, missing_value};
use taskmanager_theme::{Theme, tokens};

use super::overlays::modal_overlay;
use crate::app::{FocusTarget, Message};
use crate::ui::components::{key_value_rows, titled_card};
use crate::{IcedApp, focus, theme};

pub(super) fn open_button_owned(
    theme_snapshot: Theme,
    source_index: usize,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    focus::dynamic_button_owned(
        theme_snapshot,
        FocusTarget::ServiceDetailsOpen {
            index: source_index,
        },
        t("common.details").to_owned(),
        Message::OpenServiceDetailsFor {
            index: source_index,
        },
        false,
    )
}

pub(super) fn render(app: &IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let Some(service_id) = app.service_details_target() else {
        return modal_overlay(
            app.theme(),
            t("dialog.service_details"),
            t("svc.details_hint"),
            text(t("empty.no_services_reported")).into(),
            app.modal_appear_progress(),
        );
    };
    let item = app
        .shell
        .projection()
        .services
        .as_deref()
        .and_then(|services| services.iter().find(|service| &service.id == service_id))
        .cloned();
    let Some(item) = item else {
        return modal_overlay(
            app.theme(),
            t("dialog.service_details"),
            t("svc.details_hint"),
            text(t("empty.no_services_reported")).into(),
            app.modal_appear_progress(),
        );
    };

    let details = app.service_details_snapshot();
    let source_index = app
        .shell
        .projection()
        .services
        .as_deref()
        .and_then(|services| services.iter().position(|service| service.id == item.id));

    let header = row![
        text(item.name.clone()).size(f32::from(tokens::FONT_16)),
        text(item.status.to_string()).size(f32::from(tokens::FONT_12)),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    let matching_pid = app
        .shell
        .projection()
        .processes
        .as_deref()
        .and_then(|procs| {
            let service_stem = item.name.strip_suffix(".service").unwrap_or(&item.name);
            procs
                .iter()
                .find(|p| {
                    p.name.eq_ignore_ascii_case(service_stem)
                        || p.name.eq_ignore_ascii_case(&item.name)
                        || p.cmdline.contains(service_stem)
                })
                .map(|p| p.pid)
        });

    let lifecycle_content: Element<'_, Message, iced::Theme, iced::Renderer> =
        if let Some(pid) = matching_pid {
            column![
                key_value_rows(service_rows(&item, Some(pid))),
                focus::dynamic_button(
                    app.theme(),
                    FocusTarget::ServiceDetailsJumpToProcess,
                    format!("{} (PID {pid})", t("svc.jump_to_process")),
                    Message::JumpToProcess { pid },
                    false,
                ),
            ]
            .spacing(8)
            .into()
        } else {
            key_value_rows(service_rows(&item, None))
        };

    let body = column![
        header,
        scrollable(
            column![
                titled_card(app.theme(), t("svc.lifecycle"), lifecycle_content),
                titled_card(
                    app.theme(),
                    t("svc.dependencies"),
                    dependency_panel(app, &details.dependencies),
                ),
                titled_card(
                    app.theme(),
                    t("svc.logs"),
                    logs_panel(app, details.clone(), source_index),
                ),
            ]
            .spacing(10),
        )
        .height(Length::Fixed(420.0))
        .width(Length::Fill),
    ]
    .spacing(8)
    .into();

    modal_overlay(
        app.theme(),
        t("dialog.service_details"),
        t("svc.details_hint"),
        body,
        app.modal_appear_progress(),
    )
}

fn service_rows(item: &ServiceItem, matching_pid: Option<u32>) -> Vec<(String, String)> {
    let mut rows = vec![
        (t("common.name").to_owned(), value_or_dash(&item.name)),
        (
            t("common.description").to_owned(),
            value_or_dash(&item.description),
        ),
        (t("common.status").to_owned(), item.status.to_string()),
        (
            t("svc.load_state").to_owned(),
            value_or_dash(&item.load_state),
        ),
        (
            t("svc.active_state").to_owned(),
            value_or_dash(&item.active_state),
        ),
        (
            t("svc.sub_state").to_owned(),
            value_or_dash(&item.sub_state),
        ),
    ];
    if let Some(pid) = matching_pid {
        rows.push((t("proc.pid").to_owned(), pid.to_string()));
    }
    rows
}

pub(crate) fn dependency_panel<'a>(
    app: &'a IcedApp,
    lifecycle: &taskmanager_application::ServiceDependenciesLifecycle,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    if let Some(failure) = lifecycle.failure() {
        return container(
            column![
                text(control_error_detail(failure)).size(f32::from(tokens::FONT_12)),
                focus::ghost_button_with_icon(
                    app.theme(),
                    FocusTarget::ServiceDetailsRetry,
                    taskmanager_ui_contract::IconId::Refresh,
                    t("common.refresh"),
                    Message::RefreshServiceDetails,
                ),
            ]
            .spacing(8),
        )
        .style(move |_| warning_panel_style(app.theme()))
        .padding(10)
        .width(Length::Fill)
        .into();
    }
    if lifecycle.is_loading() {
        return container(text(t("svc.details_loading")).size(f32::from(tokens::FONT_12)))
            .style(move |_| theme::card_style(app.theme()))
            .padding(10)
            .width(Length::Fill)
            .into();
    }

    let deps = lifecycle.projected().cloned().unwrap_or_default();
    let mut sections: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();
    let dep_groups = [
        (t("svc.requires"), ServiceRelationKind::Requires),
        (t("svc.wants"), ServiceRelationKind::Wants),
        (t("svc.wanted_by"), ServiceRelationKind::WantedBy),
        (t("svc.after"), ServiceRelationKind::After),
    ];

    for (label, kind) in dep_groups {
        let mut row_items: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();
        let items = deps.relation_targets(&kind).collect::<Vec<_>>();
        if items.is_empty() {
            row_items.push(
                text("—")
                    .size(f32::from(tokens::FONT_12))
                    .style(move |_| text::Style {
                        color: Some(theme::muted_text_color(app.theme())),
                    })
                    .into(),
            );
        } else {
            for target in items {
                let dep_name = target.as_str();
                let target_index = app
                    .shell
                    .projection()
                    .services
                    .as_deref()
                    .and_then(|services| services.iter().position(|service| service.id == *target));

                if let Some(idx) = target_index {
                    let label = app
                        .shell
                        .projection()
                        .services
                        .as_deref()
                        .and_then(|services| services.get(idx))
                        .map_or_else(|| dep_name.to_owned(), |service| service.name.clone());
                    row_items.push(focus::dynamic_button(
                        app.theme(),
                        FocusTarget::ServiceDetailsOpen { index: idx },
                        label,
                        Message::OpenServiceDetailsFor { index: idx },
                        false,
                    ));
                } else {
                    row_items.push(
                        container(text(dep_name.to_owned()).size(f32::from(tokens::FONT_11)))
                            .padding([2, 6])
                            .style(move |_| container::Style {
                                background: Some(iced::Background::Color(
                                    taskmanager_theme::iced::color(app.theme().shade),
                                )),
                                border: iced::Border {
                                    radius: 3.0.into(),
                                    width: 1.0,
                                    color: taskmanager_theme::iced::color(
                                        app.theme().palette().border,
                                    ),
                                },
                                ..Default::default()
                            })
                            .into(),
                    );
                }
            }
        }

        let section = column![
            text(label)
                .size(f32::from(tokens::FONT_12))
                .style(move |_| text::Style {
                    color: Some(theme::muted_text_color(app.theme())),
                }),
            row(row_items).spacing(4).wrap(),
        ]
        .spacing(4);

        sections.push(section.into());
    }

    container(column(sections).spacing(8))
        .style(move |_| theme::card_style(app.theme()))
        .padding(10)
        .width(Length::Fill)
        .into()
}

/// The merged log panel (GPUI `services_view/details.rs` parity): follow
/// controls, then the resolved log lines in their own bounded scroll region.
/// The standalone `svc.open_logs` entry stays available as the full-window /
/// export path when the service still exists in the inventory.
fn logs_panel<'a>(
    app: &'a IcedApp,
    details: crate::app::ServiceDetailsSnapshot,
    source_index: Option<usize>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let mut controls = row![
        focus::button(
            theme_snapshot,
            FocusTarget::ServiceDetailsLogPause,
            if details.log_paused {
                t("svc.logs_follow")
            } else {
                t("svc.logs_pause")
            },
            Message::ToggleServiceDetailsLogPaused,
            false,
        ),
        focus::dynamic_button(
            theme_snapshot,
            FocusTarget::ServiceDetailsLogLevel,
            log_level_label(details.log_level).to_owned(),
            Message::CycleServiceDetailsLogLevel,
            false,
        ),
        focus::dynamic_button(
            theme_snapshot,
            FocusTarget::ServiceDetailsLogTime,
            log_time_label(details.log_time).to_owned(),
            Message::CycleServiceDetailsLogTime,
            false,
        ),
        focus::button(
            theme_snapshot,
            FocusTarget::ServiceDetailsLogCopy,
            t("common.copy"),
            Message::CopyServiceDetailsLog,
            false,
        ),
        focus::ghost_button_with_icon(
            theme_snapshot,
            FocusTarget::ServiceDetailsLogRefresh,
            taskmanager_ui_contract::IconId::Refresh,
            t("common.refresh"),
            Message::RefreshServiceDetailsLogs,
        ),
    ]
    .spacing(6);
    if let Some(index) = source_index {
        controls = controls.push(focus::dynamic_button(
            theme_snapshot,
            FocusTarget::ServiceLogOpen { index },
            t("svc.open_logs").to_owned(),
            Message::OpenServiceLogFor { index },
            false,
        ));
    }

    let lines_body: Element<'_, Message, iced::Theme, iced::Renderer> = match &details.logs {
        ServiceLogState::Ready(lines) => {
            // Each log line is an independently selectable value (GPUI
            // SelectableText parity for the line-level copy workflow): drag a
            // line into the primary clipboard, Ctrl/Cmd-C to the standard
            // clipboard. Whole-block export stays on the copy/export actions
            // by design — the paragraph layer exposes no line metrics, so a
            // block-wide highlight could only be approximate.
            let owner = app.text_selection_owner();
            let rows = lines
                .as_slice()
                .iter()
                .enumerate()
                .map(|(line_index, line)| {
                    let value_id =
                        iced::advanced::widget::Id::from(format!("svc-details-log-{line_index}"));
                    let is_owner = owner.as_ref() == Some(&value_id);
                    crate::ui::components::SelectableText::new(
                        value_id,
                        line.clone(),
                        f32::from(tokens::FONT_11),
                        taskmanager_theme::iced::color(theme_snapshot.palette().fg),
                    )
                    .selection_owner(is_owner)
                    .into()
                })
                .collect::<Vec<Element<'_, Message, iced::Theme, iced::Renderer>>>();
            scrollable(column(rows).spacing(2))
                .height(Length::Fixed(140.0))
                .width(Length::Fill)
                .into()
        }
        state => container(text(log_state_caption(state)).size(f32::from(tokens::FONT_11)))
            .padding(8)
            .width(Length::Fill)
            .into(),
    };

    column![controls, lines_body].spacing(6).into()
}

/// The merged panel's level-filter caption — the same shared-catalog keys the
/// standalone overlay and GPUI's details view render.
fn log_level_label(filter: ServiceLogLevelFilter) -> &'static str {
    match filter {
        ServiceLogLevelFilter::All => t("svc.logs_level_all"),
        ServiceLogLevelFilter::Errors => t("svc.logs_level_errors"),
        ServiceLogLevelFilter::WarningsAndErrors => t("svc.logs_level_warnings"),
        ServiceLogLevelFilter::InfoAndAbove => t("svc.logs_level_info"),
    }
}

/// The merged panel's time-filter caption (shared-catalog keys).
fn log_time_label(filter: ServiceLogTimeFilter) -> &'static str {
    match filter {
        ServiceLogTimeFilter::All => t("svc.logs_time_all"),
        ServiceLogTimeFilter::LastHour => t("svc.logs_time_hour"),
        ServiceLogTimeFilter::LastDay => t("svc.logs_time_day"),
    }
}

/// The honest caption for a non-ready log state (loading / empty / failure).
/// Pure so the state→caption mapping is table-tested headlessly.
fn log_state_caption(state: &ServiceLogState) -> &'static str {
    match state {
        ServiceLogState::Ready(_) => t("svc.logs"),
        ServiceLogState::Loading => t("svc.logs_loading"),
        ServiceLogState::Empty => t("svc.logs_empty"),
        ServiceLogState::Unavailable(_) => t("svc.logs_failed"),
    }
}

fn value_or_dash(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        missing_value()
    } else {
        value.to_owned()
    }
}

fn warning_panel_style(theme_snapshot: &Theme) -> iced::widget::container::Style {
    let mut style = theme::panel_style(theme_snapshot);
    style.border.color = taskmanager_theme::iced::color(theme_snapshot.palette().warning);
    style
}
