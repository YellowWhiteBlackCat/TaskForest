//! Processes (Apps) view: a virtualized (`uniform_list`) canonical category tree,
//! clickable column-sort headers (Name / User / PID /
//! Status / CPU / Memory / Disk read / Disk write, each with a ▲/▼ direction indicator),
//! click-to-select rows, and an action bar
//! (End task / Kill / Suspend / Resume). Search + the 8-signal right-click menu come
//! later (gpui 0.2.2 ships no text_input or popover, so those need custom building).
//!
//! **State ownership:** collapse/expand + affinity dialog state
//! live as fields on `RootView` (prefixed `processes_`), threaded into
//! [`render_processes`] by value / by ref each render. The SORT, the status
//! filter, and the search query live in the shell `DirectTrackState`
//! process-viewing slot (see `root/shell_state.rs`) — the same authority the
//! iced/TUI frontends read — and mutating a header/pill click or a search
//! keystroke goes through the shell reducers (`click_process_sort` /
//! `set_process_status_filter` / `set_process_query`). The search grammar is
//! the shared `taskmanager_shell::matches_process_query` (structured
//! `pid:`/`user:`/`status:`/`cmd:`/`name:` selectors plus the multi-field
//! fallback), so all three frontends filter identically.

pub mod chrome;
pub mod rows;
pub mod sort_key;

use gpui::{ScrollHandle, UniformListScrollHandle};

/// Per-window scroll ownership for the Apps table. The vertical virtual-list
/// handle and horizontal content handle are kept together so RootView carries
/// one state boundary for the page rather than two unrelated globals.
#[derive(Default)]
pub struct ProcessesScrollState {
    pub vertical: UniformListScrollHandle,
    pub horizontal: ScrollHandle,
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/processes_view/tests.rs"]
mod tests;

pub use chrome::*;
pub use rows::*;

// The status-bucket filter is the shell's `ProcessStatusFilter` (classifier,
// labels, and stable control ids live there); re-exported here so the
// historical `processes_view::ProcessStatusFilter` import path keeps working.
pub use taskmanager_shell::ProcessStatusFilter;
