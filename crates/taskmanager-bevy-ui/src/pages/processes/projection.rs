//! Process-row display projection for the Bevy Applications table.
//!
//! This module owns the current/last-known fold and the final cell strings.
//! Scene adapters receive those strings and never read process observations
//! while painting. The projection is deliberately independent of Bevy scene
//! types so the same behavior can be tested without a window.

use taskforest_core::core::process::ProcessItem;
use taskforest_core::core::time::LocalTimeRulesObservation;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::{
    MISSING_VALUE, bytes, optional_cpu_time_seconds, optional_nice, process_anomaly_summary,
    start_clock_local, uninterruptible_process_summary,
};
use taskmanager_shell::saved_views::sort_token;
use taskmanager_shell::{ShellApp, SortCol, SortDir, process_semantic_key};
pub(crate) use taskmanager_ui_contract::ProcessColumnSpec;

use crate::widgets::table::{SortProjection, visible_columns};
use taskforest_core::core::percent::precise_percent_text;
use zhuge_ui_core::row_page::RowPage;

/// One row's cell vector over the contract columns (contract order, widths,
/// and numeric alignment come from the shared vocabulary, never a local copy).
/// The selected row prefixes the Name cell with the TUI's `›` cursor marker.
pub(crate) fn row_cells(
    process: &ProcessItem,
    columns: &[&ProcessColumnSpec],
    selected: bool,
) -> Vec<String> {
    columns
        .iter()
        .map(|column| {
            let text = cell_text(process, column.id);
            if selected && column.id == "Name" {
                format!("› {text}")
            } else {
                text
            }
        })
        .collect()
}

/// Resolve one row cell by the contract's stable column identity.
///
/// Card surfaces use semantic IDs rather than positional indices because a
/// saved view may hide columns; a positional lookup would silently paint a
/// different business fact.
#[must_use]
pub(crate) fn cell_by_id(row: &ProcessRowView, columns: &[&ProcessColumnSpec], id: &str) -> String {
    columns
        .iter()
        .position(|column| column.id == id)
        .and_then(|index| row.cells.get(index))
        .cloned()
        .unwrap_or_else(|| MISSING_VALUE.to_owned())
}

/// One cell's final display text. Unavailable scalars render the shared
/// `MISSING_VALUE` dash, exactly like the TUI cells — a provider failure is
/// never shown as a zero. The Start column uses the unsupported local-time
/// observation, so it renders `—` until this frontend observes a timezone.
fn cell_text(process: &ProcessItem, column: &str) -> String {
    match column {
        "Name" => process.name.clone(),
        "User" => process
            .current_user()
            .unwrap_or_else(|| MISSING_VALUE.to_owned()),
        "PID" => process.pid.to_string(),
        "Threads" => process
            .current_threads()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| value.to_string()),
        "StartTime" => start_clock_local(
            process.current_start_time_secs(),
            &LocalTimeRulesObservation::unsupported(0),
        ),
        "Status" => process.status.clone(),
        "CPU" => process
            .current_cpu_percentage()
            .map_or_else(|| MISSING_VALUE.to_owned(), precise_percent_text),
        "Memory" => process
            .current_memory_bytes()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "Swap" => process
            .current_swap_bytes()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "MemoryPss" => process
            .current_memory_pss_bytes()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "DiskRead" => process
            .current_disk_read_bytes_per_sec()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "DiskWrite" => process
            .current_disk_write_bytes_per_sec()
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes),
        "Network" => process
            .current_network_bytes_per_sec()
            .map_or_else(|| MISSING_VALUE.to_owned(), |v| format!("{}/s", bytes(v))),
        "CPUTime" => optional_cpu_time_seconds(process.current_cpu_time_secs()),
        "FDs" => process
            .current_fds()
            .map_or_else(|| MISSING_VALUE.to_owned(), |value| value.to_string()),
        "Nice" => optional_nice(process.current_nice()),
        _ => MISSING_VALUE.to_owned(),
    }
}

/// The shell sort as the widgets' header projection input.
///
/// The column token is the shell's canonical `sort_token` — the same string
/// the persisted sort and hidden-column tokens use, so persistence, the
/// contract row and the header cannot disagree.
pub(crate) fn sort_projection(sort: (SortCol, SortDir)) -> SortProjection {
    let (column, direction) = sort;
    SortProjection {
        column: sort_token(column),
        descending: direction == SortDir::Desc,
    }
}

/// One rendered row: its visible-set index, identity for the accessibility
/// node and the details seam, its cells, and the selected flag.
pub(crate) struct ProcessRowView {
    /// Visible-set index of the row (the shell cursor's coordinate space).
    pub(crate) index: usize,
    /// Core-backed incarnation key for semantic and accessibility identity.
    pub(crate) semantic_id: String,
    pub(crate) name: String,
    pub(crate) cells: Vec<String>,
    pub(crate) selected: bool,
    /// Whether the process carries a resolved application identity — the
    /// precondition for the inline app icon in the name cell.
    pub(crate) app_icon: bool,
}

/// Full window projection: the total behind the filter plus the visible slice.
pub(crate) struct ProcessRowsProjection {
    pub(crate) total: usize,
    pub(crate) window: RowPage,
    pub(crate) rows: Vec<ProcessRowView>,
}

/// Build the window projection with optional hidden column filtering.
pub(crate) fn rows_projection_with_hidden(
    shell: &ShellApp,
    viewport_rows: usize,
    scroll_top: usize,
    hidden_cols: &[&str],
) -> ProcessRowsProjection {
    let visible = shell.visible_processes();
    let total = visible.len();
    let window = RowPage::pin(total, viewport_rows, scroll_top);
    let selected = total.checked_sub(1).map(|last| shell.selected.min(last));
    let columns = visible_columns(hidden_cols);
    let rows = visible[window.range()]
        .iter()
        .enumerate()
        .map(|(offset, process)| {
            let index = window.first() + offset;
            let is_selected = Some(index) == selected || shell.is_process_selected(process);
            ProcessRowView {
                index,
                semantic_id: process_semantic_key(process),
                name: process.name.clone(),
                cells: row_cells(process, &columns, is_selected),
                selected: is_selected,
                app_icon: process.current_application_identity().is_some(),
            }
        })
        .collect();
    ProcessRowsProjection {
        total,
        window,
        rows,
    }
}

/// Build the window projection. The selected flag clamps the shell cursor to
/// the row space first (the same defensive clamp as the TUI `table_window`),
/// so a stale cursor after a shrinking fold can never index out of range or
/// silently select nothing.
pub(crate) fn rows_projection(
    shell: &ShellApp,
    viewport_rows: usize,
    scroll_top: usize,
) -> ProcessRowsProjection {
    rows_projection_with_hidden(shell, viewport_rows, scroll_top, &[])
}

/// Build the row views for an explicit set of visible-set indices.
///
/// The card lane admits whole status buckets out of the shell's visible
/// projection, so it needs row views for the ADMITTED indices only: a
/// viewport window would cut a bucket in half, and projecting every visible
/// row would build cells nobody paints.
pub(crate) fn row_views_for_indices(
    shell: &ShellApp,
    indices: &[usize],
    hidden_cols: &[&str],
) -> Vec<ProcessRowView> {
    let visible = shell.visible_processes();
    let selected = visible
        .len()
        .checked_sub(1)
        .map(|last| shell.selected.min(last));
    let columns = visible_columns(hidden_cols);
    indices
        .iter()
        .filter_map(|index| {
            let process = visible.get(*index)?;
            let is_selected = Some(*index) == selected || shell.is_process_selected(process);
            Some(ProcessRowView {
                index: *index,
                semantic_id: process_semantic_key(process),
                name: process.name.clone(),
                cells: row_cells(process, &columns, is_selected),
                selected: is_selected,
                app_icon: process.current_application_identity().is_some(),
            })
        })
        .collect()
}

/// The status line under the search box: the shared running-count copy, plus
/// the shared match-counter copy while a query is active.
pub(crate) fn count_line_text(visible: usize, query: &str) -> String {
    let base = t("proc.processes_running_subtitle").replacen("{count}", &visible.to_string(), 1);
    if query.trim().is_empty() {
        return base;
    }
    let key = if visible == 1 {
        "tui.search_matches_one"
    } else {
        "tui.search_matches_many"
    };
    format!(
        "{base}{}",
        t(key).replacen("{count}", &visible.to_string(), 1)
    )
}

pub(crate) fn count_line_text_for_shell(shell: &ShellApp, visible: usize, query: &str) -> String {
    let base = count_line_text(visible, query);
    let processes = shell
        .projection()
        .processes
        .as_ref()
        .map(|items| items.as_slice());
    [
        uninterruptible_process_summary(processes),
        process_anomaly_summary(processes),
    ]
    .into_iter()
    .flatten()
    .fold(base, |line, summary| format!("{line} · {summary}"))
}

/// Honest empty-table copy: a quiet platform (no processes reported yet) is a
/// different state from an over-narrow query — shared `empty.*` strings.
pub(crate) fn empty_state_text(query: &str) -> String {
    if query.trim().is_empty() {
        t("empty.no_processes_reported").to_owned()
    } else {
        t("empty.no_processes_match_query").to_owned()
    }
}
