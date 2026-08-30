//! The Applications page render path: the search field, the canonical
//! Applications / Background / Uncategorized category-tree selector and the
//! shared filtered/sorted process table. Extracted from [`super`] so the page
//! entry stays a single call from the root view. The row builders and the
//! column header projection live here too (the pure seams the headless tests
//! assert on).

use super::*;
use crate::app::ColumnWidthOverrides;
use crate::theme;
use iced::Length;
use iced::alignment::Horizontal;
use iced::widget::{Space, Stack, button, container, mouse_area, row, text};
use std::rc::Rc;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::ProcessStatusFilter;
use taskmanager_shell::{SortCol, SortDir};
use taskmanager_ui_contract::ProcessColumnSpec;

use super::process_projection::ProcessProjection;
use super::process_sparkline::{PROCESS_SPARK_HEIGHT, PROCESS_SPARK_WIDTH, ProcessCpuSparkline};

mod priority_choice;
use priority_choice::{PriorityChoice, selection_hint};
mod process_menu;

pub(crate) mod page;
mod projection;

pub(crate) mod rows;
use rows::RowRender;
use rows::project_row_element;

/// Fixed table-header extent used by the header row container. The header is
/// stacked OUTSIDE the body scrollable (sticky): this extent shapes the table
/// surface, not the body's scroll geometry — the body window uses
/// [`VirtualWindow::for_sticky_rows`], whose content starts at the first row.
pub(crate) const APPLICATION_HEADER_HEIGHT: f32 = VIRTUAL_TABLE_HEADER_HEIGHT;

/// The row extent is a renderer contract: the row builders are wrapped to this
/// height before entering the virtual list. Compact density keeps the same
/// bounded geometry with a smaller contract so more rows fit the viewport.
pub(crate) fn application_row_height(compact: bool) -> f32 {
    if compact { 24.0 } else { 32.0 }
}

/// Stable invalidation key for the lazy body, composed through the shared
/// [`lazy_key::LazyKey`] discipline (scope + theme fingerprint + page
/// fields). The projection generation covers process data and view-mode
/// changes; the remaining fields cover renderer-local row appearance and
/// interaction state. Set members are sorted before hashing so HashSet
/// iteration order cannot cause spurious rebuilds. Column-width overrides
/// participate too: a drag must rebuild the materialized rows.
pub(crate) fn applications_table_key(generation: u64, render: &RowRender) -> u64 {
    let mut selected: Vec<ProcessLiveKey> = render.selected_identities.iter().copied().collect();
    selected.sort_unstable();
    let mut hidden = render
        .hidden_columns
        .iter()
        .map(|column| column.label())
        .collect::<Vec<_>>();
    hidden.sort_unstable();
    let widths = render
        .column_widths
        .iter()
        .map(|(column, width)| (sort_col_contract_id(column), width.to_bits()))
        .collect::<Vec<_>>();
    super::lazy_key::LazyKey::new("applications-table")
        .revision(generation)
        .theme(&render.theme)
        .field(render.query.clone())
        .field(render.search_active)
        .field(render.swap_visible)
        .field(render.compact)
        .field(render.ui_size)
        .field(render.gray_zero)
        .field(render.selected_row)
        // The open context menu re-hosts one materialized row, so opening or
        // closing it must rebuild the lazy body exactly like any other visual
        // invalidation.
        .field(render.open_menu_identity)
        .field(selected)
        .field(hidden)
        .field(widths)
        .finish()
}

/// The Applications-page hierarchy label and its tree-wide actions. The label
/// is descriptive, not a selector: the category tree is the only runtime
/// projection.
pub(crate) fn process_view_selector(
    theme_snapshot: &taskmanager_theme::Theme,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let mut tabs: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = vec![
        text(t("proc.mode_category_tree"))
            .size(f32::from(taskmanager_theme::tokens::FONT_CAPTION))
            .color(theme::muted_text_color(theme_snapshot))
            .into(),
    ];
    tabs.push(focus::ghost_button(
        theme_snapshot,
        FocusTarget::ProcessTreeExpandAll,
        t("proc.expand_all"),
        Message::ExpandAllProcessTree,
    ));
    tabs.push(focus::ghost_button(
        theme_snapshot,
        FocusTarget::ProcessTreeCollapseAll,
        t("proc.collapse_all"),
        Message::CollapseAllProcessTree,
    ));
    row(tabs).spacing(4).into()
}

/// The Applications process-state segmented control. It uses the same
/// renderer-local focusable pill as the view-mode selector, while the shell
/// owns the actual filtered row projection consumed by the table and keyboard
/// paths.
pub(crate) fn process_status_filter_selector(
    theme_snapshot: &taskmanager_theme::Theme,
    selected: ProcessStatusFilter,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let tabs: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = ProcessStatusFilter::ALL
        .into_iter()
        .map(|filter| {
            focus::choice_pill(
                theme_snapshot,
                FocusTarget::ProcessStatusFilterTab(filter),
                filter.label().to_string(),
                filter == selected,
                Message::SelectProcessStatusFilter(filter),
            )
        })
        .collect();
    row![
        text(t("proc.status_filter"))
            .size(f32::from(taskmanager_theme::tokens::FONT_CAPTION))
            .color(theme::muted_text_color(theme_snapshot)),
        row(tabs).spacing(4),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center)
    .into()
}

/// The GPUI-parity empty-state message: `No processes` when the query is
/// empty, `No processes match "query"` otherwise (mirrors
/// `processes_view/chrome/render.rs`).
#[must_use]
pub(crate) fn empty_state_message(query: &str) -> String {
    let query = query.trim();
    if query.is_empty() {
        t("proc.no_processes").to_string()
    } else {
        format!("{} \u{201C}{}\u{201D}", t("proc.no_processes_match"), query)
    }
}

/// Build only the requested row range. The full projection remains available
/// for canonical keyboard order, but row widgets are now viewport-bounded.
pub(crate) fn applications_table_rows_range(
    ctx: &RowRender,
    projection: &ProcessProjection,
    start: usize,
    end: usize,
) -> Vec<Element<'static, Message, iced::Theme, iced::Renderer>> {
    let start = start.min(projection.rows().len());
    let end = end.min(projection.rows().len()).max(start);
    projection.rows()[start..end]
        .iter()
        .enumerate()
        .map(|(offset, row)| {
            let row_index = start + offset;
            let element = project_row_element(ctx, projection, row, row_index)
                .unwrap_or_else(|| text("").into());
            container(element)
                .height(Length::Fixed(application_row_height(ctx.compact)))
                .width(Length::Fill)
                .into()
        })
        .collect()
}

/// Arrow marker projected on the active sort column: `▲` ascending, `▼`
/// descending, `None` on every other column. A pure function over the shared
/// sort state so the renderer and the tests agree on the affordance without a
/// pixel read-back.
#[must_use]
pub(crate) fn sort_arrow(active: (SortCol, SortDir), column: SortCol) -> Option<&'static str> {
    (column == active.0).then_some(match active.1 {
        SortDir::Asc => "▲",
        SortDir::Desc => "▼",
    })
}

// ── Process-column contract kit ──────────────────────────────────────────────
//
// The Applications table consumes the toolkit-neutral
// `taskmanager_ui_contract::PROCESS_COLUMNS` inventory as its single semantic
// source: stable identity tokens, default widths, numeric alignment, and
// hideability all come from the contract; this page keeps only the projection
// (display order, Trend sparkline cell) and rendering. The kit mirrors the
// GPUI consumption pattern (`gpui_app/processes_view/rows.rs`) — semantics
// aligned, code not shared.

/// Stable token identifying a shell `SortCol` in the neutral
/// `PROCESS_COLUMNS` inventory — spelled exactly like the persisted
/// sort/hidden-columns config tokens. The match is compiler-exhaustive: a new
/// `SortCol` variant without an arm is a build error, and the contract gate
/// test in `tests/gui/ui/table_columns_tests.rs` then forces the matching
/// contract row.
pub(crate) fn sort_col_contract_id(column: SortCol) -> &'static str {
    match column {
        SortCol::Name => "Name",
        SortCol::User => "User",
        SortCol::Pid => "PID",
        SortCol::Threads => "Threads",
        SortCol::StartTime => "StartTime",
        SortCol::State => "Status",
        SortCol::Cpu => "CPU",
        SortCol::Memory => "Memory",
        SortCol::Swap => "Swap",
        SortCol::DiskRead => "DiskRead",
        SortCol::DiskWrite => "DiskWrite",
        SortCol::CpuTime => "CPUTime",
        SortCol::Fds => "FDs",
        SortCol::Nice => "Nice",
        // PSS is part of the three-frontend shell superset but not the
        // neutral inventory (GPUI does not surface it either); the token
        // keeps the arm exhaustive and never resolves to a contract row.
        SortCol::Pss => "PSS",
    }
}

/// Inverse of [`sort_col_contract_id`] for the persisted column-width tokens
/// (`Config::process_col_widths`). The spelling is exact (no fuzzy matching),
/// so an unknown token maps to `None` and the load path drops it instead of
/// fabricating state. `Name` resolves like every other token; the resizable
/// gate at the caller keeps the identity column out of the width store.
#[must_use]
pub(crate) fn sort_col_from_contract_id(token: &str) -> Option<SortCol> {
    match token {
        "Name" => Some(SortCol::Name),
        "User" => Some(SortCol::User),
        "PID" => Some(SortCol::Pid),
        "Threads" => Some(SortCol::Threads),
        "StartTime" => Some(SortCol::StartTime),
        "Status" => Some(SortCol::State),
        "CPU" => Some(SortCol::Cpu),
        "Memory" => Some(SortCol::Memory),
        "Swap" => Some(SortCol::Swap),
        "DiskRead" => Some(SortCol::DiskRead),
        "DiskWrite" => Some(SortCol::DiskWrite),
        "CPUTime" => Some(SortCol::CpuTime),
        "FDs" => Some(SortCol::Fds),
        "Nice" => Some(SortCol::Nice),
        "PSS" => Some(SortCol::Pss),
        _ => None,
    }
}

/// The neutral contract row for this column. `None` is reachable only for the
/// shell-superset `Pss` column (no contract row by design); a miss for any
/// other variant is a programming error the `debug_assert` gate and the
/// contract gate test both catch, while the accessors below keep panic-free
/// page-local fallbacks so rendering survives a release build.
fn contract_spec(column: SortCol) -> Option<&'static ProcessColumnSpec> {
    let spec = taskmanager_ui_contract::find(sort_col_contract_id(column));
    debug_assert!(
        column == SortCol::Pss || spec.is_some(),
        "SortCol {column:?} ({}) is missing from PROCESS_COLUMNS",
        sort_col_contract_id(column)
    );
    spec
}

/// Page-local default width of the shell-superset `Pss` column — the only
/// column without a contract row. Every other column's width is contract
/// truth; do not add more literals here.
const PSS_LOCAL_WIDTH: f32 = 90.0;

/// Default pixel width of this column's header AND body cell — delegated to
/// the contract row (the single toolkit-neutral table of defaults). Header
/// and body read the same accessor, which keeps their boundaries
/// pixel-aligned under the sticky header.
pub(crate) fn column_width(column: SortCol) -> f32 {
    match contract_spec(column) {
        Some(spec) => spec.default_width,
        None => PSS_LOCAL_WIDTH,
    }
}

/// Horizontal cell alignment: numeric contract columns right-align so digits
/// line up vertically tick-to-tick; text columns stay left-aligned. The
/// shell-superset `Pss` column (no contract row) still renders byte values,
/// so it keeps the numeric alignment. This is an alignment concern only,
/// independent of the shell's sort-direction classification (contract parity
/// with the GPUI accessor).
pub(crate) fn column_alignment(column: SortCol) -> Horizontal {
    match contract_spec(column) {
        Some(spec) if spec.numeric => Horizontal::Right,
        Some(_) => Horizontal::Left,
        None => Horizontal::Right,
    }
}

/// Whether the "Choose columns" picker may hide this column — delegated to
/// contract hideability (`Name` is the always-visible identity column).
pub(crate) fn column_hideable(column: SortCol) -> bool {
    contract_spec(column).is_some_and(|spec| spec.hideable)
}

/// Whether a resize edge may mount on this column's header cell. The identity
/// column (`Name`) is never resizable, mirroring the contract's identity rule
/// (never hideable, and the flexible/anchor column in the contract's model):
/// every other column is a fixed extent with a draggable trailing edge,
/// including the shell-superset `Pss`. The Trend cell is not a `SortCol` and
/// carries no edge.
pub(crate) fn column_resizable(column: SortCol) -> bool {
    column != SortCol::Name
}

/// The Applications-table columns in display order with their sort identity
/// and contract-default widths. Widths come from [`column_width`] (contract
/// truth); `State` is intentionally absent from the `s`/`S` display cycle —
/// it has a header cell here — while `Swap` is shown only when the host has a
/// swap device. Labels themselves are not duplicated here: each header reads
/// [`localized_sort_column_label`].
pub(crate) fn apps_columns(swap_visible: bool) -> Vec<(SortCol, f32)> {
    apps_columns_with(swap_visible, &ColumnWidthOverrides::default())
}

/// [`apps_columns`] with session-local width overrides applied: a column
/// reports its stored drag override when present, else its contract default.
pub(crate) fn apps_columns_with(
    swap_visible: bool,
    widths: &ColumnWidthOverrides,
) -> Vec<(SortCol, f32)> {
    fn spec(column: SortCol, widths: &ColumnWidthOverrides) -> (SortCol, f32) {
        (
            column,
            widths.get(column).unwrap_or_else(|| column_width(column)),
        )
    }
    let mut columns = vec![
        spec(SortCol::Pid, widths),
        spec(SortCol::Name, widths),
        spec(SortCol::Cpu, widths),
        spec(SortCol::Memory, widths),
        spec(SortCol::Pss, widths),
    ];
    if swap_visible {
        columns.push(spec(SortCol::Swap, widths));
    }
    // Advanced typed columns (disk rate, cumulative CPU time, thread count) sit
    // next to memory and are individually sortable via per-header click. They
    // are absent from the TUI display cycle but available here because the
    // window can scroll horizontally past the table baseline.
    columns.extend([
        spec(SortCol::DiskRead, widths),
        spec(SortCol::DiskWrite, widths),
        spec(SortCol::CpuTime, widths),
        spec(SortCol::Threads, widths),
    ]);
    columns.push(spec(SortCol::User, widths));
    // GPUI-parity advanced columns (mirrors the gpui processes_view header):
    // process state, open-fd count, nice, and the local-time start clock
    // (`HH:MM`, gpui `format_start_time` parity). Each is individually
    // sortable via per-header click (advanced columns stay out of the
    // `s`-key display cycle so the TUI is unaffected).
    columns.extend([
        spec(SortCol::State, widths),
        spec(SortCol::Fds, widths),
        spec(SortCol::Nice, widths),
        spec(SortCol::StartTime, widths),
    ]);
    columns
}

/// Apply the renderer-local column chooser to the canonical column order.
/// Name remains visible even if a stale state somehow contains it.
///
/// Test seam for the default geometry: the render path always resolves
/// through [`visible_apps_columns_with`] with the live override set, and this
/// default-only projection is what the contract-gate tests assert.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn visible_apps_columns(
    swap_visible: bool,
    hidden: &std::collections::HashSet<SortCol>,
) -> Vec<(SortCol, f32)> {
    visible_apps_columns_with(swap_visible, hidden, &ColumnWidthOverrides::default())
}

/// [`visible_apps_columns`] with session-local width overrides applied.
pub(crate) fn visible_apps_columns_with(
    swap_visible: bool,
    hidden: &std::collections::HashSet<SortCol>,
    widths: &ColumnWidthOverrides,
) -> Vec<(SortCol, f32)> {
    apps_columns_with(swap_visible, widths)
        .into_iter()
        .filter(|(column, _)| *column == SortCol::Name || !hidden.contains(column))
        .collect()
}

/// Place the non-sortable Trend cell immediately after CPU when CPU is
/// visible, otherwise after the mandatory Name cell. This keeps header and
/// body order identical while the column menu hides scalar columns.
#[must_use]
pub(crate) fn trend_header_index_for(columns: &[(SortCol, f32)]) -> usize {
    columns
        .iter()
        .position(|(column, _)| *column == SortCol::Cpu)
        .map(|index| index + 1)
        .or_else(|| {
            columns
                .iter()
                .position(|(column, _)| *column == SortCol::Name)
                .map(|index| index + 1)
        })
        .unwrap_or(0)
        .min(columns.len())
}

/// Trailing-edge hot zone of a resizable header cell, in device-independent
/// pixels. Wide enough to acquire the press reliably, narrow enough that
/// ordinary header clicks keep sorting.
const PROCESS_HEADER_RESIZE_EDGE_PX: f32 = 6.0;

/// Build one clickable header cell: the column label plus the ▲/▼ marker on the
/// active sort column, with the whole column-width surface a click target that
/// emits [`Message::SortBy`]. Inactive columns are still clickable so a click
/// switches to them. Cell width and text alignment come from the contract kit
/// ([`column_width`]/[`column_alignment`]), the same source the body cells
/// read — that shared derivation is what keeps the sticky header over its
/// column. Resizable columns additionally carry a trailing-edge drag handle
/// ([`column_resizable`]); the identity column keeps a plain sort button.
fn header_cell(
    theme_snapshot: &taskmanager_theme::Theme,
    column: SortCol,
    width: f32,
    active_sort: (SortCol, SortDir),
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let active = column == active_sort.0;
    let label: Element<'_, Message, iced::Theme, iced::Renderer> =
        match sort_arrow(active_sort, column) {
            Some(marker) => row![
                text(localized_sort_column_label(column))
                    .wrapping(iced::widget::text::Wrapping::None),
                text(marker)
                    .size(f32::from(taskmanager_theme::tokens::FONT_CAPTION))
                    .wrapping(iced::widget::text::Wrapping::None),
            ]
            .spacing(4)
            .width(Length::Shrink)
            .align_y(iced::Alignment::Center)
            .into(),
            None => text(localized_sort_column_label(column))
                .wrapping(iced::widget::text::Wrapping::None)
                .width(Length::Shrink)
                .into(),
        };
    let sort_button = button(
        container(label)
            .width(Length::Fill)
            .align_x(column_alignment(column)),
    )
    .on_press(Message::SortBy(column))
    .style(move |_theme, status| theme::header_button_style(theme_snapshot, status, active))
    .padding([2.0, 4.0])
    .width(Length::Fixed(width));
    if !column_resizable(column) {
        return sort_button.into();
    }
    // Resize edge: a `mouse_area` strip over the cell's trailing edge. The
    // strip's press is captured before the sort button underneath sees the
    // event (stack dispatches top layer first), and the live drag continues
    // through the raw pointer subscription (`app::subscription`) because a
    // `mouse_area` only reports motion while hovered — a pointer dragging
    // past the 6px edge would otherwise stall.
    let edge = mouse_area(
        Space::new()
            .width(Length::Fixed(PROCESS_HEADER_RESIZE_EDGE_PX))
            .height(Length::Fill),
    )
    .on_press(Message::BeginProcessColumnDrag {
        column,
        start_width: width,
    })
    .interaction(iced::mouse::Interaction::ResizingColumn);
    Stack::new()
        .push(sort_button)
        .push(
            container(edge)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Right),
        )
        .width(Length::Fixed(width))
        .into()
}

/// Localized Applications header labels. The shell owns sort identity and
/// ordering; the frontend owns the visible copy so a Chinese session does not
/// fall back to the shell's English diagnostic labels.
#[must_use]
pub(crate) fn localized_sort_column_label(column: SortCol) -> &'static str {
    match column {
        SortCol::Pid => t("proc.pid"),
        SortCol::Name => t("common.name"),
        SortCol::Cpu => t("common.cpu"),
        SortCol::Memory => t("common.memory"),
        SortCol::Pss => t("proc.pss"),
        SortCol::Swap => t("proc.swap"),
        SortCol::User => t("common.user"),
        SortCol::State => t("common.state"),
        SortCol::Threads => t("common.threads"),
        SortCol::CpuTime => t("proc.cpu_time"),
        SortCol::DiskRead => t("proc.disk_read"),
        SortCol::DiskWrite => t("proc.disk_write"),
        SortCol::StartTime => t("proc.start"),
        SortCol::Fds => t("proc.fds"),
        SortCol::Nice => t("proc.nice"),
    }
}

/// Intrinsic width of the Applications table content. Iced's scrollable
/// defaults to a shrink-to-fit content strategy; without an explicit width a
/// wide row is compressed into the viewport and fixed cells visually collide.
/// The value includes the Trend cell, shared gutters, and the comfortable row
/// horizontal padding so both header and body stay inside the scroll content.
///
/// Test seam for the default geometry: the render path always resolves
/// through [`apps_table_width_with`] with the live override set.
#[cfg_attr(not(test), allow(dead_code))]
#[must_use]
pub(crate) fn apps_table_width(
    swap_visible: bool,
    hidden: &std::collections::HashSet<SortCol>,
) -> f32 {
    apps_table_width_with(swap_visible, hidden, &ColumnWidthOverrides::default())
}

/// [`apps_table_width`] with session-local column width overrides applied.
/// Overrides shift the extent by exactly their delta over the contract
/// defaults, so header, body and scroll extent stay one geometry.
#[must_use]
pub(crate) fn apps_table_width_with(
    swap_visible: bool,
    hidden: &std::collections::HashSet<SortCol>,
    widths: &ColumnWidthOverrides,
) -> f32 {
    let columns = visible_apps_columns_with(swap_visible, hidden, widths);
    let cell_count = columns.len() + 1; // + Trend
    let columns_width: f32 = columns.iter().map(|(_, width)| *width).sum();
    columns_width
        + PROCESS_SPARK_WIDTH
        + theme::table_column_spacing() * (cell_count.saturating_sub(1) as f32)
        + 2.0 * f32::from(taskmanager_theme::tokens::SPACE_8)
}

/// The non-sortable Trend header cell: a plain (non-button) left-aligned muted
/// text cell of the sparkline column width. NOT a `header_cell` (no `SortCol`,
/// no click target, no sort arrow) — mirrors the gpui processes_view chrome
/// where the Trend header is a plain `div`, not a `sort_cell`.
fn trend_header_cell(
    theme_snapshot: &taskmanager_theme::Theme,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    text(t("proc.trend"))
        .size(f32::from(taskmanager_theme::tokens::FONT_CAPTION))
        .color(theme::muted_text_color(theme_snapshot))
        .width(Length::Fixed(PROCESS_SPARK_WIDTH))
        .into()
}
