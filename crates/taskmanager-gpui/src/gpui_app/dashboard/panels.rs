//! Dialog content for dashboard rule, event, and saved-view management.

use super::{DashboardPanel, DashboardState, EventFilter, EventKind};
use crate::gpui_app::elements;
use crate::gpui_app::processes_view::rows::header_label;
use crate::gpui_app::root::RootView;
use gpui::{
    AnyElement, App, ClipboardItem, Context, Div, Entity, InteractiveElement, IntoElement,
    ParentElement, ScrollHandle, Styled, Window, div, px,
};
use taskmanager_application::ManagedAlertRule;
use taskmanager_application::i18n;
use taskmanager_core::core::{AlertEvent, AlertMetric, AlertSeverity};
use taskmanager_shell::ProcessStatusFilter;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::layout::{BoundedScrollRailSpec, bounded_scroll_region_with_rail};
use taskmanager_ui::primitives::card_surface::CardSurface;

use super::saved_view_transfer::{
    SavedViewTransferFeedback, export_saved_views_json, import_saved_views_json,
};

mod alerts;
use alerts::render_alert_rules;

pub struct DashboardPanelOverlayProps<'a> {
    pub theme: &'a Theme,
    pub panel: DashboardPanel,
    pub state: &'a DashboardState,
    pub events: &'a [AlertEvent],
    pub rules: &'a [ManagedAlertRule],
    pub entity: Entity<RootView>,
    pub scroll: ScrollHandle,
}

pub fn render_panel_overlay(
    props: DashboardPanelOverlayProps<'_>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let DashboardPanelOverlayProps {
        theme,
        panel,
        state,
        events,
        rules,
        entity,
        scroll,
    } = props;
    let close = entity.clone();
    let on_close = move |_window: &mut Window, cx: &mut App| {
        close.update(cx, |view, cx| {
            view.dismiss_window_surface(
                crate::gpui_app::root::WindowSurfaceKind::DashboardPanel,
                crate::gpui_app::root::WindowSurfaceDismissReason::CloseButton,
            );
            cx.notify();
        });
    };
    let (title, content) = match panel {
        DashboardPanel::AlertRules => (
            i18n::t("alerts.manage"),
            render_alert_rules(theme, rules, entity.clone()).into_any_element(),
        ),
        DashboardPanel::Events => (
            i18n::t("events.title"),
            render_events(theme, state, events, entity).into_any_element(),
        ),
        DashboardPanel::SavedViews => (
            i18n::t("saved_views.title"),
            render_saved_views(theme, state, entity).into_any_element(),
        ),
    };
    let viewport = window.viewport_size();
    let max_height = (f32::from(viewport.height) - 150.0).max(240.0);
    let dialog_width = (f32::from(viewport.width) - 80.0).clamp(320.0, 680.0);
    let content_width = (dialog_width - 50.0).max(270.0);
    let content: AnyElement = bounded_scroll_region_with_rail(
        BoundedScrollRailSpec {
            id: "dashboard-panel-scroll",
            viewport_selector: "tm-dashboard-panel-scroll",
            scrollbar_id: "dashboard-panel-scrollbar",
            scrollbar_selector: "tm-dashboard-panel-scrollbar",
            track_selector: "tm-dashboard-panel-scrollbar-track",
            width: Some(px(content_width)),
            max_height: px(max_height),
            scroll,
            palette: theme.palette(),
        },
        content,
    )
    .into_any_element();
    elements::dialog_overlay_width(
        theme,
        window,
        cx,
        px(dialog_width),
        title,
        on_close,
        content,
    )
    .into_any_element()
}

fn render_events(
    theme: &Theme,
    state: &DashboardState,
    events: &[AlertEvent],
    entity: Entity<RootView>,
) -> Div {
    let mut filters = div().flex().flex_row().flex_wrap().gap(tokens::SPACE_4);
    for filter in [EventFilter::All, EventFilter::Active, EventFilter::Cleared] {
        let entity = entity.clone();
        filters = filters.child(elements::pill(
            theme,
            event_filter_id(filter),
            event_filter_label(filter),
            filter == state.events.filter,
            false,
            move |_window, cx| {
                entity.update(cx, |view, cx| {
                    view.dashboard.events.filter = filter;
                    cx.notify();
                });
            },
            |_, _, _| {},
        ));
    }
    let visible = state.events.visible_events(events);
    let mut events = div().flex().flex_col().gap(tokens::SPACE_6);
    if visible.is_empty() {
        events = events.child(
            div()
                .py(tokens::SPACE_24)
                .text_color(theme.fg_dim)
                .text_size(tokens::FONT_12)
                .child(i18n::t("events.empty")),
        );
    }
    for event in visible {
        let color = match event.alert.severity {
            AlertSeverity::Info => theme.accent,
            AlertSeverity::Warning => theme.gpu,
            AlertSeverity::Critical => theme.danger,
        };
        events = events.child(
            CardSurface::new(theme.palette())
                .background(theme.sidebar_card_bg)
                .padding(tokens::SPACE_8)
                .radius(tokens::card_radius(theme))
                .bordered(false)
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .gap(tokens::SPACE_8)
                        .child(
                            div()
                                .font_weight(tokens::FONT_WEIGHT_SEMIBOLD.into())
                                .child(format!(
                                    "{} · {}",
                                    event_kind_label(event.kind),
                                    metric_label(event.alert.metric)
                                )),
                        )
                        .child(
                            div()
                                .text_size(tokens::FONT_11)
                                .text_color(theme.fg_dim)
                                .child(format!("{} ms", event.observed_at_ms)),
                        ),
                )
                .child(
                    div()
                        .mt(tokens::SPACE_3)
                        .text_size(tokens::FONT_12)
                        .text_color(theme.fg_dim)
                        .child(format!(
                            "{} · {:.1} / {:.1}",
                            event.alert.target, event.alert.value, event.alert.threshold
                        )),
                )
                .render()
                .border_l_2()
                .border_color(color),
        );
    }
    let mark = entity.clone();
    let clear = entity;
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_10)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .justify_between()
                .gap(tokens::SPACE_6)
                .child(filters)
                .child(
                    div()
                        .flex()
                        .gap(tokens::SPACE_4)
                        .child(elements::pill(
                            theme,
                            "events-mark-read",
                            i18n::t("events.mark_all_read"),
                            false,
                            false,
                            move |_window, cx| {
                                mark.update(cx, |view, cx| {
                                    view.dashboard.events.mark_all_read(
                                        view.shell.projection().alert_center.event_history(),
                                    );
                                    cx.notify();
                                });
                            },
                            |_, _, _| {},
                        ))
                        .child(elements::pill(
                            theme,
                            "events-clear",
                            i18n::t("common.clear"),
                            false,
                            false,
                            move |_window, cx| {
                                clear.update(cx, |view, cx| {
                                    view.dashboard.events.clear();
                                    view.shell.clear_alert_event_history();
                                    cx.notify();
                                });
                            },
                            |_, _, _| {},
                        )),
                ),
        )
        .child(events)
}

fn render_saved_views(theme: &Theme, state: &DashboardState, entity: Entity<RootView>) -> Div {
    let mut rows = div().flex().flex_col().gap(tokens::SPACE_7);
    for preset in &state.saved_views {
        let apply = entity.clone();
        let remove = entity.clone();
        let preset_for_apply = preset.clone();
        let preset_id = preset.id;
        let mut actions = div().flex().gap(tokens::SPACE_4).child(elements::pill(
            theme,
            ("saved-view-apply", preset_id),
            i18n::t("common.apply"),
            false,
            false,
            move |_window, cx| {
                apply.update(cx, |view, cx| {
                    view.apply_saved_view(&preset_for_apply);
                    cx.notify();
                });
            },
            |_, _, _| {},
        ));
        if !preset.built_in {
            actions = actions.child(elements::pill(
                theme,
                ("saved-view-remove", preset_id),
                i18n::t("common.remove"),
                false,
                false,
                move |_window, cx| {
                    remove.update(cx, |view, cx| {
                        view.dashboard
                            .saved_views
                            .retain(|saved| saved.id != preset_id);
                        cx.notify();
                    });
                },
                |_, _, _| {},
            ));
        }
        rows = rows.child(
            CardSurface::new(theme.palette())
                .background(theme.sidebar_card_bg)
                .padding(tokens::SPACE_8)
                .radius(tokens::card_radius(theme))
                .bordered(false)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap(tokens::SPACE_8)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(190.0))
                                .child(
                                    div()
                                        .font_weight(tokens::FONT_WEIGHT_SEMIBOLD.into())
                                        .child(preset.display_name()),
                                )
                                .child(
                                    div()
                                        .text_size(tokens::FONT_11)
                                        .text_color(theme.fg_dim)
                                        .child(format!(
                                            "{} · {} · {} {}",
                                            process_hierarchy_label(),
                                            status_filter_label(preset.filter),
                                            header_label(preset.sort_col),
                                            if preset.sort_asc { "↑" } else { "↓" }
                                        )),
                                ),
                        )
                        .child(actions),
                )
                .render(),
        );
    }
    let save = entity.clone();
    let import = entity.clone();
    let export = entity;
    let mut content = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_10)
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(i18n::t("saved_views.help")),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(tokens::SPACE_6)
                .child(elements::pill(
                    theme,
                    "saved-view-save-current",
                    i18n::t("saved_views.save_current"),
                    false,
                    false,
                    move |_window, cx| {
                        save.update(cx, |view, cx| {
                            let (sort_col, sort_dir) = view.process_sort();
                            view.dashboard.save_current_view(
                                view.process_status_filter(),
                                sort_col,
                                matches!(sort_dir, taskmanager_shell::SortDir::Asc),
                                view.processes_state.hidden_cols.clone(),
                            );
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                ))
                .child(elements::pill(
                    theme,
                    "saved-view-import",
                    i18n::t("common.import"),
                    false,
                    false,
                    move |_window, cx| {
                        let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
                        import.update(cx, |view, cx| {
                            view.dashboard.saved_view_transfer_feedback = Some(match clipboard {
                                Some(ref json) if !json.trim().is_empty() => {
                                    match import_saved_views_json(&mut view.dashboard, json) {
                                        Ok(summary) => SavedViewTransferFeedback::Imported(summary),
                                        Err(_) => SavedViewTransferFeedback::ImportInvalid,
                                    }
                                }
                                _ => SavedViewTransferFeedback::ClipboardEmpty,
                            });
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                ))
                .child(elements::pill(
                    theme,
                    "saved-view-export",
                    i18n::t("common.export"),
                    false,
                    false,
                    move |_window, cx| {
                        export.update(cx, |view, cx| {
                            view.dashboard.saved_view_transfer_feedback =
                                Some(match export_saved_views_json(&view.dashboard) {
                                    Ok(json) => {
                                        cx.write_to_clipboard(ClipboardItem::new_string(json));
                                        SavedViewTransferFeedback::ExportCopied
                                    }
                                    Err(_) => SavedViewTransferFeedback::ExportFailed,
                                });
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                )),
        );
    if let Some(feedback) = state.saved_view_transfer_feedback {
        content = content.child(
            div()
                .id("saved-view-transfer-feedback")
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(saved_view_transfer_feedback(feedback)),
        );
    }
    content.child(rows)
}

fn saved_view_transfer_feedback(feedback: SavedViewTransferFeedback) -> String {
    match feedback {
        SavedViewTransferFeedback::ExportCopied => i18n::t("hint.copied").to_string(),
        SavedViewTransferFeedback::ExportFailed => i18n::t("saved_views.export_failed").to_string(),
        SavedViewTransferFeedback::Imported(summary) => i18n::t("saved_views.import_success")
            .replace("{count}", &summary.imported.to_string())
            .replace("{renamed}", &summary.renamed.to_string()),
        SavedViewTransferFeedback::ClipboardEmpty => {
            i18n::t("saved_views.clipboard_empty").to_string()
        }
        SavedViewTransferFeedback::ImportInvalid => {
            i18n::t("saved_views.import_invalid").to_string()
        }
    }
}

fn metric_label(metric: AlertMetric) -> &'static str {
    match metric {
        AlertMetric::CpuUsagePercent => i18n::t("common.cpu"),
        AlertMetric::MemoryUsagePercent => i18n::t("common.memory"),
        AlertMetric::DiskTemperatureC => i18n::t("alert.disk_temperature"),
        AlertMetric::SmartPercentUsed => i18n::t("alert.smart_wear"),
        AlertMetric::SmartCriticalWarning => i18n::t("alert.smart_critical"),
    }
}

pub(super) fn severity_label(severity: AlertSeverity) -> &'static str {
    match severity {
        AlertSeverity::Info => i18n::t("alert.info"),
        AlertSeverity::Warning => i18n::t("alert.warning"),
        AlertSeverity::Critical => i18n::t("alert.critical"),
    }
}

fn event_filter_id(filter: EventFilter) -> &'static str {
    match filter {
        EventFilter::All => "event-filter-all",
        EventFilter::Active => "event-filter-active",
        EventFilter::Cleared => "event-filter-cleared",
    }
}

fn event_filter_label(filter: EventFilter) -> &'static str {
    match filter {
        EventFilter::All => i18n::t("common.all"),
        EventFilter::Active => i18n::t("events.active"),
        EventFilter::Cleared => i18n::t("events.cleared"),
    }
}

fn event_kind_label(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Activated => i18n::t("events.activated"),
        EventKind::Cleared => i18n::t("events.cleared"),
    }
}

fn process_hierarchy_label() -> &'static str {
    i18n::t("proc.mode_category_tree")
}

fn status_filter_label(filter: ProcessStatusFilter) -> &'static str {
    match filter {
        ProcessStatusFilter::All => i18n::t("common.all"),
        ProcessStatusFilter::Running => i18n::t("saved_views.running"),
        ProcessStatusFilter::Sleeping => i18n::t("saved_views.sleeping"),
        ProcessStatusFilter::Stopped => i18n::t("saved_views.stopped"),
        ProcessStatusFilter::Zombie => i18n::t("saved_views.zombie"),
        ProcessStatusFilter::Other => i18n::t("saved_views.other"),
    }
}
