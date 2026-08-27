//! Processes page chrome: hierarchy summary, status-filter pills, sortable column
//! header, action bar, and the column-visibility picker.

use gpui::{
    App, Div, Entity, InteractiveElement, ParentElement, Pixels, Stateful,
    StatefulInteractiveElement, Styled, div, px,
};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::gpui_app::elements::{self};
use crate::gpui_app::root::{Hover, RootView};
use crate::gpui_app::theme::tokens::{RowDensity, UiSize};
use crate::gpui_app::theme::{Color, Theme, mono_font_with_fallback, tokens};
use crate::i18n;

use super::rows::{SortCol, is_numeric, sort_col_step, sort_id, visible_sort_cols};

mod action_bar;
mod action_button;
mod hierarchy_summary;
mod overlays;
mod page_chrome;
mod page_layout;
pub(crate) mod render;
pub use status_filter::status_filter_row;
mod columns;
pub use columns::columns_dropdown;
mod resize;
use action_button::{ActionBtnProps, action_btn};
use hierarchy_summary::hierarchy_summary;
use overlays::{AffinityOverlayProps, affinity_overlay};
pub use page_layout::ProcessChromePresentation;
pub use render::{ProcessesViewProps, render_processes};
use resize::{header_col_width, mount_resize_handle};

// ── status-filter segmented control ──────────────────────────────────────────

/// Row of status-filter segments (All / Running / Sleeping / Stopped / Zombie /
/// Other) as one connected `Segmented` track. Clicking a segment writes it into
/// `RootView::processes_state.filter`, which `render_processes` applies to the
/// row-build input alongside the search query. Keyboard: Left / Right move the
/// active segment (one tab stop for the whole control).
mod status_filter;

#[cfg(test)]
#[path = "../../../tests/gui/gpui_app/processes_view/chrome_page_layout_tests.rs"]
mod page_layout_tests;

// ── column-sort header row ─────────────────────────────────────────────────────

pub struct SortHeaderRowProps<'a> {
    theme: &'a Theme,
    sort_col: SortCol,
    sort_asc: bool,
    hovered: Option<&'a Hover>,
    entity: &'a Entity<RootView>,
    /// Table row density: the header mirrors the body rows' vertical padding /
    /// line-height so header and rows stay pixel-aligned in both densities.
    density: RowDensity,
    ui_size: UiSize,
    /// Columns the user has hidden via the "Choose columns" picker. Membership =
    /// "skip this header cell". `Name` is never hidden (identity column), so its
    /// cell always renders; the per-row CPU sparkline header is tied to CPU
    /// visibility (it labels the cpu-history trend).
    hidden_cols: &'a HashSet<SortCol>,
    /// Live column widths (user overrides merged with defaults by the caller).
    /// Each header cell reads the SAME width its body cell reads, so header +
    /// body stay pixel-aligned after a drag (the pre-resize defaults match the
    /// old hardcoded `.w(px(X))` exactly — byte-identical layout until drag).
    col_widths: &'a HashMap<SortCol, Pixels>,
    /// Width of the identity column in the intrinsic process-table surface.
    name_width: Pixels,
    /// Presentation-only column cursor. It is drawn as a small accent rail so
    /// a keypress that stays inside the current band remains visible.
    column_cursor: SortCol,
}

fn sort_header_row(props: SortHeaderRowProps<'_>) -> Stateful<Div> {
    let SortHeaderRowProps {
        theme,
        sort_col,
        sort_asc,
        hovered,
        entity,
        density,
        ui_size,
        hidden_cols,
        col_widths,
        name_width,
        column_cursor,
    } = props;
    // Arrow-key header navigation operates on the rendered column projection
    // (canonical order minus hidden). Computed once per frame and shared by
    // every cell's key handler via a cloned `Rc` (the listeners are 'static).
    let visible = Rc::new(visible_sort_cols(hidden_cols));
    // Every canonical row reserves the expand-chevron gutter; the Name header
    // mirrors it so labels remain aligned with process names.
    let name_gutter = px(18.0);
    // Build incrementally so each non-Name column can be conditionally dropped via
    // a plain `if` (no FluentBuilder import needed). `.child()` returns `Div`, so
    // every branch preserves the element type and the conditional reassign compiles.
    let name_header = sort_cell(SortCellProps {
        theme,
        label: i18n::t("common.name"),
        col: SortCol::Name,
        sort_col,
        sort_asc,
        hovered,
        entity,
        visible: &visible,
    })
    .pl(name_gutter + px(tokens::SPACE_8.0));
    let name_header = name_header.w(name_width).flex_shrink_0();

    let mut h = div()
        .id("proc-sort-header")
        .flex()
        .items_center()
        .px(tokens::SPACE_8)
        .py(density.header_padding_y())
        .text_size(ui_size.header_font_size())
        .line_height(tokens::LINE_HEIGHT_HEADER)
        .relative()
        // One hairline under the header separates chrome from primary content
        // (the shared taskmanager-ui Table paints the same border_b_1 under its
        // live header); without it body row 0 sits flush against the header.
        .border_b_1()
        .border_color(theme.border)
        // Name: identity column — always visible (never in hidden_cols by
        // contract) and fixed as the leading navigation anchor.
        .child(name_header);
    if !hidden_cols.contains(&SortCol::User) {
        let w = header_col_width(col_widths, SortCol::User);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("common.user"),
                col: SortCol::User,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::User,
            w,
            theme,
            entity,
        ));
    }
    if !hidden_cols.contains(&SortCol::Pid) {
        let w = header_col_width(col_widths, SortCol::Pid);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("proc.pid"),
                col: SortCol::Pid,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::Pid,
            w,
            theme,
            entity,
        ));
    }
    if !hidden_cols.contains(&SortCol::Threads) {
        let w = header_col_width(col_widths, SortCol::Threads);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("common.threads"),
                col: SortCol::Threads,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::Threads,
            w,
            theme,
            entity,
        ));
    }
    if !hidden_cols.contains(&SortCol::StartTime) {
        let w = header_col_width(col_widths, SortCol::StartTime);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("proc.start"),
                col: SortCol::StartTime,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::StartTime,
            w,
            theme,
            entity,
        ));
    }
    if !hidden_cols.contains(&SortCol::State) {
        let w = header_col_width(col_widths, SortCol::State);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("common.status"),
                col: SortCol::State,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::State,
            w,
            theme,
            entity,
        ));
    }
    // CPU column + the adjacent sparkline trend header are gated TOGETHER: the
    // sparkline visualizes cpu_history, so it disappears with CPU. The sparkline
    // header itself is NOT sortable (a per-row visual, no underlying scalar to
    // rank), but it carries a small "Trend" label so the header visibly
    // corresponds to the sparkline column below it (the gpui-component 0.5.1
    // IconName registry has no Activity/ChartLine/LineChart/TrendingUp glyph, so
    // the sanctioned tiny-text fallback is used). The Trend header is a plain
    // div (not a `sort_cell`), so it gets NO resize handle — only the CPU
    // `sort_cell` is resizable. Same width + left-alignment as the body
    // sparkline cell so the column boundary stays pixel-aligned.
    if !hidden_cols.contains(&SortCol::Cpu) {
        let w = header_col_width(col_widths, SortCol::Cpu);
        h = h
            .child(mount_resize_handle(
                sort_cell(SortCellProps {
                    theme,
                    label: i18n::t("common.cpu"),
                    col: SortCol::Cpu,
                    sort_col,
                    sort_asc,
                    hovered,
                    entity,
                    visible: &visible,
                })
                .w(w),
                SortCol::Cpu,
                w,
                theme,
                entity,
            ))
            .child(
                div()
                    .w(px(56.0))
                    .flex()
                    .items_center()
                    // Inset the caption from the right-aligned CPU label so
                    // "CPU▼" and "Trend" don't visually touch (BorderBox keeps
                    // the outer 56px column boundary — and header/body
                    // alignment — unchanged).
                    .pl(tokens::SPACE_4)
                    .text_size(tokens::FONT_CAPTION)
                    .font_weight(tokens::FONT_WEIGHT_STRONG.into())
                    .text_color(theme.fg_dim)
                    .child(i18n::t("proc.trend")),
            );
    }
    if !hidden_cols.contains(&SortCol::Memory) {
        let w = header_col_width(col_widths, SortCol::Memory);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("common.memory"),
                col: SortCol::Memory,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::Memory,
            w,
            theme,
            entity,
        ));
    }
    if !hidden_cols.contains(&SortCol::Swap) {
        let w = header_col_width(col_widths, SortCol::Swap);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("proc.swap"),
                col: SortCol::Swap,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::Swap,
            w,
            theme,
            entity,
        ));
    }
    if !hidden_cols.contains(&SortCol::DiskRead) {
        let w = header_col_width(col_widths, SortCol::DiskRead);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("proc.disk_read"),
                col: SortCol::DiskRead,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::DiskRead,
            w,
            theme,
            entity,
        ));
    }
    if !hidden_cols.contains(&SortCol::DiskWrite) {
        let w = header_col_width(col_widths, SortCol::DiskWrite);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("proc.disk_write"),
                col: SortCol::DiskWrite,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::DiskWrite,
            w,
            theme,
            entity,
        ));
    }
    if !hidden_cols.contains(&SortCol::CpuTime) {
        let w = header_col_width(col_widths, SortCol::CpuTime);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("proc.cpu_time"),
                col: SortCol::CpuTime,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::CpuTime,
            w,
            theme,
            entity,
        ));
    }
    if !hidden_cols.contains(&SortCol::Fds) {
        let w = header_col_width(col_widths, SortCol::Fds);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("proc.fds"),
                col: SortCol::Fds,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::Fds,
            w,
            theme,
            entity,
        ));
    }
    if !hidden_cols.contains(&SortCol::Nice) {
        let w = header_col_width(col_widths, SortCol::Nice);
        h = h.child(mount_resize_handle(
            sort_cell(SortCellProps {
                theme,
                label: i18n::t("proc.nice"),
                col: SortCol::Nice,
                sort_col,
                sort_asc,
                hovered,
                entity,
                visible: &visible,
            })
            .w(w),
            SortCol::Nice,
            w,
            theme,
            entity,
        ));
    }
    if let Some(cursor_ix) = visible.iter().position(|col| *col == column_cursor) {
        let mut cursor_left = px(tokens::SPACE_8.0);
        for col in visible.iter().take(cursor_ix) {
            cursor_left += if *col == SortCol::Name {
                name_width
            } else {
                header_col_width(col_widths, *col)
                    + if *col == SortCol::Cpu {
                        px(56.0)
                    } else {
                        px(0.0)
                    }
            };
        }
        let cursor_width = if column_cursor == SortCol::Name {
            name_width
        } else {
            header_col_width(col_widths, column_cursor)
                + if column_cursor == SortCol::Cpu {
                    px(56.0)
                } else {
                    px(0.0)
                }
        };
        h = h.child(
            div()
                .absolute()
                .left(cursor_left)
                .bottom_0()
                .h(px(2.0))
                .w(cursor_width)
                .bg(theme.accent)
                .opacity(0.8),
        );
    }
    #[cfg(any(test, feature = "test-support"))]
    let h = h.debug_selector(|| "tm-proc-hdr-row".to_string());
    h
}

/// One sortable header cell. The 8 args are the straight-through render params
/// (theme/labels/state/actions); clippy's arity lint is waived like it is for
/// the other render helpers in this codebase.
struct SortCellProps<'a> {
    theme: &'a Theme,
    label: &'static str,
    col: SortCol,
    sort_col: SortCol,
    sort_asc: bool,
    hovered: Option<&'a Hover>,
    entity: &'a Entity<RootView>,
    /// The rendered column projection (canonical order minus hidden) for
    /// arrow-key navigation — same list the caller's `if` gates rendered.
    visible: &'a Rc<Vec<SortCol>>,
}

fn sort_cell(props: SortCellProps<'_>) -> Stateful<Div> {
    let SortCellProps {
        theme,
        label,
        col,
        sort_col,
        sort_asc,
        hovered,
        entity,
        visible,
    } = props;
    let active = sort_col == col;
    let id_str = sort_id(col);
    let is_hov = hovered == Some(&Hover::Static(id_str));
    let bg = if active {
        theme.accent.with_alpha(0.10)
    } else if is_hov {
        theme.accent.with_alpha(0.08)
    } else {
        Color::TRANSPARENT
    };
    let fg = if active { theme.fg } else { theme.fg_dim };
    // ▲ / ▼ only on the active column.
    let indicator = if active {
        if sort_asc { " \u{25B2}" } else { " \u{25BC}" }
    } else {
        ""
    };
    let ent_c = entity.clone();
    let ent_h = entity.clone();
    let ent_key = entity.clone();
    let visible_key = visible.clone();
    // Numeric columns render right-aligned in the monospace stack so the header
    // label lines up over the digits in the body cell (`proc_row::numeric_cell`)
    // — matches the Win11 TM / Mission Center numeric-column look. Text columns
    // stay left-aligned in the UI font.
    let numeric = is_numeric(col);
    let cell = div()
        .id(id_str)
        // Column gutter (owner-directed 2026-08-15): every header cell carries
        // the same inner padding as its body cells (`cells.rs`), so labels sit
        // over their column's content and adjacent columns read apart.
        .pl(tokens::SPACE_8)
        .pr(tokens::SPACE_8)
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(theme))
        .cursor_pointer()
        .on_click(move |_ev, _win, cx: &mut App| {
            ent_c.update(cx, |v, cx| {
                // Same-column flip / conventional initial direction — the
                // shell-owned reducer (root/shell_state.rs delegates to the
                // DirectTrackState process-viewing slot).
                v.click_process_sort(col);
                cx.notify();
            });
        })
        .on_hover(move |is_hov: &bool, _win, cx: &mut App| {
            ent_h.update(cx, |v, cx| {
                v.set_hover(
                    if *is_hov {
                        Some(Hover::Static(id_str))
                    } else {
                        None
                    },
                    cx,
                );
            });
        })
        // Zed-style header navigation: the cell is already a Tab stop
        // (`.tab_stop(true)` above); ArrowLeft / ArrowRight switch the SORT
        // column to the adjacent rendered column, wrapping at the header ends.
        // Unlike a click (`click_process_sort`), the direction is preserved —
        // only the active column changes. Hidden columns are skipped because
        // `visible_key` is the same projection the header row rendered.
        // stop_propagation keeps the gesture from also reaching the root key
        // handler (mirrors `proc_row`'s arrow handling). The handler reads the
        // LIVE sort state from RootView, so consecutive presses keep moving
        // even though focus stays on the originally-focused header cell.
        .on_key_down(move |ev, _win, cx: &mut App| {
            let key = ev.keystroke.key.as_str();
            if key != "left" && key != "right" {
                return;
            }
            let right = key == "right";
            ent_key.update(cx, |v, cx| {
                let next = sort_col_step(v.process_sort().0, right, &visible_key);
                v.move_process_sort_column(next);
                cx.notify();
            });
            cx.stop_propagation();
        })
        .flex()
        .items_center()
        .rounded(tokens::small_radius(theme))
        .bg(bg)
        .font_weight(tokens::FONT_WEIGHT_HEADER.into())
        .text_color(fg);
    let cell = if numeric {
        cell.justify_end().font(mono_font_with_fallback(theme))
    } else {
        cell
    };
    #[cfg(any(test, feature = "test-support"))]
    let cell = cell.debug_selector(move || format!("tm-proc-h-{id_str}"));
    cell.child(format!("{}{}", label, indicator))
}
