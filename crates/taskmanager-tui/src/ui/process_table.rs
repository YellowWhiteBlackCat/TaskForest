//! Applications-page process table renderer: the search bar, the canonical
//! category/application/process hierarchy, the per-row CPU sparkline
//! column, and the active-sort header. Extracted from `ui.rs` so no single
//! renderer file exceeds the source line budget; behavior unchanged.

use std::collections::HashSet;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row};
use taskforest_core::core::process::ProcessLiveKey;
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use crate::TuiTheme;
use crate::process_view::ProcessRow;
use crate::{FocusPanel, TuiApp};
use taskmanager_shell::{SortCol, SortDir};

use super::containers::{WindowedTableProps, render_windowed_table};
use super::frame_plan::TablePanelProjection;
use super::panel;
use super::process_details;
use super::sparkline;
use taskmanager_shell::presentation::{process_anomaly_summary, uninterruptible_process_summary};

/// The minimum body width at which the per-row CPU sparkline column renders.
/// Below this the trend column hides (mirroring `swap_visible`'s adaptivity):
/// the base table already saturates a ~140-col terminal, and the 24-col
/// sparkline would push late columns (Start / CPU time / Disk r/w) off the
/// frame. The 54×16 minimum frame stays honest — sparkline hidden — and the
/// wide case (≥160 cols) renders the trend without truncation.
pub(super) const SPARKLINE_MIN_AREA_WIDTH: u16 = 160;

/// The outer width at which the process table widens its column gutter from
/// one blank to two. Budget: the widest projection (sparkline + swap + every
/// advanced column) is ~149 content chars; a two-blank gutter adds ~28 more,
/// so a terminal narrower than this cannot afford it and the last columns
/// would be truncated — the gutter yields to content below the threshold.
pub(super) const COLUMN_GUTTER_WIDE_WIDTH: u16 = 190;

/// The Applications page's three vertical bands. This is shared by the
/// renderer and `table_hit`, so pointer coordinates always address the same
/// table area that was painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProcessTableLayout {
    pub(super) search: Rect,
    pub(super) table: Rect,
    pub(super) details: Rect,
}

/// Resolve the Applications page bands from the already-allocated body area.
/// The details panel keeps its product height; the table receives the
/// remaining space and Ratatui clips only the bounded row window handed to it.
#[must_use]
pub(super) fn process_table_layout(area: Rect, focus: FocusPanel) -> ProcessTableLayout {
    let constraints =
        if focus == FocusPanel::Details && area.height < 3 + 5 + PROCESS_DETAILS_HEIGHT {
            let search = if area.height >= 11 { 3 } else { 0 };
            [
                Constraint::Length(search),
                Constraint::Min(3),
                Constraint::Length(
                    area.height
                        .saturating_sub(search + 3)
                        .min(PROCESS_DETAILS_HEIGHT),
                ),
            ]
        } else {
            [
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(PROCESS_DETAILS_HEIGHT),
            ]
        };
    let [search, table, details] = Layout::vertical(constraints).areas(area);
    ProcessTableLayout {
        search,
        table,
        details,
    }
}

pub(super) const PROCESS_DETAILS_HEIGHT: u16 = 18;

pub(super) fn render_processes(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    layout: ProcessTableLayout,
    table: TablePanelProjection,
    focus: super::TuiFocusPlan,
) {
    // Room for the full field set: identity + resource scalars + the cpu-time/
    // nice/disk-rate parity rows + start/executable/command.
    let ProcessTableLayout {
        search,
        table: table_area,
        details: details_area,
    } = layout;
    // The search field's caret and title paint come from the committed focus
    // plan (the field's own target), not a second search_active read; the
    // match counter below stays a data read of the live query.
    let search_focused = focus.search_field_focused();
    // A confirmed zero swap total means the host has no swap device and the
    // Swap column is intentionally hidden. An unknown total keeps the column
    // visible so an unavailable value cannot be mistaken for an absent one.
    let swap_visible = super::process_data::swap_column_visible(app.projection().snapshot.as_ref());
    // The per-row CPU sparkline column adapts to terminal width the same way
    // `swap_visible` adapts to swap presence: hidden below a comfortable
    // threshold so the 54×16 minimum frame never regresses and the wide case
    // (≥160 cols) renders the trend without pushing other columns off-screen.
    let sparkline_visible = table_area.width >= SPARKLINE_MIN_AREA_WIDTH;
    // The panel title lists the Applications-page chords; Enter expands a
    // category/application header or opens details on a process row.
    let processes = app
        .projection()
        .processes
        .as_ref()
        .map(|items| items.as_slice());
    let title_budget = table_area.width.saturating_sub(4);
    let long_title = t("tui.processes_title");
    let mut panel_title = if title_budget < 80 {
        compact_process_title(long_title)
    } else {
        long_title.to_owned()
    };
    for summary in [
        uninterruptible_process_summary(processes),
        process_anomaly_summary(processes),
    ]
    .into_iter()
    .flatten()
    {
        panel_title.push_str(" · ");
        panel_title.push_str(&summary);
    }
    // Read the whole Applications projection from the TUI's canonical-row
    // cache through the LAZY indexed accessor: the owned id slice plus an
    // on-demand index resolver, so the frame never materializes the shell's
    // O(visible N) pointer vector and a frame's cost follows the viewport,
    // not the visible N.
    app.with_canonical_rows_indexed(|ids, visible| {
        // The match counter: while a non-empty query is active, show how many
        // visible rows match and the current cursor's position among them (the
        // Enter-to-next-match walk's position). The query's own matches keep
        // the counter honest — a filter that empties the list shows 0.
        let match_counter = if app.search_active() && !app.query.trim().is_empty() {
            let count = visible.len();
            let key = if count == 1 {
                "tui.search_matches_one"
            } else {
                "tui.search_matches_many"
            };
            t(key).replacen("{count}", &count.to_string(), 1)
        } else {
            String::new()
        };
        let query_cursor = app.query_cursor();
        let (query_before, query_after) = app.query.split_at(query_cursor);
        let mut search_spans = vec![Span::raw(format!("{} ", theme.glyph(IconId::Search)))];
        search_spans.push(Span::raw(query_before));
        if search_focused {
            search_spans.push(Span::styled(
                "▌",
                Style::new().fg(theme.color(Color::White)),
            ));
        }
        search_spans.push(Span::raw(query_after));
        frame.render_widget(
            Paragraph::new(Line::from(search_spans))
                .style(if search_focused {
                    Style::new().fg(theme.color(Color::White))
                } else {
                    Style::new().fg(theme.dim)
                })
                .block(panel(
                    &format!("{}{}", t("tui.search_title"), match_counter),
                    theme,
                )),
            search,
        );
        let anchor_pid = visible
            .id_process(ids, app.selected)
            .map(|process| process.pid);
        let column_spacing = u16::from(table_area.width >= COLUMN_GUTTER_WIDE_WIDTH) + 1;
        let responsive_order = fit_process_columns(
            table_area.width,
            column_spacing,
            swap_visible,
            sparkline_visible,
            &app.hidden_columns,
            &app.column_order,
            |column| app.column_width(column),
        );
        let columns = process_details::ColumnVisibility {
            swap_visible,
            sparkline_visible,
            hidden: &app.hidden_columns,
            order: &responsive_order,
        };
        let column_visible = |column: SortCol| columns.visible(column);
        let mut widths = vec![
            Constraint::Length(7), // PID
            Constraint::Min(16),   // Name
            Constraint::Length(8), // CPU%
        ];
        if sparkline_visible {
            widths.push(Constraint::Length(sparkline::SPARKLINE_MAX_SAMPLES as u16));
        }
        // The tail widths follow the same display order and visibility gate the
        // header and rows paint, so a reordered or hidden column can never
        // desynchronize the layout.
        widths.extend(
            columns
                .order
                .iter()
                .filter(|column| column_visible(**column))
                .map(|column| Constraint::Length(app.column_width(*column))),
        );
        // Fail-closed re-bound of the plan's committed window against the
        // cached id slice (the same math the render boundary has always
        // applied), restated as the shared primitive's typed input so the
        // window is never recomputed inside the table paint.
        let start = table.window.start.min(ids.len());
        let end = table.window.end.clamp(start, ids.len());
        let selected = table
            .window
            .selected
            .min(end.saturating_sub(start).saturating_sub(1));
        let bounded = TablePanelProjection {
            area: table.area,
            total: table.total,
            window: super::TableWindow {
                start,
                end,
                selected,
            },
        };
        // The Applications empty state is a search question, not a source
        // failure: an empty query means nothing was reported, a non-empty
        // query means nothing matched it.
        let state_message = if app.query.trim().is_empty() {
            t("empty.no_processes_reported")
        } else {
            t("empty.no_processes_match_query")
        };
        // The windowed table (or the state panel on the zero-row branch)
        // paints above; the details panel rides the same lazy projection in
        // BOTH branches exactly as this renderer always drew them.
        render_windowed_table(
            frame,
            WindowedTableProps {
                theme,
                panel: bounded,
                title: &panel_title,
                header: process_header(
                    (app.effective_sort_col(), app.process_sort.1),
                    theme,
                    columns,
                ),
                widths,
                // Two blanks between columns (the ratatui default is 1) so
                // numeric readouts never read as one string — but only when
                // the terminal affords the widest projection without
                // truncating trailing columns (the 150-col budget cannot: the
                // full header projection test pins that narrow terminals keep
                // every column at the one-blank gutter).
                column_spacing,
                state_area: table_area,
                state_message,
            },
            // Materialize exactly the painted window: one O(1) id resolution
            // per visible row through the lazy accessor.
            |index| match visible.materialize_row(ids, index) {
                Some(ProcessRow::Group {
                    name: _,
                    label,
                    depth,
                    count,
                    aggregate,
                    expanded,
                    row_key: _,
                }) => {
                    let label = format!("{}{}", "  ".repeat(depth), label);
                    Row::new(process_details::group_header_cells(
                        &label,
                        count,
                        aggregate.cpu(),
                        aggregate.memory(),
                        expanded,
                        columns,
                        process_details::SearchHighlight {
                            query: &app.query,
                            search_active: app.search_active(),
                            theme,
                        },
                    ))
                    .style(Style::new().fg(theme.accent).add_modifier(Modifier::BOLD))
                }
                Some(ProcessRow::TreeNode {
                    process,
                    depth,
                    has_children,
                    collapsed,
                }) => {
                    let row = Row::new(process_details::process_cells_with_local_time(
                        process_details::ProcessCellInput {
                            process,
                            highlight: process_details::SearchHighlight {
                                query: &app.query,
                                search_active: app.search_active(),
                                theme,
                            },
                            columns,
                            tree_prefix: Some(process_details::ProcessTreePrefix {
                                depth,
                                branch: match (has_children, collapsed) {
                                    (false, _) => process_details::ProcessTreeBranch::Leaf,
                                    (true, false) => process_details::ProcessTreeBranch::Expanded,
                                    (true, true) => process_details::ProcessTreeBranch::Collapsed,
                                },
                            }),
                            gray_zero: app.prefs.gray_zero,
                            local_time_rules: &app.local_time_rules,
                        },
                    ));
                    multi_select_style(
                        row,
                        app,
                        theme,
                        ProcessLiveKey::from_process(process),
                        process.pid,
                        anchor_pid,
                    )
                }
                // The cache key pins the exact visible list the ids index, so
                // an unresolvable id cannot occur in a consistent state; the
                // honest fail-closed paint is a blank row, never a fabricated
                // one.
                None => Row::new(Vec::<Cell>::new()),
            },
        );
        process_details::render_process_details_with_focus_from_canonical_indexed(
            frame,
            app,
            theme,
            details_area,
            focus.applications_details_focused(),
            ids,
            visible,
        );
    });
}

/// Select a whole localized compact title from the existing title's complete
/// action groups. The compact hint keeps the page name and Enter action; it
/// never clips a chord or its label halfway through.
fn compact_process_title(long_title: &str) -> String {
    let mut groups = long_title.split(" · ");
    let page = groups.next().unwrap_or(long_title);
    groups.nth(3).map_or_else(
        || page.to_owned(),
        |enter_hint| format!("{page} · {enter_hint}"),
    )
}

/// Fit the user's ordered tail columns into this frame's table slot. PID,
/// Name, and CPU are the fixed prefix; each optional column is admitted as a
/// complete cell only while the next ordered column still fits. Returned
/// state is frame-local and never changes the user's hide/order preferences.
fn fit_process_columns(
    table_width: u16,
    column_spacing: u16,
    swap_visible: bool,
    sparkline_visible: bool,
    user_hidden: &HashSet<SortCol>,
    user_order: &[SortCol],
    column_width: impl Fn(SortCol) -> u16,
) -> Vec<SortCol> {
    let mut order = Vec::with_capacity(user_order.len());
    let prefix_columns = 3 + usize::from(sparkline_visible);
    let mut used_width =
        7_u16
            .saturating_add(16)
            .saturating_add(8)
            .saturating_add(if sparkline_visible {
                u16::try_from(super::sparkline::SPARKLINE_MAX_SAMPLES).unwrap_or(u16::MAX)
            } else {
                0
            })
            .saturating_add(column_spacing.saturating_mul(
                u16::try_from(prefix_columns.saturating_sub(1)).unwrap_or(u16::MAX),
            ));
    let available_width = table_width.saturating_sub(2);
    let mut accepting_tail = true;

    for &column in user_order {
        if matches!(column, SortCol::Pid | SortCol::Name | SortCol::Cpu)
            || user_hidden.contains(&column)
            || (column == SortCol::Swap && !swap_visible)
        {
            continue;
        }
        if !accepting_tail {
            continue;
        }
        let next_width = column_spacing.saturating_add(column_width(column));
        if used_width.saturating_add(next_width) <= available_width {
            used_width = used_width.saturating_add(next_width);
            order.push(column);
        } else {
            accepting_tail = false;
        }
    }
    order
}
/// Tint a row that is a member of the batch-control multi-select set (but is
/// not the keyboard anchor, which the table's own highlight already paints).
/// The muted background reads as "marked" without competing with the anchor.
///
/// `anchor_pid` is resolved ONCE by the caller from the already-built visible
/// list (`visible.get(app.selected).map(|p| p.pid)`) so this per-row check is
/// O(1) — the identity-set `contains` is HashSet O(1), and the anchor compare
/// is a pid equality — instead of recomputing `app.visible_processes()` per
/// row (which made the table O(N²) per frame: every row paid the O(N)
/// filter+sort+alloc just to read one pid).
fn multi_select_style<'a>(
    row: Row<'a>,
    app: &TuiApp,
    theme: TuiTheme,
    identity: Option<ProcessLiveKey>,
    pid: u32,
    anchor_pid: Option<u32>,
) -> Row<'a> {
    let marked =
        identity.is_some_and(|identity| app.shell.selected_identities().contains(&identity));
    if marked && Some(pid) != anchor_pid {
        row.style(
            Style::new()
                .bg(theme.highlight_bg)
                .fg(theme.color(Color::Gray)),
        )
    } else {
        row
    }
}

/// Build the process-table header, marking the active sort column with a
/// direction arrow (▲ ascending / ▼ descending) so the user can see what is
/// sorted and which way. The Trend (per-row CPU sparkline) column is
/// non-sortable — `cpu_history` is a `Vec<f32>`, not a `SortCol` key — so it
/// is spliced in as a plain label at the CPU-adjacent position only while
/// `sparkline_visible`, matching the cell order [`process_cells`] emits.
fn process_header(
    sort: (SortCol, SortDir),
    theme: TuiTheme,
    columns: process_details::ColumnVisibility,
) -> Row<'static> {
    let process_details::ColumnVisibility {
        swap_visible,
        sparkline_visible,
        hidden,
        order,
    } = columns;
    let visible = |column: SortCol| {
        process_details::ColumnVisibility {
            swap_visible,
            sparkline_visible,
            hidden,
            order,
        }
        .visible(column)
    };
    let (column, direction) = sort;
    let arrow = match direction {
        SortDir::Asc => " ▲",
        SortDir::Desc => " ▼",
    };
    let mut columns = vec![SortCol::Pid, SortCol::Name, SortCol::Cpu];
    // The Trend label splices in AFTER CPU% (index 3 of the rendered header)
    // so it lines up with the sparkline cell emitted at the same position by
    // `process_cells`. Built outside the `columns` vec because it has no
    // SortCol — the active-sort arrow never lands on it.
    let splice_trend_at = columns.len();
    // The tail headers follow the user's display order (the column menu's
    // reorder gesture), gated by the same visibility as the widths and cells.
    columns.extend(
        order
            .iter()
            .copied()
            .filter(|candidate| visible(*candidate)),
    );
    let mut cells: Vec<Cell> = columns
        .iter()
        .map(|&col| {
            let is_active = col == column;
            let text = if is_active {
                format!("{}{}", col.label(), arrow)
            } else {
                col.label().to_owned()
            };
            let style = if is_active {
                Style::new()
                    .fg(theme.color(Color::White))
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
            };
            Cell::from(text).style(style)
        })
        .collect();
    if sparkline_visible {
        cells.insert(
            splice_trend_at,
            Cell::from(t("proc.trend"))
                .style(Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)),
        );
    }
    Row::new(cells).bottom_margin(1)
}
