//! Durable per-application history page.
//!
//! The page renders the application-owned [`ApplicationHistoryProjection`].
//! It never reads the live process list and never records a frontend-local
//! sample: identity correlation, three-metric joining and lifecycle honesty
//! are shared by GPUI, Iced and TUI.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Div, Entity, InteractiveElement, IntoElement, ListSizingBehavior,
    ParentElement, Styled, UniformListScrollHandle, Window, div, px, uniform_list,
};
use taskmanager_application::{ApplicationHistoryProjection, ApplicationHistoryStatus};
use taskmanager_core::core::history::HistoryWindow;

use taskmanager_ui::data::row::DataRow;
use taskmanager_ui::layout::pinned_scroll_region;
use taskmanager_ui::primitives::segmented::{Segment, Segmented};
use taskmanager_ui::primitives::state_panel::StatePanel;
use taskmanager_ui_contract::IconId;

use crate::gpui_app::elements;
use crate::gpui_app::formatting;
use crate::gpui_app::graph::GraphCacheHandle;
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::responsive::{LayoutProfile, PageLayoutBudget};
use taskmanager_application::i18n;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_theme::tokens;
use taskmanager_theme::{Color, Theme};

const TREND_W: f32 = 160.0;
const PROCESS_PEAK_W: f32 = 104.0;
const TREND_H: f32 = 22.0;
const APP_HISTORY_ROW_HEIGHT: f32 = 38.0;
const MIN_TREND_SAMPLES: usize = 2;

pub struct AppHistoryViewProps<'a> {
    pub theme: &'a Theme,
    pub projection: ApplicationHistoryProjection,
    /// Request-keyed renderer projection. Arc→Rc conversion happens only when
    /// the accepted replay request changes, preserving graph scene identity.
    pub rows: Rc<Vec<AppHistoryRow>>,
    pub scroll: &'a UniformListScrollHandle,
    pub entity: Entity<RootView>,
    pub(crate) graph_cache: GraphCacheHandle,
    pub ui_size: taskmanager_theme::tokens::UiSize,
    pub columns: AppHistoryColumns,
    /// Presentation unit preferences for the peak-memory column.
    pub units: UnitPreferences,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppHistoryColumns {
    Essential,
    Full,
}

impl AppHistoryColumns {
    #[must_use]
    pub const fn from_page_layout(layout: PageLayoutBudget) -> Self {
        match layout.profile {
            LayoutProfile::UltraCompact => Self::Essential,
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => Self::Full,
        }
    }

    const fn shows_process_peak(self) -> bool {
        matches!(self, Self::Full)
    }
}

pub fn render_app_history(props: AppHistoryViewProps<'_>) -> Div {
    let theme = props.theme;
    let scroll = props.scroll.clone();
    let header = page_header(
        theme,
        props.projection.selected_window,
        props.projection.refreshing,
        &props.entity,
    );
    let body = match props.projection.status {
        ApplicationHistoryStatus::Ready => history_table(
            theme,
            props.rows,
            scroll.clone(),
            props.ui_size,
            props.columns,
            props.units,
            props.graph_cache,
        ),
        ApplicationHistoryStatus::Disabled => state_panel(
            theme,
            "history.application.disabled",
            i18n::t("history.application.disabled_detail").to_owned(),
            theme.fg_dim,
        ),
        ApplicationHistoryStatus::Unavailable => state_panel(
            theme,
            "history.application.unavailable",
            props.projection.unavailable_reason.map_or_else(
                || i18n::t("history.application.unavailable_detail").to_owned(),
                |reason| {
                    format!(
                        "{} ({})",
                        i18n::t("history.application.unavailable_detail"),
                        reason.stable_code()
                    )
                },
            ),
            theme.warning,
        ),
        ApplicationHistoryStatus::Connecting => state_panel(
            theme,
            "history.application.connecting",
            i18n::t("history.application.connecting_detail").to_owned(),
            theme.accent,
        ),
        ApplicationHistoryStatus::Collecting => state_panel(
            theme,
            "history.application.collecting",
            i18n::t("history.application.collecting_detail").to_owned(),
            theme.accent,
        ),
    };

    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(header)
        .child(body)
}

fn history_table(
    theme: &Theme,
    rows: Rc<Vec<AppHistoryRow>>,
    scroll: UniformListScrollHandle,
    ui_size: taskmanager_theme::tokens::UiSize,
    columns: AppHistoryColumns,
    units: UnitPreferences,
    graph_cache: GraphCacheHandle,
) -> AnyElement {
    let row_count = rows.len();
    let row_theme = *theme;
    let content = div()
        .id("app-history-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .min_w(px(0.0))
        .pr(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_16,
        ))
        .overflow_hidden()
        .child(
            div()
                .id("app-history-header")
                .debug_selector(|| "tm-app-history-header".to_string())
                .flex_shrink_0()
                .child(header_row(theme, ui_size, columns)),
        )
        .child(
            uniform_list(
                ("app-history-rows", 0_usize),
                row_count,
                move |range, _, _| {
                    range
                        .filter_map(|index| {
                            rows.get(index).map(|row| {
                                row_for_projected(
                                    &row_theme,
                                    row,
                                    index,
                                    ui_size,
                                    columns,
                                    units,
                                    graph_cache.clone(),
                                )
                            })
                        })
                        .collect::<Vec<_>>()
                },
            )
            .track_scroll(scroll.clone())
            .with_sizing_behavior(ListSizingBehavior::Auto)
            .id("app-history-list")
            .debug_selector(|| "tm-app-history-list".to_string())
            .flex_1()
            .min_h(px(0.0)),
        );
    pinned_scroll_region(
        "app-history-scroll-viewport",
        "tm-app-history-scroll-viewport",
        "app-history-scrollbar",
        Rc::new(scroll),
        theme.palette(),
        content,
    )
    .into_any_element()
}

fn page_header(
    theme: &Theme,
    selected_window: HistoryWindow,
    refreshing: bool,
    entity: &Entity<RootView>,
) -> Div {
    let mut windows = Segmented::new("app-history-window", theme.palette());
    for window in HistoryWindow::ALL {
        let click_entity = entity.clone();
        windows = windows.segment(
            Segment::new(
                history_window_id(window),
                i18n::t(history_window_key(window)),
                move |_window: &mut Window, cx: &mut App| {
                    click_entity.update(cx, |view, cx| {
                        view.set_history_replay_window(window, cx);
                    });
                },
                |_hovered, _window, _cx| {},
            )
            .active(window == selected_window),
        );
    }
    let mut title = div()
        .flex_1()
        .min_w(px(0.0))
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_18))
        .font_weight(taskmanager_ui::theme_binding::font_weight(
            tokens::FONT_WEIGHT_SEMIBOLD,
        ))
        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
        .child(i18n::t("history.application.title").to_string());
    if refreshing {
        title = title.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(i18n::t("history.application.refreshing").to_string()),
        );
    }
    div()
        .flex()
        .items_center()
        .w_full()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .debug_selector(|| "tm-app-history-page-header".to_string())
        .child(title)
        .child(windows)
}

fn history_window_key(window: HistoryWindow) -> &'static str {
    match window {
        HistoryWindow::OneHour => "perf.replay.window.1h",
        HistoryWindow::TwentyFourHours => "perf.replay.window.24h",
        HistoryWindow::SevenDays => "perf.replay.window.7d",
    }
}

fn history_window_id(window: HistoryWindow) -> &'static str {
    match window {
        HistoryWindow::OneHour => "app-history-window-1h",
        HistoryWindow::TwentyFourHours => "app-history-window-24h",
        HistoryWindow::SevenDays => "app-history-window-7d",
    }
}

fn state_panel(theme: &Theme, title: &'static str, detail: String, tone: Color) -> AnyElement {
    StatePanel::new(IconId::History, i18n::t(title), theme.palette())
        .detail(detail)
        .tone(tone)
        .render()
        .into_any_element()
}

fn header_row(
    theme: &Theme,
    ui_size: taskmanager_theme::tokens::UiSize,
    columns: AppHistoryColumns,
) -> Div {
    row_skeleton(
        theme,
        i18n::t("common.name"),
        i18n::t("history.application.peak_cpu"),
        i18n::t("history.application.peak_memory"),
        i18n::t("history.application.peak_processes"),
        i18n::t("proc.trend"),
        true,
        ui_size,
        columns,
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppHistoryRow {
    name: String,
    verified: bool,
    peak_process_count: Option<f64>,
    peak_cpu_usage: Option<f64>,
    peak_memory_bytes: Option<f64>,
    cpu_samples: Rc<[f32]>,
}

pub(crate) fn projected_app_history_rows(
    rows: &[taskmanager_application::ApplicationHistoryRow],
) -> Vec<AppHistoryRow> {
    rows.iter()
        .map(|row| AppHistoryRow {
            name: row.display_name().to_owned(),
            verified: row.identity.is_verified(),
            peak_process_count: row.peak_process_count(),
            peak_cpu_usage: row.peak_cpu_usage_pct(),
            peak_memory_bytes: row.peak_memory_bytes(),
            cpu_samples: row.cpu_usage.as_ref().map_or_else(
                || Rc::from([]),
                |series| Rc::from(series.gap_aware_samples().as_ref()),
            ),
        })
        .collect()
}

fn row_for_projected(
    theme: &Theme,
    row: &AppHistoryRow,
    row_index: usize,
    ui_size: taskmanager_theme::tokens::UiSize,
    columns: AppHistoryColumns,
    units: UnitPreferences,
    graph_cache: GraphCacheHandle,
) -> Div {
    let cpu = row
        .peak_cpu_usage
        .filter(|value| value.is_finite())
        .map_or_else(formatting::missing_value, |value| format!("{value:.1}%"));
    let memory = row
        .peak_memory_bytes
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or_else(formatting::missing_value, |value| {
            units.format_quantity(
                value.min(u64::MAX as f64) as u64,
                QuantityFamily::Memory,
                false,
            )
        });
    let count = row
        .peak_process_count
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or_else(formatting::missing_value, |value| {
            format!("{:.0}", value.round())
        });
    let mut rendered = row_skeleton_with_count(
        theme,
        &row.name,
        row.verified,
        &count,
        &cpu,
        &memory,
        trend_cell_from_samples(theme, &row.cpu_samples, ui_size, graph_cache),
        false,
        ui_size,
        columns,
    );
    if row_index % 2 == 1 {
        rendered = rendered.bg(taskmanager_ui::theme_binding::fill(theme.zebra_bg()));
    }
    rendered
        .h(px(f32::from(ui_size.body_font_size())
            .max(APP_HISTORY_ROW_HEIGHT - 22.0)
            + 22.0))
        .debug_selector(|| "tm-app-history-row".to_string())
}

fn trend_cell_from_samples(
    theme: &Theme,
    samples: &Rc<[f32]>,
    ui_size: taskmanager_theme::tokens::UiSize,
    graph_cache: GraphCacheHandle,
) -> Div {
    let cell = div().w(px(TREND_W)).min_w(px(0.0)).flex().items_center();
    if samples.iter().filter(|sample| sample.is_finite()).count() >= MIN_TREND_SAMPLES {
        cell.child(elements::sparkline(
            Rc::clone(samples),
            taskmanager_ui::theme_binding::rgba(theme.accent),
            TREND_W,
            TREND_H,
            graph_cache,
        ))
    } else {
        cell.text_size(taskmanager_ui::theme_binding::absolute(
            ui_size.caption_font_size(),
        ))
        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
        .child(formatting::missing_value())
    }
}

// The skeleton's cell set mirrors the table's visible-column contract
// one-to-one; folding cells into a struct would hide that mapping.
#[allow(clippy::too_many_arguments)]
fn row_skeleton_with_count(
    theme: &Theme,
    name: &str,
    verified: bool,
    process_count: &str,
    cpu: &str,
    memory: &str,
    trend: Div,
    is_header: bool,
    ui_size: taskmanager_theme::tokens::UiSize,
    columns: AppHistoryColumns,
) -> Div {
    let background = if is_header {
        theme.sidebar_bg
    } else {
        Color::TRANSPARENT
    };
    let foreground = if is_header { theme.fg_dim } else { theme.fg };
    let weight = if is_header {
        tokens::FONT_WEIGHT_BOLD
    } else {
        tokens::FONT_WEIGHT_NORMAL
    };
    let cell_font_size = if is_header {
        ui_size.header_font_size()
    } else {
        ui_size.body_font_size()
    };
    let mut name_cell = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .text_size(taskmanager_ui::theme_binding::absolute(if is_header {
            ui_size.header_font_size()
        } else {
            ui_size.body_font_size()
        }))
        .text_color(taskmanager_ui::theme_binding::hsla(foreground))
        .font_weight(taskmanager_ui::theme_binding::font_weight(weight))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .child(name.to_owned()),
        );
    if !is_header {
        name_cell = name_cell.child(
            div()
                .flex_shrink_0()
                .text_size(taskmanager_ui::theme_binding::absolute(
                    ui_size.caption_font_size(),
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(i18n::t(if verified {
                    "history.application.verified"
                } else {
                    "history.application.unverified"
                })),
        );
    }
    let mut row = DataRow::new(theme.palette()).background(background);
    if is_header {
        row = row.radius(tokens::control_radius(theme));
    } else {
        row = row
            .radius(taskmanager_theme::Length(0.0))
            .bottom_border(theme.border.with_alpha(0.35));
    }
    let mut row = row
        .child(name_cell)
        .child(fixed_cell(cpu, 96.0, foreground, weight, cell_font_size))
        .child(fixed_cell(
            memory,
            112.0,
            foreground,
            weight,
            cell_font_size,
        ));
    if columns.shows_process_peak() {
        row = row.child(fixed_cell(
            process_count,
            PROCESS_PEAK_W,
            foreground,
            weight,
            cell_font_size,
        ));
    }
    row.child(trend).render()
}

// Same cell-per-visible-column mapping as [`row_skeleton_with_count`].
#[allow(clippy::too_many_arguments)]
fn row_skeleton(
    theme: &Theme,
    name: &str,
    cpu: &str,
    memory: &str,
    process_count: &str,
    trend: &str,
    is_header: bool,
    ui_size: taskmanager_theme::tokens::UiSize,
    columns: AppHistoryColumns,
) -> Div {
    let weight = if is_header {
        tokens::FONT_WEIGHT_BOLD
    } else {
        tokens::FONT_WEIGHT_NORMAL
    };
    let trend_label = div()
        .w(px(TREND_W))
        .min_w(px(0.0))
        .text_size(taskmanager_ui::theme_binding::absolute(
            ui_size.header_font_size(),
        ))
        .text_color(taskmanager_ui::theme_binding::hsla(if is_header {
            theme.fg_dim
        } else {
            theme.fg
        }))
        .font_weight(taskmanager_ui::theme_binding::font_weight(weight))
        .child(trend.to_owned());
    row_skeleton_with_count(
        theme,
        name,
        true,
        process_count,
        cpu,
        memory,
        trend_label,
        is_header,
        ui_size,
        columns,
    )
}

fn fixed_cell(
    label: &str,
    width: f32,
    foreground: Color,
    weight: taskmanager_theme::Weight,
    font_size: taskmanager_theme::Length,
) -> Div {
    div()
        .w(px(width))
        .min_w(px(0.0))
        .flex_shrink_0()
        .text_size(taskmanager_ui::theme_binding::absolute(font_size))
        .text_color(taskmanager_ui::theme_binding::hsla(foreground))
        .font_weight(taskmanager_ui::theme_binding::font_weight(weight))
        .child(label.to_owned())
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_app_history_view_tests.rs"]
mod tests;
