//! Body column cells and the per-row context menu for the process table.

use super::{VisibleRow, default_width};
use gpui::{
    AnyElement, App, Div, ElementId, Entity, IntoElement, ParentElement, Pixels, Stateful, Styled,
    div, px,
};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use taskmanager_shell::SortCol;

use taskmanager_ui::overlays::context_menu::ContextMenuExt;
use taskmanager_ui::overlays::popup::PopupMenuState;

use crate::gpui_app::elements;
use crate::gpui_app::root::{self, RootView};
use crate::gpui_app::theme::mono_font_with_fallback;
use taskmanager_application::i18n;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::ProcessStatusFilter;
use taskmanager_theme::tokens::UiSize;
use taskmanager_theme::{Color, Theme};

use taskmanager_theme::tokens;

fn numeric_cell(theme: &Theme, text: String, color: Color, ui_size: UiSize) -> Div {
    div()
        .flex()
        .flex_row()
        .justify_end()
        .text_size(taskmanager_ui::theme_binding::absolute(
            ui_size.body_font_size(),
        ))
        .font(mono_font_with_fallback(theme))
        .text_color(taskmanager_ui::theme_binding::hsla(color))
        .child(text)
}

/// Apply the optional Apps-page zero-value policy to a metric's semantic color.
/// Missing values are handled by the caller and never reach this helper, so a
/// dash for unavailable data cannot be mistaken for a measured zero.
fn zero_value_color(base: Color, muted: Color, enabled: bool, is_zero: bool) -> Color {
    if enabled && is_zero { muted } else { base }
}

/// Status-dot color by bucket: Running=success (green), Stopped=warning
/// (amber), Zombie=danger (red), Sleeping/Other=fg_dim (quiet). Decorative
/// fill, so the 4.5:1 text-contrast gate does not bind it; the three semantic
/// colors are already used as text/bg elsewhere (alert_ui / system_health).
fn status_dot(theme: &Theme, status: &str) -> Div {
    let color = match ProcessStatusFilter::classify(status) {
        ProcessStatusFilter::Running => theme.success,
        ProcessStatusFilter::Stopped => theme.warning,
        ProcessStatusFilter::Zombie => theme.danger,
        _ => theme.fg_dim,
    };
    div()
        .w(px(8.0))
        .h(px(8.0))
        .flex_shrink_0()
        .rounded_full()
        .bg(taskmanager_ui::theme_binding::fill(color))
}

/// Localized status label by bucket. The `Other` bucket has no stable
/// translation, so it surfaces the raw data-layer string (untranslatable
/// unknown) — hence `String`, not `&'static str`.
pub(super) fn status_label(status: &str) -> String {
    match ProcessStatusFilter::classify(status) {
        ProcessStatusFilter::Running => i18n::t("proc.status_running").to_string(),
        ProcessStatusFilter::Sleeping => i18n::t("proc.status_sleeping").to_string(),
        ProcessStatusFilter::Stopped => i18n::t("proc.status_stopped").to_string(),
        ProcessStatusFilter::Zombie => i18n::t("proc.status_zombie").to_string(),
        ProcessStatusFilter::Other | ProcessStatusFilter::All => status.to_string(),
    }
}

/// Resolve a column's live body-cell width: the user's resized override if
/// present, else its hardcoded default. Mirrors the header's width resolution
/// (`chrome::sort_header_row`) so header + body stay pixel-aligned after a drag.
fn live_width(widths: &HashMap<SortCol, Pixels>, col: SortCol) -> Pixels {
    widths
        .get(&col)
        .copied()
        .unwrap_or_else(|| default_width(col))
}

/// Build every body cell after the Name column plus the trailing context menu.
/// The Name cell is assembled by the caller so the row's keyboard/pointer
/// handlers stay in `proc_row`; this function owns the data-column rendering.
pub(super) struct AppendBodyCellsProps<'a> {
    pub theme: &'a Theme,
    pub row: &'a VisibleRow,
    pub row_idx: usize,
    pub identity: Option<ProcessLiveKey>,
    pub hidden_cols: &'a HashSet<SortCol>,
    pub col_widths: &'a HashMap<SortCol, Pixels>,
    pub entity: &'a Entity<RootView>,
    pub gray_zero_values: bool,
    pub ui_size: UiSize,
    pub graph_cache: crate::gpui_app::graph::GraphCacheHandle,
}

pub(super) fn append_body_cells(
    line: Stateful<Div>,
    props: AppendBodyCellsProps<'_>,
) -> AnyElement {
    let AppendBodyCellsProps {
        theme,
        row,
        row_idx,
        identity,
        hidden_cols,
        col_widths,
        entity,
        gray_zero_values,
        ui_size,
        graph_cache,
    } = props;
    let mut line = line;
    if !hidden_cols.contains(&SortCol::User) {
        line = line.child(
            div()
                .w(live_width(col_widths, SortCol::User))
                .pl(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .pr(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .min_w(px(0.0))
                .truncate()
                .text_size(taskmanager_ui::theme_binding::absolute(
                    ui_size.body_font_size(),
                ))
                // Identity tier (Name / User / PID): primary foreground, not the
                // dim secondary-counter color, so identity reads above counts.
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(row.cell_text.user.clone()),
        );
    }
    if !hidden_cols.contains(&SortCol::Pid) {
        line = line.child(
            numeric_cell(theme, row.cell_text.pid.clone(), theme.fg, ui_size)
                .w(live_width(col_widths, SortCol::Pid))
                .pl(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .pr(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                )),
        );
    }
    if !hidden_cols.contains(&SortCol::Threads) {
        line = line.child(
            numeric_cell(
                theme,
                row.cell_text.threads.clone(),
                zero_value_color(
                    theme.fg_dim,
                    theme.fg_dim,
                    gray_zero_values,
                    row.threads == Some(0),
                ),
                ui_size,
            )
            .w(live_width(col_widths, SortCol::Threads))
            .pl(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            ))
            .pr(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            )),
        );
    }
    if !hidden_cols.contains(&SortCol::StartTime) {
        line = line.child(
            div()
                .w(live_width(col_widths, SortCol::StartTime))
                .pl(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .pr(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .min_w(px(0.0))
                .truncate()
                .text_size(taskmanager_ui::theme_binding::absolute(
                    ui_size.body_font_size(),
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(row.cell_text.start_time.clone()),
        );
    }
    if !hidden_cols.contains(&SortCol::State) {
        line = line.child(
            div()
                .w(live_width(col_widths, SortCol::State))
                .pl(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .pr(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .min_w(px(0.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_4,
                ))
                .text_size(taskmanager_ui::theme_binding::absolute(
                    ui_size.body_font_size(),
                ))
                // A colored status dot gives the only categorical column a
                // visual identity separate from the adjacent numeric columns
                // (the core "muddle" fix); the label is lifted to fg so state
                // reads as primary info next to the dim counts. The .truncate()
                // lives on the INNER text div so the dot is never clipped.
                .child(status_dot(theme, &row.status))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                        .child(row.cell_text.status_label.clone()),
                ),
        );
    }
    // CPU cell + the per-row sparkline are gated TOGETHER: the sparkline visualizes
    // cpu_history, so it disappears when CPU is hidden. Shown on real process rows
    // (including tree parents); aggregate rows carry no single history
    // (cpu_history empty) → a blank cell of the matching width keeps columns aligned.
    if !hidden_cols.contains(&SortCol::Cpu) {
        line = line
            .child(
                numeric_cell(
                    theme,
                    row.cell_text.cpu.clone(),
                    zero_value_color(
                        theme.cpu,
                        theme.fg_dim,
                        gray_zero_values,
                        row.cpu == Some(0.0),
                    ),
                    ui_size,
                )
                .w(live_width(col_widths, SortCol::Cpu))
                .pl(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .pr(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                )),
            )
            .child({
                if row.process_identity.is_some() {
                    div()
                        .w(px(56.0))
                        .flex()
                        .flex_row()
                        .items_center()
                        .child(elements::sparkline(
                            Rc::clone(&row.cpu_history),
                            taskmanager_ui::theme_binding::rgba(theme.cpu),
                            48.0,
                            16.0,
                            graph_cache.clone(),
                        ))
                } else {
                    div().w(px(56.0))
                }
            });
    }
    if !hidden_cols.contains(&SortCol::Memory) {
        line = line.child(
            numeric_cell(
                theme,
                row.cell_text.memory.clone(),
                // Memory is a headline metric (peer of CPU/Disk): category color,
                // not the dim counter gray. Matches CPU→theme.cpu / Disk→theme.disk.
                zero_value_color(
                    theme.memory,
                    theme.fg_dim,
                    gray_zero_values,
                    row.mem == Some(0),
                ),
                ui_size,
            )
            .w(live_width(col_widths, SortCol::Memory))
            .pl(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            ))
            .pr(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            )),
        );
    }
    if !hidden_cols.contains(&SortCol::Swap) {
        line = line.child(
            numeric_cell(
                theme,
                row.cell_text.swap.clone(),
                // Swap has its own typed source and deliberately does not use
                // the Memory/PSS category so the UI cannot imply it is part of
                // application memory.
                zero_value_color(
                    theme.fg_dim,
                    theme.fg_dim,
                    gray_zero_values,
                    row.swap == Some(0),
                ),
                ui_size,
            )
            .w(live_width(col_widths, SortCol::Swap))
            .pl(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            ))
            .pr(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            )),
        );
    }
    if !hidden_cols.contains(&SortCol::DiskRead) {
        line = line.child(
            numeric_cell(
                theme,
                row.cell_text.disk_read.clone(),
                zero_value_color(
                    theme.disk,
                    theme.fg_dim,
                    gray_zero_values,
                    row.disk_read == Some(0),
                ),
                ui_size,
            )
            .w(live_width(col_widths, SortCol::DiskRead))
            .pl(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            ))
            .pr(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            )),
        );
    }
    if !hidden_cols.contains(&SortCol::DiskWrite) {
        line = line.child(
            numeric_cell(
                theme,
                row.cell_text.disk_write.clone(),
                zero_value_color(
                    theme.disk,
                    theme.fg_dim,
                    gray_zero_values,
                    row.disk_write == Some(0),
                ),
                ui_size,
            )
            .w(live_width(col_widths, SortCol::DiskWrite))
            .pl(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            ))
            .pr(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            )),
        );
    }
    if !hidden_cols.contains(&SortCol::CpuTime) {
        line = line.child(
            numeric_cell(
                theme,
                row.cell_text.cpu_time.clone(),
                zero_value_color(
                    theme.fg_dim,
                    theme.fg_dim,
                    gray_zero_values,
                    row.cpu_time_secs == Some(0),
                ),
                ui_size,
            )
            .w(live_width(col_widths, SortCol::CpuTime))
            .pl(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            ))
            .pr(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            )),
        );
    }
    // FDs column: open file-descriptor count. `None` renders as "—" (the no-data
    // convention shared with Threads). On aggregate rows `row.fds` is the SUM
    // across the category/application root's available members.
    if !hidden_cols.contains(&SortCol::Fds) {
        line = line.child(
            numeric_cell(
                theme,
                row.cell_text.fds.clone(),
                zero_value_color(
                    theme.fg_dim,
                    theme.fg_dim,
                    gray_zero_values,
                    row.fds == Some(0),
                ),
                ui_size,
            )
            .w(live_width(col_widths, SortCol::Fds))
            .pl(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            ))
            .pr(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            )),
        );
    }
    // Nice column: scheduling priority, signed. `format_nice` renders "+5"/"-3"/"0"
    // (0 is a real value, NOT the "—" sentinel). `None` renders as "—".
    if !hidden_cols.contains(&SortCol::Nice) {
        line = line.child(
            numeric_cell(
                theme,
                row.cell_text.nice.clone(),
                zero_value_color(
                    theme.fg_dim,
                    theme.fg_dim,
                    gray_zero_values,
                    row.nice == Some(0),
                ),
                ui_size,
            )
            .w(live_width(col_widths, SortCol::Nice))
            .pl(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            ))
            .pr(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            )),
        );
    }
    // Right-click process menu: taskmanager-ui `ContextMenuExt` attaches the
    // menu to this row (equivalent mounting to the old gc `ContextMenuExt` —
    // the popup renders as an anchored, window-positioned child of the row, so
    // it escapes the row/table clip). MUST be the last builder step —
    // `ContextMenu<E>` forwards `Styled` + `ParentElement` but NOT
    // `InteractiveElement` (focusable / on_key_down / on_click / on_hover above
    // must run on the underlying `Stateful<Div>` first). The builder runs on
    // right-click (deferred to next frame by the framework). Every action
    // closure captures THIS row's pid; RootView keeps no stale menu-target
    // cache after the popup closes.
    let Some(identity) = identity else {
        // Aggregate rows intentionally have no process identity. In
        // particular, do not attach a context menu carrying a representative
        // PID: right-click actions must never target an invisible member.
        return line.into_any_element();
    };
    let palette = theme.palette();
    // The menu host id must be unique per row: gpui keys element state
    // (focus/hitboxes) by the global id, and `uniform_list` rows that share a
    // constant id collide — the rows then lose their state and stop rendering.
    let menu_id = ElementId::Name(format!("proc-row-menu:{row_idx}").into());
    line.context_menu(menu_id, palette, {
        let ent = entity.clone();
        move |_state: PopupMenuState, cx: &mut App| {
            // Collapse the shell-owned selection onto this row for visual
            // sync; the menu actions themselves already carry `pid`.
            ent.update(cx, |v, cx| {
                v.select_process_single(identity);
                cx.notify();
            });
            PopupMenuState::new(root::build_proc_menu(ent.clone(), identity), cx)
        }
    })
    .into_any_element()
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_processes_view_rows_cells_tests.rs"]
mod tests;
