//! Applications-page process table renderer: the search bar, the canonical
//! category/application/process hierarchy, the per-row CPU sparkline
//! column, and the active-sort header. Extracted from `ui.rs` so no single
//! renderer file exceeds the source line budget; behavior unchanged.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Cell, Paragraph, Row};
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;

use crate::TuiApp;
use crate::TuiTheme;
use crate::process_view::ProcessRow;
use taskmanager_shell::{SortCol, SortDir};

use super::containers::{WindowedTableProps, render_windowed_table};
use super::frame_plan::TablePanelProjection;
use super::panel;
use super::process_details;
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
pub(super) fn process_table_layout(area: Rect) -> ProcessTableLayout {
    let [search, table, details] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(PROCESS_DETAILS_HEIGHT),
    ])
    .areas(area);
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
    let cursor = if search_focused { "▌" } else { "" };
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
    let panel_title = t("tui.processes_title");
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
        frame.render_widget(
            Paragraph::new(format!(
                "{} {}{}",
                theme.glyph(IconId::Search),
                app.query,
                cursor
            ))
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
        let columns = process_details::ColumnVisibility {
            swap_visible,
            sparkline_visible,
            hidden: &app.hidden_columns,
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
                title: panel_title,
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
                column_spacing: u16::from(table_area.width >= COLUMN_GUTTER_WIDE_WIDTH) + 1,
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
                    cpu,
                    memory,
                    expanded,
                    row_key: _,
                }) => {
                    let label = format!("{}{}", "  ".repeat(depth), label);
                    Row::new(process_details::group_header_cells(
                        &label,
                        count,
                        cpu,
                        memory,
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
                        taskmanager_shell::ProcessRowIdentity::from_process(process),
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
    identity: Option<taskmanager_shell::ProcessRowIdentity>,
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
