//! Direct-track Applications table viewing state.

use super::*;

impl Default for ProcessViewing {
    fn default() -> Self {
        Self {
            // Matches the historic GPUI default (`SortCol::Cpu`, descending)
            // and the shell track's `ShellApp::process_sort` default.
            sort: (SortCol::Cpu, SortDir::Desc),
            status_filter: ProcessStatusFilter::All,
            query: String::new(),
        }
    }
}

impl ProcessViewing {
    /// The active (column, direction) sort.
    #[must_use]
    pub const fn sort(&self) -> (SortCol, SortDir) {
        self.sort
    }

    /// The active Applications status bucket.
    #[must_use]
    pub const fn status_filter(&self) -> ProcessStatusFilter {
        self.status_filter
    }

    /// The raw search query (trim at the projection boundary, like the shell
    /// track's `visible_processes` memo key).
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Replace the whole query (a search box reports absolute text). The
    /// matching grammar — `pid:`/`user:`/`status:`/`cmd:`/`name:` selectors
    /// plus the name-or-pid-or-user-or-cmdline fallback — lives in
    /// [`crate::matches_process_query`], the same function the shell track
    /// and the iced/TUI frontends consume.
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
    }

    /// Select the Applications status bucket (segmented pill click).
    pub fn set_status_filter(&mut self, filter: ProcessStatusFilter) {
        self.status_filter = filter;
    }

    /// Apply an ABSOLUTE (column, direction) sort — the persistence/saved-view
    /// restore edge. Interactive paths use [`Self::click_sort_column`] /
    /// [`Self::move_sort_column`] instead so the click conventions survive.
    pub fn set_sort(&mut self, column: SortCol, direction: SortDir) {
        self.sort = (column, direction);
    }

    /// Header-click semantics (the shell-track counterpart of
    /// `ShellApp::set_sort_column` plus its flip rule): clicking the
    /// already-active column toggles the direction; clicking a new column
    /// activates it with the conventional initial direction — ascending for
    /// text-like columns (`Name`/`User`/`Pid`/`State`), descending for every
    /// numeric resource column.
    pub fn click_sort_column(&mut self, column: SortCol) {
        let (_, direction) = self.sort;
        self.sort = if self.sort.0 == column {
            (column, direction.toggle())
        } else {
            (column, initial_sort_direction(column))
        };
    }

    /// Move the active sort column WITHOUT touching the direction (header
    /// ArrowLeft/ArrowRight navigation steps the column through the rendered
    /// header projection the caller computed).
    pub fn move_sort_column(&mut self, column: SortCol) {
        self.sort = (column, self.sort.1);
    }
}

/// Conventional initial direction for a freshly-activated column: text-like
/// columns read naturally ascending; numeric resource columns read
/// high-to-low.
const fn initial_sort_direction(column: SortCol) -> SortDir {
    match column {
        SortCol::Name | SortCol::User | SortCol::Pid | SortCol::State => SortDir::Asc,
        SortCol::Cpu
        | SortCol::Memory
        | SortCol::Pss
        | SortCol::Swap
        | SortCol::Threads
        | SortCol::CpuTime
        | SortCol::DiskRead
        | SortCol::DiskWrite
        | SortCol::StartTime
        | SortCol::Fds
        | SortCol::Nice => SortDir::Desc,
    }
}
