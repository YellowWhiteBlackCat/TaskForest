//! Process-table cells, group-header cells, the selected-process detail
//! panel, and its bounded ProcessInsights cards. Extracted from `ui.rs`
//! to keep the renderer dispatch under the source line budget.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Wrap};
use taskmanager_application::process_details_vm::{DetailValue, ProcessDetailsField};
use taskmanager_application::units::UnitPreferences;
use taskmanager_application::{AppPage, FrozenProcessIdentity, ProcessItem, i18n::t};

use super::highlight;
use super::{kv, panel};
use crate::TuiApp;
use crate::TuiTheme;
use taskmanager_shell::SortCol;
use taskmanager_shell::presentation::{MISSING_VALUE, bytes, missing_value};

mod insights;
pub(crate) use insights::{insights_lines, network_requires_escalation};

/// Clamp a stored vertical-scroll intent to the valid viewport range so the
/// rendered content never scrolls past the last line. `content_lines` is the
/// wrapped height of the text (as reported by `Paragraph::line_count` against
/// the inner width); `visible_height` is the inner area height (rows available
/// for text, excluding any block borders). The stored `offset` is the user's
/// intent and may exceed the max when content shrinks; the renderer always uses
/// the clamped value. Returns `(effective_offset, max_offset)` so callers can
/// render a position indicator / overflow hint.
pub(crate) fn clamped_scroll(
    content_lines: usize,
    visible_height: u16,
    offset: usize,
) -> (usize, usize) {
    if visible_height == 0 {
        return (0, 0);
    }
    let max = content_lines.saturating_sub(visible_height as usize);
    (offset.min(max), max)
}

/// Compute the wrapped content height (text rows, excluding block borders) for
/// a paragraph built from `lines` with `Wrap { trim: true }` at `width`. Uses
/// ratatui's own `Paragraph::line_count` (the `unstable-rendered-line-info`
/// feature) so the count matches what the renderer actually draws — never a
/// hand-rolled char-width estimate that would drift from the word-wrapper.
pub(crate) fn wrapped_content_height(lines: &[Line<'_>], width: u16) -> usize {
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .line_count(width)
}

/// Build the cell vector for one recursive process row. Every hierarchy level
/// shares this column layout and search-highlight path.
/// One search-highlighted cell: the query's matching segments wear the
/// accent style, everything else renders plain. The single implementation
/// for every highlighted column (name / pid / user / group header) — ADR-020
/// single-source rule. Returns an owned cell so callers can pass formatted
/// temporaries (pid strings, group names) without lifetime gymnastics.
fn search_highlight_cell(
    text: &str,
    query: &str,
    search_active: bool,
    theme: TuiTheme,
) -> Cell<'static> {
    if search_active && !query.trim().is_empty() {
        let spans = highlight::highlight_segments(text, query)
            .into_iter()
            .map(|(segment, is_match)| {
                Span::styled(
                    segment.to_owned(),
                    if is_match {
                        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::new()
                    },
                )
            })
            .collect::<Vec<_>>();
        Cell::from(Line::from(spans))
    } else {
        Cell::from(text.to_owned())
    }
}

/// One numeric cell that honors the gray-zero-values policy: a MEASURED zero
/// renders in the muted foreground when the preference is on; unavailable
/// values render as dashes by the caller and never reach this helper, so an
/// unavailable value can never be dimmed as a zero.
fn zero_tinted_cell(value: Option<u64>, gray_zero: bool, theme: TuiTheme) -> Cell<'static> {
    match value {
        Some(0) if gray_zero => Cell::from(bytes(0)).style(Style::new().fg(theme.dim)),
        Some(value) => Cell::from(bytes(value)),
        None => Cell::from(MISSING_VALUE),
    }
}

/// Complete renderer input for one canonical process-tree row. Keeping the
/// row identity, search projection, column geometry and local-time rule in one
/// named value prevents call sites from silently swapping positional flags.
#[derive(Clone, Copy)]
pub(super) struct ProcessCellInput<'a> {
    pub(super) process: &'a ProcessItem,
    pub(super) highlight: SearchHighlight<'a>,
    pub(super) columns: ColumnVisibility<'a>,
    pub(super) tree_prefix: Option<ProcessTreePrefix>,
    pub(super) gray_zero: bool,
    pub(super) local_time_rules: &'a taskmanager_application::LocalTimeRulesObservation,
}

/// Tree chrome projected for one process cell. The names make the upstream
/// hierarchy booleans explicit at the renderer boundary.
#[derive(Clone, Copy)]
pub(super) struct ProcessTreePrefix {
    pub(super) depth: usize,
    pub(super) branch: ProcessTreeBranch,
}

/// Exactly the three visible tree states. A leaf cannot also be marked
/// collapsed, so the cell renderer never receives an invalid boolean pair.
#[derive(Clone, Copy)]
pub(super) enum ProcessTreeBranch {
    Leaf,
    Expanded,
    Collapsed,
}

pub(super) fn process_cells_with_local_time<'a>(input: ProcessCellInput<'a>) -> Vec<Cell<'a>> {
    let ProcessCellInput {
        process,
        highlight:
            SearchHighlight {
                query,
                search_active,
                theme,
            },
        columns,
        tree_prefix,
        gray_zero,
        local_time_rules,
    } = input;
    let data = super::process_data::process_cell_data(process, local_time_rules);
    let ColumnVisibility {
        swap_visible,
        sparkline_visible,
        hidden,
    } = columns;
    let visible = |column: SortCol| {
        ColumnVisibility {
            swap_visible,
            sparkline_visible,
            hidden,
        }
        .visible(column)
    };
    let name_cell: Cell<'a> = if let Some(ProcessTreePrefix { depth, branch }) = tree_prefix {
        // Tree-node name cell: the depth indent plus the expand/collapse
        // chevron (a leaf gets a dot so the columns stay aligned).
        let marker = match branch {
            ProcessTreeBranch::Leaf => "·",
            ProcessTreeBranch::Expanded => "▼",
            ProcessTreeBranch::Collapsed => "▶",
        };
        let mut spans = vec![Span::styled(
            format!("{}{} ", "  ".repeat(depth), marker),
            Style::new().fg(theme.dim),
        )];
        if search_active && !query.trim().is_empty() {
            spans.extend(
                highlight::highlight_segments(process.name.as_str(), query)
                    .into_iter()
                    .map(|(segment, is_match)| {
                        Span::styled(
                            segment,
                            if is_match {
                                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
                            } else {
                                Style::new()
                            },
                        )
                    }),
            );
        } else {
            spans.push(Span::raw(process.name.as_str()));
        }
        Cell::from(Line::from(spans))
    } else {
        search_highlight_cell(process.name.as_str(), query, search_active, theme)
    };
    // The pid cell highlights a numeric query (searching a pid matches the
    // pid column, not just the name). The formatted string is owned by the
    // cell, so the temporary borrow ends before the vec lives.
    let pid_text = process.pid.to_string();
    let cpu_cell = {
        // A measured 0.0% CPU dims under gray-zero; any other value stays
        // white.
        let cpu = data.cpu;
        let cell = Cell::from(cpu.map_or_else(
            || MISSING_VALUE.to_owned(),
            |value| format!("{value:>5.1}%"),
        ));
        if gray_zero && cpu == Some(0.0) {
            cell.style(Style::new().fg(theme.dim))
        } else {
            cell
        }
    };
    let mut cells: Vec<Cell<'a>> = vec![
        search_highlight_cell(&pid_text, query, search_active, theme),
        name_cell,
        cpu_cell,
    ];
    if sparkline_visible {
        // Per-row CPU trend sits immediately right of the CPU% readout
        // (mirrors gpui `processes_view/rows/cells.rs`: CPU cell + sparkline
        // gated together). A tree PARENT carries no single history — its
        // `cpu_history` aggregates children and would mislead — so it renders
        // an honest dash like the other per-process-only columns on a group
        // header. A real leaf renders the shared per-row sparkline primitive
        // (per-row min/max normalization built in); a just-started process
        // (<2 finite samples) falls through to the dotted "collecting"
        // placeholder rather than a fabricated single-block line.
        let is_parent = tree_prefix.is_some_and(|prefix| {
            matches!(
                prefix.branch,
                ProcessTreeBranch::Expanded | ProcessTreeBranch::Collapsed
            )
        });
        let trend = if is_parent {
            Cell::from(MISSING_VALUE)
        } else {
            Cell::from(super::sparkline::process_cpu_trend(&process.cpu_history))
        };
        cells.push(trend);
    }
    cells.push(zero_tinted_cell(data.memory, gray_zero, theme));
    if visible(SortCol::Pss) {
        cells.push(zero_tinted_cell(data.pss, gray_zero, theme));
    }
    if visible(SortCol::Swap) {
        cells.push(zero_tinted_cell(data.swap, gray_zero, theme));
    }
    // The tail columns (User / State / Threads / Fds / Nice / StartTime /
    // CpuTime / Disk r/w) push only when the user has not hidden them; the
    // header and widths apply the same gate so every row stays aligned.
    if visible(SortCol::User) {
        cells.push(search_highlight_cell(
            &data.user,
            query,
            search_active,
            theme,
        ));
    }
    if visible(SortCol::State) {
        cells.push(Cell::from(process.status.as_str()));
    }
    if visible(SortCol::Threads) {
        cells.push(Cell::from(data.threads));
    }
    if visible(SortCol::Fds) {
        cells.push(Cell::from(data.fds));
    }
    if visible(SortCol::Nice) {
        cells.push(Cell::from(data.nice));
    }
    if visible(SortCol::StartTime) {
        cells.push(Cell::from(data.start_time));
    }
    if visible(SortCol::CpuTime) {
        cells.push(Cell::from(data.cpu_time));
    }
    if visible(SortCol::DiskRead) {
        cells.push(zero_tinted_cell(data.disk_read, gray_zero, theme));
    }
    if visible(SortCol::DiskWrite) {
        cells.push(zero_tinted_cell(data.disk_write, gray_zero, theme));
    }
    cells
}

/// Which optional columns the renderer is emitting this frame. The two
/// adaptive columns (`Swap`, `Trend`) hide on hosts without swap and on
/// narrow terminals respectively; the user-hidden set (`C` column menu) drops
/// toggleable columns from every row + the header so the table stays aligned.
/// All flags are computed once in `render_processes` and threaded through
/// `process_cells` / `group_header_cells` / `process_header` so every row and
/// the header agree on the column layout. Bundled so the row builders stay
/// under clippy's argument budget.
#[derive(Clone, Copy)]
pub(super) struct ColumnVisibility<'a> {
    pub(super) swap_visible: bool,
    pub(super) sparkline_visible: bool,
    pub(super) hidden: &'a std::collections::HashSet<SortCol>,
}

impl ColumnVisibility<'_> {
    /// Whether a column is rendered this frame: PID and Name are
    /// always-visible identity columns; every other column obeys the
    /// user-hide set (and Swap additionally requires a swap device).
    #[must_use]
    pub(super) fn visible(self, column: SortCol) -> bool {
        match column {
            SortCol::Pid | SortCol::Name => true,
            SortCol::Swap => self.swap_visible && !self.hidden.contains(&SortCol::Swap),
            _ => !self.hidden.contains(&column),
        }
    }
}

/// Build the cell vector for one group header row. The header fills the SAME
/// column layout as a process row (so the table alignment is unchanged) but
/// shows the expansion chevron, the group name with its member count as "×N",
/// the summed CPU% / memory, and honest dashes for the per-process-only
/// columns (PSS / Swap / User / Status have no aggregate meaning).
/// The search-highlight context for one rendered cell group (the query, its
/// active flag, and the theme). Bundled so the row builders stay under
/// clippy's argument budget.
#[derive(Clone, Copy)]
pub(super) struct SearchHighlight<'a> {
    pub(super) query: &'a str,
    pub(super) search_active: bool,
    pub(super) theme: TuiTheme,
}

pub(super) fn group_header_cells(
    name: &str,
    count: usize,
    cpu: f32,
    memory: u64,
    expanded: bool,
    columns: ColumnVisibility,
    highlight: SearchHighlight,
) -> Vec<Cell<'static>> {
    let ColumnVisibility {
        swap_visible,
        sparkline_visible,
        hidden,
    } = columns;
    let visible = |column: SortCol| {
        ColumnVisibility {
            swap_visible,
            sparkline_visible,
            hidden,
        }
        .visible(column)
    };
    let chevron = if expanded { "▾" } else { "▸" };
    let group_name = format!("{name} ×{count}");
    let mut cells: Vec<Cell<'static>> = vec![
        Cell::from(chevron),
        search_highlight_cell(
            &group_name,
            highlight.query,
            highlight.search_active,
            highlight.theme,
        ),
        Cell::from(format!("{cpu:>5.1}%")),
    ];
    if sparkline_visible {
        // A group header aggregates many processes; it carries no single
        // cpu_history a sparkline could plot. Mirror the per-process-only
        // columns below: an honest dash keeps the row aligned with the
        // widened table (and matches the gpui `show_spark=false` blank cell).
        cells.push(Cell::from(MISSING_VALUE));
    }
    cells.push(Cell::from(bytes(memory)));
    if visible(SortCol::Pss) {
        cells.push(Cell::from(MISSING_VALUE));
    }
    if visible(SortCol::Swap) {
        cells.push(Cell::from(MISSING_VALUE));
    }
    // The per-process-only columns (User / State / Threads / Fds / Nice /
    // StartTime / CpuTime / Disk r/w) have no aggregate meaning: honest
    // dashes keep the row aligned with the widened table. Each pushes only
    // when the column is visible (same gate as the process rows).
    for column in [
        SortCol::User,
        SortCol::State,
        SortCol::Threads,
        SortCol::Fds,
        SortCol::Nice,
        SortCol::StartTime,
        SortCol::CpuTime,
        SortCol::DiskRead,
        SortCol::DiskWrite,
    ] {
        if visible(column) {
            cells.push(Cell::from(MISSING_VALUE));
        }
    }
    cells
}

/// Render one VM value with the TUI's shared dash spelling — the single
/// adapter between the neutral [`DetailValue`] fold and every details/
/// properties row this module renders.
pub(crate) fn vm_text(
    rows: &[taskmanager_application::process_details_vm::ProcessDetailsRowVm],
    field: ProcessDetailsField,
) -> String {
    rows.iter()
        .find(|row| row.field == field)
        .map_or_else(missing_value, |row| match &row.value {
            DetailValue::Text(text) => text.clone(),
            DetailValue::Missing => missing_value(),
        })
}

/// Fold the selected process into the detail panel's label/value pairs by
/// consuming the neutral process-details VM (single fold, ADR-020). The
/// TUI's compaction stays presentational: three rows join two VM fields
/// (CPU/Memory, PSS/Swap, Threads/FD) and the start row wraps the VM
/// timestamp with the verified-token note when the frozen identity carries
/// an authoritative token. Labels resolve through the shared i18n catalog.
fn detail_panel_pairs_with_local_time(
    process: &ProcessItem,
    frozen: Option<&FrozenProcessIdentity>,
    local_time_rules: &taskmanager_application::LocalTimeRulesObservation,
) -> Vec<(&'static str, String)> {
    let rows = taskmanager_application::process_details_vm::process_details_rows_with_local_time(
        process,
        &UnitPreferences::default(),
        local_time_rules,
    );
    let text = |field| vm_text(&rows, field);
    let start = if frozen.is_some_and(|identity| identity.authoritative_start_token().is_some()) {
        t("tui.start_token_verified").replacen("{start}", &text(ProcessDetailsField::StartTime), 1)
    } else {
        text(ProcessDetailsField::StartTime)
    };
    vec![
        (t("common.name"), text(ProcessDetailsField::Name)),
        (t("proc.pid"), text(ProcessDetailsField::Pid)),
        (t("common.user"), text(ProcessDetailsField::User)),
        (t("common.status"), text(ProcessDetailsField::Status)),
        (
            t("common.cpu_memory"),
            format!(
                "{} / {}",
                text(ProcessDetailsField::Cpu),
                text(ProcessDetailsField::Memory)
            ),
        ),
        (
            t("common.pss_swap"),
            format!(
                "{} / {}",
                text(ProcessDetailsField::Pss),
                text(ProcessDetailsField::Swap)
            ),
        ),
        (
            t("common.threads_fd"),
            format!(
                "{} / {}",
                text(ProcessDetailsField::Threads),
                text(ProcessDetailsField::Fds)
            ),
        ),
        (t("proc.cpu_time"), text(ProcessDetailsField::CpuTime)),
        (t("proc.nice"), text(ProcessDetailsField::Nice)),
        (t("proc.disk_read"), text(ProcessDetailsField::DiskReadRate)),
        (
            t("proc.disk_write"),
            text(ProcessDetailsField::DiskWriteRate),
        ),
        (t("proc.start"), start),
        (t("common.executable"), text(ProcessDetailsField::Exe)),
        (t("prop.command"), text(ProcessDetailsField::Cmdline)),
    ]
}

/// Detail panel for the selected process: frozen identity facts plus the
/// current row's full field set. In the grouped modes the cursor ranges over
/// a visual row list that interleaves group headers; a header has no single
/// process, so the panel surfaces an honest hint instead of fabricating one.
/// Missing rows (empty list / cursor past the end) render the empty state.
pub(super) fn render_process_details(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
) {
    let Some(process) = app.selected_detail_process() else {
        // Distinguish "no processes at all" from "the cursor is on a group
        // header" so the hint stays honest in the grouped modes.
        let on_group_header = app.page() == AppPage::Applications
            && matches!(
                app.process_rows_snapshot().get(app.selected),
                Some(crate::process_view::ProcessRow::Group { .. })
            );
        let hint = if on_group_header {
            t("tui.details_group_hint").replacen("{label}", t("empty.no_process_selected"), 1)
        } else {
            t("tui.details_move_hint").replacen("{label}", t("empty.no_process_selected"), 1)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, Style::new().fg(theme.dim))))
                .block(panel(t("prop.process_details"), theme)),
            area,
        );
        return;
    };
    let frozen = app.application.selected_process.as_ref();
    let mut lines: Vec<Line<'static>> =
        detail_panel_pairs_with_local_time(&process, frozen, &app.local_time_rules)
            .into_iter()
            .map(|(label, value)| kv(label, value, theme))
            .collect();
    // The insight cards project the shared per-process insight projection
    // (last-wins for the frozen target). Bounded: the terminal clips the
    // panel, so the renderer only appends the cards for the selected pid.
    lines.extend(insights_lines(app, theme, process.pid));
    // Short-terminal scroll: the detail + insights content can exceed the
    // fixed 18-row panel, so the paragraph scrolls by the clamped user intent
    // (Ctrl+Up / Ctrl+Down on the Applications page). The wrap-aware height
    // comes from ratatui's own line_count against the inner width, so the
    // clamp matches what the renderer actually draws — never a char-width
    // estimate that would drift from the word-wrapper. When content overflows,
    // the panel title surfaces the scroll chord so the feature is discoverable.
    let inner_width = area.width.saturating_sub(2);
    let inner_height = area.height.saturating_sub(2);
    let content_lines = wrapped_content_height(&lines, inner_width);
    let (effective, max) = clamped_scroll(content_lines, inner_height, app.detail_scroll);
    let focused = app.focus_panel == crate::FocusPanel::Details;
    let scroll_hint = if focused {
        t("tui.scroll_hint_focused")
    } else if max > 0 {
        t("tui.scroll_hint_available")
    } else {
        ""
    };
    let title = format!(
        "{}{}{}",
        if focused {
            format!("▸ {}", t("prop.process_details"))
        } else {
            t("prop.process_details").to_owned()
        },
        scroll_hint,
        if max > 0 {
            format!(" {effective}/{max}")
        } else {
            String::new()
        },
    );
    let block = panel(title.as_str(), theme);
    // The focused panel gets a brighter border so the keyboard focus is
    // visually unambiguous (the table highlight is the other half of the
    // focus ring).
    let block = if focused {
        block.border_style(Style::new().fg(theme.accent))
    } else {
        block
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: true })
            .scroll((effective as u16, 0)),
        area,
    );
}

#[cfg(test)]
#[path = "../../tests/gui/ui/process_details_tests.rs"]
mod detail_panel_vm_tests;
