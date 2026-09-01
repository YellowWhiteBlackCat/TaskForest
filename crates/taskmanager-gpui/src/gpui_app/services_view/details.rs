//! Service properties, dependency rows, and bounded journal feed rendering.

use gpui::{ClipboardItem, Context, Div, ParentElement, Styled, div, prelude::FluentBuilder, px};

use taskmanager_ui_contract::IconId;

use super::details_state::{ServiceDetailsSnapshot, ServiceLogCopyFeedback};
use crate::gpui_app::elements;
use crate::gpui_app::formatting;
use crate::gpui_app::root::{RootView, prop_row};
use crate::gpui_app::theme::mono_font_with_fallback;
use taskmanager_application::i18n;
use taskmanager_core::core::services::ServiceItem;
use taskmanager_core::core::services::{
    ServiceDeps, ServiceLogErrorKind, ServiceLogLevelFilter, ServiceLogState, ServiceLogTimeFilter,
    ServiceRelationKind,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use taskmanager_ui::primitives::card_surface::CardSurface;

fn log_level_label(filter: ServiceLogLevelFilter) -> &'static str {
    match filter {
        ServiceLogLevelFilter::All => i18n::t("svc.logs_level_all"),
        ServiceLogLevelFilter::Errors => i18n::t("svc.logs_level_errors"),
        ServiceLogLevelFilter::WarningsAndErrors => i18n::t("svc.logs_level_warnings"),
        ServiceLogLevelFilter::InfoAndAbove => i18n::t("svc.logs_level_info"),
    }
}

fn log_time_label(filter: ServiceLogTimeFilter) -> &'static str {
    match filter {
        ServiceLogTimeFilter::All => i18n::t("svc.logs_time_all"),
        ServiceLogTimeFilter::LastHour => i18n::t("svc.logs_time_hour"),
        ServiceLogTimeFilter::LastDay => i18n::t("svc.logs_time_day"),
    }
}

fn format_dependencies(dependencies: &ServiceDeps, kind: &ServiceRelationKind) -> String {
    const MAX_CHARS: usize = 80;
    let dependencies = dependencies
        .relation_targets(kind)
        .map(|target| target.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if dependencies.is_empty() {
        formatting::missing_value()
    } else if dependencies.chars().count() > MAX_CHARS {
        format!(
            "{}…",
            dependencies.chars().take(MAX_CHARS).collect::<String>()
        )
    } else {
        dependencies.to_string()
    }
}

/// Render the service details dialog body from a snapshot of the per-window
/// `RootView.service_details` state (taken by the render-edge caller). The
/// interactive pills mutate that same state through the `RootView` entity they
/// capture — never a shared `thread_local` (which crossed window boundaries).
pub fn render_details(
    theme: &Theme,
    item: &ServiceItem,
    details: ServiceDetailsSnapshot,
    now_micros: u64,
    cx: &mut Context<RootView>,
) -> Div {
    let root = cx.entity();
    let stream_lines: Vec<_> = details
        .feed
        .visible_entries(now_micros)
        .into_iter()
        .map(|entry| format!("[{:?}] {}", entry.level, entry.message))
        .collect();
    let logs = details
        .log_stream
        .resolve_lines(&details.logs, stream_lines);
    let refresh_id = item.id.clone();
    let refresh_root = root.clone();
    let copy_text = logs.copy_text();
    let copy_root = root.clone();
    let pause_id = item.id.clone();
    let pause_root = root.clone();
    let level_id = item.id.clone();
    let level_root = root.clone();
    let time_id = item.id.clone();
    let time_root = root.clone();
    let export_id = item.id.clone();
    let export_name = item.name.clone();
    let export_now_micros = now_micros;
    let export_root = root.clone();
    let actions = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .child(
            elements::Pill::new(
                "service-logs-refresh",
                i18n::t("common.refresh"),
                move |window, cx| {
                    refresh_root.update(cx, |view, cx| {
                        view.refresh_service_logs(&refresh_id);
                        cx.notify();
                    });
                    window.refresh();
                    cx.refresh_windows();
                },
                |_, _, _| {},
            )
            .icon(IconId::Refresh)
            .render(theme),
        )
        .child(elements::pill(
            theme,
            "service-logs-copy",
            i18n::t("common.copy"),
            false,
            false,
            move |window, cx| {
                if let Some(text) = &copy_text {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    copy_root.update(cx, |view, cx| {
                        view.service_details
                            .set_copy_feedback(ServiceLogCopyFeedback::Copied);
                        cx.notify();
                    });
                } else {
                    copy_root.update(cx, |view, cx| {
                        view.service_details
                            .set_copy_feedback(ServiceLogCopyFeedback::NoData);
                        cx.notify();
                    });
                }
                window.refresh();
                cx.refresh_windows();
            },
            |_, _, _| {},
        ))
        .child(elements::pill(
            theme,
            "service-logs-pause",
            if details.feed.paused {
                i18n::t("svc.logs_follow")
            } else {
                i18n::t("svc.logs_pause")
            },
            details.feed.paused,
            false,
            move |window, cx| {
                pause_root.update(cx, |view, cx| {
                    view.service_details.toggle_pause(&pause_id);
                    cx.notify();
                });
                window.refresh();
                cx.refresh_windows();
            },
            |_, _, _| {},
        ))
        .child(elements::pill(
            theme,
            "service-logs-level",
            log_level_label(details.feed.level),
            false,
            false,
            move |window, cx| {
                level_root.update(cx, |view, cx| {
                    view.service_details.cycle_level(&level_id);
                    cx.notify();
                });
                window.refresh();
                cx.refresh_windows();
            },
            |_, _, _| {},
        ))
        .child(elements::pill(
            theme,
            "service-logs-time",
            log_time_label(details.feed.time),
            false,
            false,
            move |window, cx| {
                time_root.update(cx, |view, cx| {
                    view.service_details.cycle_time(&time_id);
                    cx.notify();
                });
                window.refresh();
                cx.refresh_windows();
            },
            |_, _, _| {},
        ))
        .child(elements::pill(
            theme,
            "service-logs-export",
            i18n::t("common.export"),
            false,
            false,
            move |window, cx| {
                export_root.update(cx, |view, cx| {
                    view.export_service_details_logs(&export_id, &export_name, export_now_micros);
                    cx.notify();
                });
                window.refresh();
                cx.refresh_windows();
            },
            |_, _, _| {},
        ));
    let dependencies = details
        .dependencies
        .projected()
        .cloned()
        .unwrap_or_default();
    let dependencies_loading = details.dependencies.is_loading();
    div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .w(px(430.0))
        .child(prop_row(theme, i18n::t("common.name"), item.name.clone()))
        .child(prop_row(
            theme,
            i18n::t("common.description"),
            item.description.clone(),
        ))
        .child(prop_row(
            theme,
            i18n::t("common.status"),
            item.status.to_string(),
        ))
        .child(prop_row(
            theme,
            i18n::t("svc.load_state"),
            item.load_state.clone(),
        ))
        .child(prop_row(
            theme,
            i18n::t("svc.active_state"),
            item.active_state.clone(),
        ))
        .child(prop_row(
            theme,
            i18n::t("svc.sub_state"),
            item.sub_state.clone(),
        ))
        .child(prop_row(
            theme,
            i18n::t("svc.requires"),
            format_dependencies(&dependencies, &ServiceRelationKind::Requires),
        ))
        .child(prop_row(
            theme,
            i18n::t("svc.wants"),
            format_dependencies(&dependencies, &ServiceRelationKind::Wants),
        ))
        .child(prop_row(
            theme,
            i18n::t("svc.wanted_by"),
            format_dependencies(&dependencies, &ServiceRelationKind::WantedBy),
        ))
        .child(prop_row(
            theme,
            i18n::t("svc.after"),
            format_dependencies(&dependencies, &ServiceRelationKind::After),
        ))
        .when(dependencies_loading, |column| {
            column.child(
                div()
                    .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                    .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                    .child(i18n::t("svc.details_loading")),
            )
        })
        .child(
            div()
                .mt(taskmanager_ui::theme_binding::length(tokens::SPACE_8))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                        .font_weight(taskmanager_ui::theme_binding::font_weight(
                            tokens::FONT_WEIGHT_HEADER,
                        ))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                        .child(i18n::t("svc.logs")),
                )
                .child(actions),
        )
        .child(render_service_log_section(theme, &logs))
        .when_some(details.copy_feedback, |column, feedback| {
            let (text, color) = match feedback {
                ServiceLogCopyFeedback::Copied => (i18n::t("hint.copied"), theme.disk),
                ServiceLogCopyFeedback::NoData => (i18n::t("svc.logs_nothing_to_copy"), theme.gpu),
            };
            column.child(
                div()
                    .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                    .text_color(taskmanager_ui::theme_binding::hsla(color))
                    .child(text),
            )
        })
}

pub fn render_service_log_section(theme: &Theme, state: &ServiceLogState) -> Div {
    let panel = CardSurface::new(theme.palette())
        .padding(tokens::SPACE_8)
        .radius(tokens::control_radius(theme))
        .background(theme.sidebar_card_bg)
        .render()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_3,
        ))
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
        .font(mono_font_with_fallback(theme))
        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg));
    match state {
        ServiceLogState::Ready(lines) => panel.child(
            div()
                .flex()
                .flex_col()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_2,
                ))
                .children(
                    lines
                        .iter()
                        .cloned()
                        .map(|line| div().min_w(px(0.0)).whitespace_normal().child(line)),
                ),
        ),
        ServiceLogState::Loading => panel
            .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
            .child(i18n::t("svc.logs_loading")),
        ServiceLogState::Empty => panel
            .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
            .child(i18n::t("svc.logs_empty")),
        ServiceLogState::Unavailable(failure) => match failure.kind {
            ServiceLogErrorKind::TimedOut => panel
                .text_color(taskmanager_ui::theme_binding::hsla(theme.gpu))
                .child(i18n::t("svc.logs_timeout")),
            ServiceLogErrorKind::PermissionDenied => panel
                .text_color(taskmanager_ui::theme_binding::hsla(theme.gpu))
                .child(with_diagnostic(
                    i18n::t("svc.logs_permission_denied"),
                    failure.detail.as_deref(),
                )),
            ServiceLogErrorKind::MissingTool | ServiceLogErrorKind::Unsupported => panel
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(with_diagnostic(
                    i18n::t("svc.logs_unsupported"),
                    failure.detail.as_deref(),
                )),
            ServiceLogErrorKind::TemporarilyUnavailable | ServiceLogErrorKind::ProviderFailed => {
                panel
                    .text_color(taskmanager_ui::theme_binding::hsla(theme.gpu))
                    .child(with_diagnostic(
                        i18n::t("svc.logs_failed"),
                        failure.detail.as_deref(),
                    ))
            }
        },
    }
}

fn with_diagnostic(summary: &str, detail: Option<&str>) -> String {
    detail
        .filter(|detail| !detail.trim().is_empty())
        .map_or_else(
            || summary.to_string(),
            |detail| format!("{summary}: {detail}"),
        )
}
