//! Applications-page process table renderer: the search bar, the canonical
//! category/application/process hierarchy, the per-row CPU sparkline
//! column, and the active-sort header. Extracted from `ui.rs` so no single
//! renderer file exceeds the source line budget; behavior unchanged.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Cell, HighlightSpacing, Paragraph, Row, Table, TableState};
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use crate::TuiApp;
use crate::TuiTheme;
use crate::icon_glyph;
use crate::process_view::ProcessRow;
use taskmanager_shell::{SortCol, SortDir};

use super::panel;
use super::process_details;
use super::render_empty_panel;
use super::sparkline;

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

pub(super) fn render_processes(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    // Room for the full field set: identity + resource scalars + the cpu-time/
    // nice/disk-rate parity rows + start/executable/command.
    let details_height = 18;
    let [search, table_area, details_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(details_height),
    ])
    .areas(area);
    let cursor = if app.search_active() { "▌" } else { "" };
    // The match counter: while a non-empty query is active, show how many
    // visible rows match and the current cursor's position among them (the
    // Enter-to-next-match walk's position). The query's own matches keep the
    // counter honest — a filter that empties the list shows 0.
    let match_counter = if app.search_active() && !app.query.trim().is_empty() {
        let count = app.visible_processes().len();
        let key = if count == 1 {
            "tui.search_matches_one"
        } else {
            "tui.search_matches_many"
        };
        t(key).replacen("{count}", &count.to_string(), 1)
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(format!(
            "{} {}{}",
            icon_glyph(IconId::Search),
            app.query,
            cursor
        ))
        .style(if app.search_active() {
            Style::new().fg(Color::White)
        } else {
            Style::new().fg(theme.dim)
        })
        .block(panel(
            &format!("{}{}", t("tui.search_title"), match_counter),
            theme,
        )),
        search,
    );
    // A confirmed zero swap total means the host has no swap device and the
    // Swap column is intentionally hidden. An unknown total keeps the column
    // visible so an unavailable value cannot be mistaken for an absent one.
    let swap_visible = super::process_data::swap_column_visible(app.projection().snapshot.as_ref());
    // The per-row CPU sparkline column adapts to terminal width the same way
    // `swap_visible` adapts to swap presence: hidden below a comfortable
    // threshold so the 54×16 minimum frame never regresses and the wide case
    // (≥160 cols) renders the trend without pushing other columns off-screen.
    let sparkline_visible = area.width >= SPARKLINE_MIN_AREA_WIDTH;
    // Shared panel title lists the Applications-page chords; Enter expands a
    // category/application header or opens details on a process row.
    let panel_title = t("tui.processes_title");
    let visible = app.visible_processes();
    // Resolve the multi-select anchor pid ONCE from the already-built visible
    // list (the O(N) filter+sort+alloc is paid for in `visible`). Each row's
    // `multi_select_style` check then becomes O(1) — `selected_pids().contains`
    // is HashSet O(1), and the anchor comparison is a pid equality — instead
    // of recomputing `app.visible_processes()` per row (O(N²) over the table).
    let anchor_pid = app.selected_detail_process().map(|process| process.pid);
    if visible.is_empty() {
        let message = if app.query.trim().is_empty() {
            t("empty.no_processes_reported")
        } else {
            t("empty.no_processes_match_query")
        };
        render_empty_panel(frame, theme, table_area, panel_title, message);
        process_details::render_process_details(frame, app, theme, details_area);
        return;
    }
    // The canonical category tree interleaves category/application headers
    // with recursive process nodes. The cursor indexes this one row list.
    let rows_model = crate::process_view::build_process_rows(
        &visible,
        &app.expanded_groups,
        &app.collapsed_tree,
        app.process_sort,
    );
    let columns = process_details::ColumnVisibility {
        swap_visible,
        sparkline_visible,
        hidden: &app.hidden_columns,
    };
    let column_visible = |column: SortCol| columns.visible(column);
    let row_window = super::table_window(rows_model.len(), app.selected, table_area);
    let rows: Vec<Row<'_>> = rows_model[row_window.start..row_window.end]
        .iter()
        .map(|row| match row {
            ProcessRow::Group {
                name: _,
                label,
                depth,
                count,
                cpu,
                memory,
                expanded,
                row_key: _,
            } => {
                let label = format!("{}{}", "  ".repeat(*depth), label);
                Row::new(process_details::group_header_cells(
                    &label,
                    *count,
                    *cpu,
                    *memory,
                    *expanded,
                    columns,
                    process_details::SearchHighlight {
                        query: &app.query,
                        search_active: app.search_active(),
                        theme,
                    },
                ))
                .style(Style::new().fg(theme.accent).add_modifier(Modifier::BOLD))
            }
            ProcessRow::TreeNode {
                process,
                depth,
                has_children,
                collapsed,
            } => {
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
                            depth: *depth,
                            branch: match (*has_children, *collapsed) {
                                (false, _) => process_details::ProcessTreeBranch::Leaf,
                                (true, false) => process_details::ProcessTreeBranch::Expanded,
                                (true, true) => process_details::ProcessTreeBranch::Collapsed,
                            },
                        }),
                        gray_zero: app.prefs.gray_zero,
                        local_time_rules: &app.local_time_rules,
                    },
                ));
                multi_select_style(row, app, theme, process.pid, anchor_pid)
            }
        })
        .collect();
    let mut widths = vec![
        Constraint::Length(7), // PID
        Constraint::Min(16),   // Name
        Constraint::Length(8), // CPU%
    ];
    if sparkline_visible {
        widths.push(Constraint::Length(sparkline::SPARKLINE_MAX_SAMPLES as u16));
    }
    widths.extend(
        [SortCol::Memory, SortCol::Pss]
            .into_iter()
            .filter(|candidate| column_visible(*candidate))
            .map(|_| Constraint::Length(10)),
    );
    if column_visible(SortCol::Swap) {
        widths.push(Constraint::Length(10));
    }
    widths.extend(
        [
            (SortCol::User, 12),
            (SortCol::State, 7),
            (SortCol::Threads, 7),
            (SortCol::Fds, 5),
            (SortCol::Nice, 5),
            (SortCol::StartTime, 11),
            (SortCol::CpuTime, 9),
            (SortCol::DiskRead, 9),
            (SortCol::DiskWrite, 9),
        ]
        .into_iter()
        .filter(|(candidate, _)| column_visible(*candidate))
        .map(|(_, width)| Constraint::Length(width)),
    );
    let table = Table::new(rows, widths)
        .header(process_header(
            (app.effective_sort_col(), app.process_sort.1),
            theme.accent,
            columns,
        ))
        .row_highlight_style(Style::new().bg(theme.highlight_bg).fg(Color::White))
        // Two blanks between columns (the ratatui default is 1) so numeric
        // readouts never read as one string — but only when the terminal
        // affords the widest projection without truncating trailing columns
        // (the 150-col budget cannot: the full header projection test pins
        // that narrow terminals keep every column at the one-blank gutter).
        .column_spacing(u16::from(area.width >= COLUMN_GUTTER_WIDE_WIDTH) + 1)
        .highlight_symbol("› ")
        .highlight_spacing(HighlightSpacing::Always)
        .block(panel(panel_title, theme));
    let mut state = TableState::default().with_selected(Some(row_window.selected));
    frame.render_stateful_widget(table, table_area, &mut state);
    process_details::render_process_details(frame, app, theme, details_area);
}

/// Tint a row that is a member of the batch-control multi-select set (but is
/// not the keyboard anchor, which the table's own highlight already paints).
/// The muted background reads as "marked" without competing with the anchor.
///
/// `anchor_pid` is resolved ONCE by the caller from the already-built visible
/// list (`visible.get(app.selected).map(|p| p.pid)`) so this per-row check is
/// O(1) — `selected_pids().contains` is HashSet O(1), and the anchor compare
/// is a pid equality — instead of recomputing `app.visible_processes()` per
/// row (which made the table O(N²) per frame: every row paid the O(N)
/// filter+sort+alloc just to read one pid).
fn multi_select_style<'a>(
    row: Row<'a>,
    app: &TuiApp,
    theme: TuiTheme,
    pid: u32,
    anchor_pid: Option<u32>,
) -> Row<'a> {
    if app.selected_pids().contains(&pid) && Some(pid) != anchor_pid {
        row.style(Style::new().bg(theme.highlight_bg).fg(Color::Gray))
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
    accent: Color,
    columns: process_details::ColumnVisibility,
) -> Row<'static> {
    let process_details::ColumnVisibility {
        swap_visible,
        sparkline_visible,
        hidden,
    } = columns;
    let visible = |column: SortCol| {
        process_details::ColumnVisibility {
            swap_visible,
            sparkline_visible,
            hidden,
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
    columns.extend(
        [SortCol::Memory, SortCol::Pss]
            .into_iter()
            .filter(|candidate| visible(*candidate)),
    );
    if visible(SortCol::Swap) {
        columns.push(SortCol::Swap);
    }
    columns.extend(
        [
            SortCol::User,
            SortCol::State,
            SortCol::Threads,
            SortCol::Fds,
            SortCol::Nice,
            SortCol::StartTime,
            SortCol::CpuTime,
            SortCol::DiskRead,
            SortCol::DiskWrite,
        ]
        .into_iter()
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
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::new().fg(accent).add_modifier(Modifier::BOLD)
            };
            Cell::from(text).style(style)
        })
        .collect();
    if sparkline_visible {
        cells.insert(
            splice_trend_at,
            Cell::from(t("proc.trend")).style(Style::new().fg(accent).add_modifier(Modifier::BOLD)),
        );
    }
    Row::new(cells).bottom_margin(1)
}
