//! Page-local GPUI state retained by the root window entity.

use std::collections::{HashMap, HashSet};

use gpui::Pixels;

use crate::gpui_app::{processes_view, services_view, startup_view};

/// Services page state: status filter and search query.
#[derive(Clone)]
pub struct ServicesState {
    pub filter: services_view::ServiceFilter,
    pub query: String,
}

impl Default for ServicesState {
    fn default() -> Self {
        Self {
            filter: services_view::ServiceFilter::All,
            query: String::new(),
        }
    }
}

/// Startup page state: status filter and search query.
#[derive(Clone)]
pub struct StartupState {
    pub filter: startup_view::StartupFilter,
    pub query: String,
}

impl Default for StartupState {
    fn default() -> Self {
        Self {
            filter: startup_view::StartupFilter::All,
            query: String::new(),
        }
    }
}

/// Processes page expansion, affinity, column-visibility and resize state.
/// Sort, status filter and search query remain shell-owned.
#[derive(Clone, Default)]
pub struct ProcessAffinityEditorState {
    /// Window-local draft copied from a Ready affinity session on open.
    /// Applying the draft submits one typed replacement; it is not runtime
    /// affinity authority.
    pub cpus: HashSet<u32>,
    pub hover: Option<usize>,
}

#[derive(Clone)]
pub struct ProcessesState {
    pub collapsed: HashSet<u32>,
    pub expanded_apps: HashSet<String>,
    pub affinity_editor: ProcessAffinityEditorState,
    /// Columns hidden by the user. Name is never inserted.
    pub hidden_cols: HashSet<processes_view::SortCol>,
    /// User-resized widths for the resizable process columns.
    pub col_widths: HashMap<processes_view::SortCol, Pixels>,
    /// Presentation-only column cursor, independent from sort and selection.
    pub column_cursor: processes_view::SortCol,
    /// Cursor anchor retained during an active column resize.
    pub resize_anchor_x: Option<Pixels>,
}

impl Default for ProcessesState {
    fn default() -> Self {
        Self {
            collapsed: HashSet::new(),
            expanded_apps: processes_view::default_category_expansions(),
            affinity_editor: ProcessAffinityEditorState::default(),
            hidden_cols: HashSet::from([
                processes_view::SortCol::Threads,
                processes_view::SortCol::StartTime,
                processes_view::SortCol::Swap,
                processes_view::SortCol::CpuTime,
                processes_view::SortCol::Fds,
                processes_view::SortCol::Nice,
            ]),
            col_widths: HashMap::new(),
            column_cursor: processes_view::SortCol::Name,
            resize_anchor_x: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NavOrientation {
    #[default]
    Horizontal,
    Vertical,
}
