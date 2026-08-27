//! Processes screen composition and virtualized table rendering.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    Context, Div, InteractiveElement, IntoElement, IsZero, ParentElement, Pixels, ScrollHandle,
    ScrollWheelEvent, StatefulInteractiveElement, Styled, UniformListScrollHandle, Window, div, px,
    uniform_list,
};
use taskmanager_ui::inputs::text_input::TextInputState;
use taskmanager_ui::primitives::scrollbar::rail::ScrollbarRail;
use taskmanager_ui::primitives::scrollbar::{
    SCROLLBAR_HEIGHT, SCROLLBAR_WIDTH, Scrollbar, ScrollbarShow,
};

use super::page_chrome::{
    ProcessControlChromeProps, ProcessOverviewProps, process_control_chrome, process_overview,
};
use super::{AffinityOverlayProps, ProcessChromePresentation, affinity_overlay, sort_header_row};
use crate::gpui_app::processes_view::rows::{
    ProcRowProps, SortCol, VisibleRow, proc_row_with_layout, process_name_band_width,
    process_table_content_width,
};
use crate::gpui_app::root::{Hover, RootView};
use crate::gpui_app::theme::Theme;
use crate::gpui_app::theme::tokens::{RowDensity, UiSize};

use crate::i18n;
use taskmanager_shell::{ProcessRowKey, ProcessStatusFilter};

/// All straight-through processes-page render inputs (design-debt #1 props
/// consolidation); `window`/`cx` stay explicit render-lifetime handles.
pub struct ProcessesViewProps<'a> {
    pub theme: &'a Theme,
    pub application_count: usize,
    pub process_count: usize,
    pub search_input: &'a gpui::Entity<TextInputState>,
    pub rows: &'a Rc<Vec<VisibleRow>>,
    pub query: &'a str,
    pub selected: Option<u32>,
    pub selected_row: Option<ProcessRowKey>,
    pub selected_target_count: usize,
    pub selected_pids: &'a HashSet<u32>,
    pub hovered: Option<Hover>,
    pub sort_col: SortCol,
    pub sort_asc: bool,
    pub filter: ProcessStatusFilter,
    pub affinity_pid: Option<u32>,
    pub affinity_state: &'a taskmanager_application::ProcessAffinityState,
    pub affinity_cpus: &'a HashSet<u32>,
    pub affinity_hover: Option<usize>,
    pub hidden_cols: &'a HashSet<SortCol>,
    /// Confirmed host fact: the provider reported zero configured swap, so
    /// the Swap column is hidden by policy even if an old preference enabled it.
    pub swap_auto_hidden: bool,
    pub batch_history_available: bool,
    pub col_widths: &'a HashMap<SortCol, Pixels>,
    /// The actual page-content width after navigation rail and page padding.
    /// The horizontal content extent is derived from this viewport and the
    /// live column widths, so a resize never leaves a stale scroll range.
    pub viewport_width: Pixels,
    /// The virtualized process-list handle. The list and its pinned rail share
    /// this exact handle, including its bottom offset.
    pub processes_scroll: &'a UniformListScrollHandle,
    /// The horizontal column handle shared by the header and body viewports.
    pub horizontal_scroll: &'a ScrollHandle,
    /// Presentation-only active column; independent from sort and row
    /// selection.
    pub column_cursor: SortCol,
    /// Apps-page preference forwarded to body-cell rendering only; collection
    /// and row projection remain presentation-agnostic.
    pub gray_zero_values: bool,
    /// Table row density (Comfortable/Compact), snapshotted by the caller at
    /// the render entry: the header and every body row consume the same
    /// geometry so both stay pixel-aligned.
    pub density: RowDensity,
    /// Product-wide interface size; owns readable type/icon metrics and is
    /// deliberately independent from row density.
    pub ui_size: UiSize,
    /// Page-specific typed allocation derived from the frame's global layout
    /// budget. It changes placement only, never commands or process facts.
    pub presentation: ProcessChromePresentation,
}

/// GPUI 0.2.2 lets an x-scroller cross-feed a vertical wheel into its x axis
/// when the child list consumed the y delta. Keep the two axes independent:
/// real horizontal input (including platform-normalized Shift+wheel) remains
/// horizontal, while a normal vertical wheel is left to the uniform list and
/// stopped before the x parent. The platform adapters normalize Shift+wheel
/// into `delta.x`; remapping it here would happen after the child list already
/// consumed `delta.y` and would create a diagonal scroll.
fn vertical_wheel_guard() -> impl Fn(&ScrollWheelEvent, &mut Window, &mut gpui::App) {
    move |event: &ScrollWheelEvent, window, cx| {
        let delta = event.delta.pixel_delta(window.line_height());
        if !delta.x.is_zero() {
            return;
        }
        cx.stop_propagation();
    }
}

/// Wheel handling for the pinned header. The header is inside the single
/// horizontal viewport but has no vertical scroll owner of its own, so a
/// normal wheel over it is forwarded to the process list. A horizontal wheel
/// is deliberately left for the outer viewport's built-in listener.
fn header_wheel_guard(
    processes_scroll: UniformListScrollHandle,
) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut gpui::App) {
    move |event: &ScrollWheelEvent, window, cx| {
        let delta = event.delta.pixel_delta(window.line_height());
        if !delta.x.is_zero() {
            return;
        }

        if !delta.y.is_zero() {
            let handle = &processes_scroll.0.borrow().base_handle;
            let mut offset = handle.offset();
            offset.y += delta.y;
            handle.set_offset(offset);
            window.refresh();
        }
        cx.stop_propagation();
    }
}

pub fn render_processes(
    props: ProcessesViewProps<'_>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Div {
    let ProcessesViewProps {
        theme,
        application_count,
        process_count,
        search_input,
        rows,
        query,
        selected,
        selected_row,
        selected_target_count,
        selected_pids,
        hovered,
        sort_col,
        sort_asc,
        filter,
        affinity_pid,
        affinity_state,
        affinity_cpus,
        affinity_hover,
        hidden_cols,
        swap_auto_hidden,
        batch_history_available,
        col_widths,
        viewport_width,
        processes_scroll,
        horizontal_scroll,
        column_cursor,
        gray_zero_values,
        density,
        ui_size,
        presentation,
    } = props;
    let theme = *theme;
    let entity = cx.entity();
    // The row model arrives prebuilt from RootView::processes_projection (the
    // same cached projection keyboard paging consumes); the caller computes it
    // BEFORE render dispatch because an entity in the middle of rendering
    // cannot be re-updated.
    let count = rows.len();
    let affinity_hovered = hovered.clone();
    let col_widths_owned = col_widths.clone();
    let selected_pids_owned = selected_pids.clone();
    let hovered_for_rows = hovered.clone();
    let entity_for_rows = entity.clone();
    let viewport_width = viewport_width.max(px(320.0));
    let name_width = process_name_band_width(viewport_width);
    // Keep every enabled column in one intrinsic-width content surface. The
    // header and virtualized body then translate together through the shared
    // GPUI scroll state; horizontal drag is a compositor scroll, not a column
    // projection rebuild.
    // The vertical rail is a sibling of the horizontal viewport, so the
    // viewport that the table can actually occupy is narrower than the page
    // content width. Comparing against the page width made a few-pixel
    // overflow look as if no horizontal scrollbar existed at all.
    let horizontal_viewport_width = (viewport_width - px(SCROLLBAR_WIDTH)).max(px(0.0));
    let content_width =
        process_table_content_width(hidden_cols, horizontal_viewport_width, col_widths);
    let horizontal_overflow =
        content_width > horizontal_viewport_width || horizontal_scroll.max_offset().width > px(0.0);
    let render_hidden_cols = Rc::new(hidden_cols.clone());
    let horizontal_scroll = horizontal_scroll.clone();
    let processes_scroll = processes_scroll.clone();

    let header = div()
        .id("procs-header-scroll")
        .debug_selector(|| "tm-procs-header-scroll".to_string())
        .w(content_width)
        .flex_shrink_0()
        .on_scroll_wheel(header_wheel_guard(processes_scroll.clone()))
        .child(
            sort_header_row(super::SortHeaderRowProps {
                theme: &theme,
                sort_col,
                sort_asc,
                hovered: hovered.as_ref(),
                entity: &entity,
                density,
                ui_size,
                hidden_cols: &render_hidden_cols,
                col_widths,
                name_width,
                column_cursor,
            })
            .w(content_width)
            .flex_shrink_0(),
        );

    let body = if count == 0 {
        let message = if query.is_empty() {
            i18n::t("proc.no_processes").to_string()
        } else {
            format!(
                "{} \u{201C}{}\u{201D}",
                i18n::t("proc.no_processes_match"),
                query,
            )
        };
        div()
            .w(content_width)
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.fg_dim)
            .text_size(ui_size.body_font_size())
            .child(message)
            .into_any_element()
    } else {
        let rows_owned = rows.clone();
        // Row-internal keyboard navigation and shift-click range extension
        // walk PROCESS rows only (the same filtered list `move_process_page`
        // consumes). Structural aggregate rows have no process identity and
        // must not capture selection/navigation.
        let nav_pids = std::rc::Rc::new(
            rows.iter()
                .filter_map(|row| row.process_pid)
                .collect::<Vec<_>>(),
        );
        let nav_rows = std::rc::Rc::new(
            rows.iter()
                .filter_map(|row| row.selection_key)
                .collect::<Vec<_>>(),
        );
        let render_hidden_cols_owned = render_hidden_cols.clone();
        let mut process_list = uniform_list(("procs", 0_usize), count, move |range, _, _| {
            range
                .map(|index| {
                    let Some(row) = rows_owned.get(index) else {
                        return div().into_any_element();
                    };
                    let selected = row.selection_key.is_some_and(|key| {
                        selected_row == Some(key)
                            || key.process_pid().is_some_and(|pid| {
                                selected_pids_owned.contains(&pid)
                                    || (selected_pids_owned.is_empty() && selected == Some(pid))
                            })
                    });
                    proc_row_with_layout(
                        ProcRowProps {
                            theme: &theme,
                            row,
                            row_idx: index,
                            is_sel: selected,
                            is_hov: !selected
                                && row
                                    .process_pid
                                    .is_some_and(|pid| hovered_for_rows == Some(Hover::Proc(pid))),
                            entity: &entity_for_rows,
                            pids: nav_pids.clone(),
                            row_keys: nav_rows.clone(),
                            gray_zero_values,
                            density,
                            ui_size,
                        },
                        &render_hidden_cols_owned,
                        &col_widths_owned,
                        name_width,
                    )
                })
                .collect::<Vec<_>>()
        })
        .track_scroll(processes_scroll.clone())
        .w(content_width)
        .flex_1()
        .min_h(px(0.0));
        // GPUI 0.2.2 applies the same cross-axis fallback to a y-only
        // uniform_list: a pure horizontal delta becomes vertical movement
        // unless this field is set on the y owner as well as on the x owner.
        process_list.style().restrict_scroll_to_axis = Some(true);
        process_list.into_any_element()
    };

    // The vertical rail lives outside the x-scrolling viewport, so it stays
    // pinned to the window edge while the header/body translate horizontally.
    // There is exactly one horizontal scroll owner: the viewport below. The
    // header and virtualized body are siblings in one intrinsic-width content
    // surface, so GPUI cannot overwrite the shared handle's bounds/max range
    // from two independent prepaint passes.
    let table_content = div()
        .id("procs-table-content")
        .w(content_width)
        .h_full()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .child(header)
        .child(body);

    let mut horizontal_viewport = div()
        .id("procs-table-horizontal-viewport")
        .debug_selector(|| "tm-procs-table-horizontal-viewport".to_string())
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .min_w(px(0.0))
        .overflow_x_scroll()
        .track_scroll(&horizontal_scroll)
        .on_scroll_wheel(vertical_wheel_guard())
        .child(table_content);
    // The local GPUI 0.2.2 patch contains this upstream style field but does
    // not expose Zed's public fluent builder. Set the field directly so a
    // vertical wheel cannot be cross-fed into this x-only owner.
    horizontal_viewport.style().restrict_scroll_to_axis = Some(true);

    let body_frame = div()
        .id("procs-table-body-frame")
        .debug_selector(|| "tm-procs-table-body-frame".to_string())
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .min_w(px(0.0))
        .child(horizontal_viewport)
        .child(
            ScrollbarRail::vertical(
                "procs-vscrollbar",
                "tm-procs-vscrollbar",
                Rc::new(processes_scroll.clone()),
                theme.palette(),
            )
            .track_debug_selector("tm-procs-vscrollbar-track"),
        );

    let horizontal_bar = if horizontal_overflow {
        div()
            .id("procs-hscroll-track")
            .debug_selector(|| "tm-procs-hscroll-track".to_string())
            .relative()
            .w_full()
            .h(px(SCROLLBAR_HEIGHT))
            .flex_shrink_0()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(1.0))
                    .bg(theme.border.with_alpha(0.18)),
            )
            .child(
                Scrollbar::horizontal(
                    "procs-hscroll",
                    Rc::new(horizontal_scroll.clone()),
                    theme.palette(),
                )
                .show(ScrollbarShow::Always),
            )
            .into_any_element()
    } else {
        div().h(px(0.0)).into_any_element()
    };

    let table_scroll = div()
        .id("procs-table-scroll")
        .debug_selector(|| "tm-procs-table-scroll".to_string())
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .min_w(px(0.0))
        .child(body_frame)
        .child(horizontal_bar);

    let view = div()
        .flex()
        .flex_col()
        .gap(presentation.band_gap())
        .size_full()
        .child(process_overview(ProcessOverviewProps {
            theme: &theme,
            application_count,
            process_count,
            search_input,
            presentation,
            ui_size,
        }))
        .child(process_control_chrome(
            ProcessControlChromeProps {
                theme: &theme,
                selected,
                selected_pids,
                application_selected: selected_row
                    .is_some_and(|row| row.application_root().is_some()),
                selected_target_count,
                hidden_cols,
                swap_auto_hidden,
                hovered: hovered.as_ref(),
                batch_history_available,
                filter,
                entity: &entity,
                presentation,
                ui_size,
            },
            cx,
        ))
        .child(table_scroll);
    if let Some(affinity_pid) = affinity_pid {
        view.child(affinity_overlay(
            AffinityOverlayProps {
                theme: &theme,
                hovered: affinity_hovered.as_ref(),
                pid: affinity_pid,
                state: affinity_state,
                cpus: affinity_cpus,
                hover_chip: affinity_hover,
            },
            window,
            cx,
        ))
    } else {
        view
    }
}
