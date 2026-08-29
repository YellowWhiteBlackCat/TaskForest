//! Process-table and inventory-table sort model for the shell (ADR-027).
//!
//! Owns the renderer-neutral sort types ([`SortCol`]/[`SortDir`] for the
//! process table, [`InfoSortCol`]/[`InfoTable`] for the Services/Startup/Users
//! tables) plus the interactive sort methods on [`ShellApp`] (cycle column,
//! flip direction, header-click routing, and the projected/sorted row accessors
//! every frontend reads). The per-axis comparison semantics are NOT owned here:
//! [`SortCol::ascending`] delegates through [`super::sort_axis`] to the
//! toolkit-neutral [`taskmanager_application::process_sort`] comparator (typed
//! availability, ASCII case folding, `None`-before-`Some` ordering), the same
//! single source every frontend's tree/group projection consumes, so the
//! frontends cannot drift.
use super::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::startup::StartupEntry;

/// Process-table column the list is sorted by. This models the ordering that
/// was previously hard-coded inside [`ShellApp::visible_processes`] so the render
/// layer can surface it in the table header and the user can change it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SortCol {
    Pid,
    Name,
    /// Default: matches the historic primary key (highest-CPU first).
    #[default]
    Cpu,
    Memory,
    /// Hybrid proportional-set-size memory. This remains independent from
    /// resident memory so an unavailable PSS observation cannot sort as RSS.
    Pss,
    /// Per-process swap, kept separate from RSS and PSS.
    Swap,
    User,
    State,
    /// Advanced columns reachable in wide frontends (iced per-header sort) but
    /// deliberately excluded from the core display cycle ([`Self::next`]) so a
    /// 54-column terminal never has to cycle through columns it cannot show.
    Threads,
    CpuTime,
    DiskRead,
    DiskWrite,
    /// Process start time (wall-clock seconds since boot). Mirrors the gpui
    /// `SortCol::StartTime` advanced column.
    StartTime,
    /// Open file-descriptor count. Mirrors the gpui `SortCol::Fds` column.
    Fds,
    /// Nice value (-20..19). Mirrors the gpui `SortCol::Nice` column.
    Nice,
}

impl SortCol {
    /// Every sortable process-table column, in declaration order. The
    /// iteration source for consumers and tests — never duplicate the list.
    pub const ALL: [SortCol; 15] = [
        SortCol::Pid,
        SortCol::Name,
        SortCol::Cpu,
        SortCol::Memory,
        SortCol::Pss,
        SortCol::Swap,
        SortCol::User,
        SortCol::State,
        SortCol::Threads,
        SortCol::CpuTime,
        SortCol::DiskRead,
        SortCol::DiskWrite,
        SortCol::StartTime,
        SortCol::Fds,
        SortCol::Nice,
    ];

    /// Header label rendered for the column.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pid => "PID",
            Self::Name => "Name",
            Self::Cpu => "CPU",
            Self::Memory => "Memory",
            Self::Pss => "PSS",
            Self::Swap => "Swap",
            Self::User => "User",
            Self::State => "State",
            Self::Threads => "Threads",
            Self::CpuTime => "CPU time",
            Self::DiskRead => "Disk R/s",
            Self::DiskWrite => "Disk W/s",
            Self::StartTime => "Start",
            Self::Fds => "Fds",
            Self::Nice => "Nice",
        }
    }

    /// Cycle to the next column in display (left-to-right) order.
    ///
    /// The advanced columns (Threads/CpuTime/DiskRead/DiskWrite) are intentionally
    /// not in the cycle — they restart it at Pid — because the TUI `s` key drives
    /// this cycle and a terminal cannot display those columns. Wide frontends
    /// reach them directly via [`ShellApp::set_sort_column`] (per-header click).
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Pid => Self::Name,
            Self::Name => Self::Cpu,
            Self::Cpu => Self::Memory,
            Self::Memory => Self::Pss,
            Self::Pss => Self::Swap,
            Self::Swap => Self::User,
            Self::User => Self::State,
            Self::State => Self::Pid,
            Self::Threads
            | Self::CpuTime
            | Self::DiskRead
            | Self::DiskWrite
            | Self::StartTime
            | Self::Fds
            | Self::Nice => Self::Pid,
        }
    }

    /// Ascending comparison between two rows for this column — delegated to
    /// the neutral [`taskmanager_application::process_sort`] comparator (the
    /// single source of axis semantics: typed availability, ASCII case
    /// folding for Name/User, `None`-before-`Some` ordering) through the
    /// shared [`super::sort_axis::sort_axis`] translation every frontend's
    /// tree/group projection also consumes. The caller is still responsible
    /// for applying [`SortDir`] and the stable pid tiebreaker, which composes
    /// to exactly the neutral
    /// [`compare_processes`](taskmanager_application::process_sort::compare_processes)
    /// ordering.
    pub(super) fn ascending(self, left: &ProcessItem, right: &ProcessItem) -> std::cmp::Ordering {
        taskmanager_application::process_sort::compare_axis(
            left,
            right,
            super::sort_axis::sort_axis(self),
        )
    }
}

/// Direction paired with the active [`SortCol`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    /// Default: matches the historic primary direction (CPU high-to-low).
    #[default]
    Desc,
}

impl SortDir {
    #[must_use]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Asc => "ascending",
            Self::Desc => "descending",
        }
    }
}

/// Column identity for the renderer-neutral inventory-table sorts
/// (Services / Startup / Users). Each table owns one active
/// `Option<(InfoSortCol, SortDir)>`; `None` preserves the provider order until
/// the user picks a column, so existing frontends keep their historic layout.
/// The label is the shell's single source of truth for header captions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoSortCol {
    /// Service display name / startup entry name / user logon name.
    Name,
    /// Service status / startup enabled state.
    Status,
    /// Users-table session identifier.
    Session,
    /// Users-table seat identifier.
    Seat,
}

impl InfoSortCol {
    /// Header caption for the column, localized through the shared catalog.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "common.name",
            Self::Status => "common.status",
            Self::Session => "users.session",
            Self::Seat => "users.seat",
        }
    }
}

/// The three inventory tables that carry a shared interactive sort. Used to
/// route a header click to the right sort slot; the variant list is the
/// iteration source for any consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoTable {
    Services,
    Startup,
    Users,
}

impl InfoTable {
    /// Every sortable inventory table. Enumerated here so frontends never
    /// duplicate the list.
    pub const ALL: [InfoTable; 3] = [InfoTable::Services, InfoTable::Startup, InfoTable::Users];
}

/// Deterministic rank for the Services sort (active before inactive before
/// failed before unknown) so the comparison never relies on string order.
const fn status_rank(status: ServiceStatus) -> u8 {
    match status {
        ServiceStatus::Active => 0,
        ServiceStatus::Inactive => 1,
        ServiceStatus::Failed => 2,
        ServiceStatus::Unknown => 3,
    }
}

/// Apply the shared sort direction to a base ascending ordering.
const fn apply_direction(ordering: std::cmp::Ordering, direction: SortDir) -> std::cmp::Ordering {
    match direction {
        SortDir::Asc => ordering,
        SortDir::Desc => ordering.reverse(),
    }
}

impl ShellApp {
    /// Advance the process-table sort to the next column, keeping the current
    /// direction. Resets the cursor so it never points at a different process
    /// after the rows are reordered.
    pub fn cycle_sort_column(&mut self) {
        let (column, direction) = self.process_sort;
        self.process_sort = (column.next(), direction);
        self.selected = 0;
        self.sync_application_selection();
        self.report_notice(
            FeedbackSource::Navigation,
            FeedbackSeverity::Info,
            FeedbackLifecycle::SHORT,
            format!(
                "Sorted by {} {}",
                self.process_sort.0.label(),
                self.process_sort.1.label()
            ),
        );
    }

    /// Flip the process-table sort direction (ascending ⇄ descending).
    pub fn toggle_sort_direction(&mut self) {
        let (column, direction) = self.process_sort;
        self.process_sort = (column, direction.toggle());
        self.selected = 0;
        self.sync_application_selection();
        self.report_notice(
            FeedbackSource::Navigation,
            FeedbackSeverity::Info,
            FeedbackLifecycle::SHORT,
            format!(
                "Sorted by {} {}",
                self.process_sort.0.label(),
                self.process_sort.1.label()
            ),
        );
    }

    /// Set the process-table sort column to `column` directly (e.g. when a
    /// column header is clicked), keeping the current direction — the same
    /// post-conditions as [`Self::cycle_sort_column`]. Frontends should prefer
    /// this over mutating the public [`process_sort`](ShellApp::process_sort)
    /// field so the cursor reset + selection sync + status stay consistent.
    pub fn set_sort_column(&mut self, column: SortCol) {
        self.process_sort = (column, self.process_sort.1);
        self.selected = 0;
        self.sync_application_selection();
        self.report_notice(
            FeedbackSource::Navigation,
            FeedbackSeverity::Info,
            FeedbackLifecycle::SHORT,
            format!(
                "Sorted by {} {}",
                self.process_sort.0.label(),
                self.process_sort.1.label()
            ),
        );
    }

    /// Route a header click on one of the inventory tables to its own sort
    /// slot. Clicking the already-active column toggles the direction; any
    /// other column switches directly. Selection resets like the process
    /// table so the highlight stays in bounds of the new row order. The
    /// status line names the column AND the direction, matching the
    /// process-table sort feedback so every frontend reads the same shape.
    pub fn set_info_sort(&mut self, table: InfoTable, column: InfoSortCol) {
        let slot = match table {
            InfoTable::Services => &mut self.services_sort,
            InfoTable::Startup => &mut self.startup_sort,
            InfoTable::Users => &mut self.sessions_sort,
        };
        match slot {
            Some((active, direction)) if *active == column => {
                *direction = direction.toggle();
            }
            _ => *slot = Some((column, SortDir::Asc)),
        }
        self.selected = 0;
        // Both match arms leave the slot populated; read it back for the
        // status so a `S`-flip on a picked column reports the new direction.
        let notice = slot.as_ref().map(|(active, direction)| {
            format!("Sorted by {} {}", active.label(), direction.label())
        });
        if let Some(notice) = notice {
            self.report_notice(
                FeedbackSource::Navigation,
                FeedbackSeverity::Info,
                FeedbackLifecycle::SHORT,
                notice,
            );
        }
    }

    /// The sortable columns of one inventory table, in cycle order. Each table
    /// only cycles its meaningful keys (a Services table cannot sort by seat,
    /// a Users table cannot sort by service status), so the keyboard cycle
    /// never lands on a column that renders as constant.
    const fn info_cycle(table: InfoTable) -> &'static [InfoSortCol] {
        match table {
            InfoTable::Services => &[InfoSortCol::Name, InfoSortCol::Status],
            InfoTable::Startup => &[InfoSortCol::Name, InfoSortCol::Status],
            InfoTable::Users => &[InfoSortCol::Name, InfoSortCol::Session, InfoSortCol::Seat],
        }
    }

    /// Advance one inventory table's sort to the next meaningful column
    /// (`s` key on a non-Applications table page): from the currently active
    /// column to the next in the table's cycle, wrapping; when no column is
    /// active (provider order), start at the first cycle column. Clicking the
    /// same column twice flips the direction through [`Self::set_info_sort`]
    /// semantics. Selection resets like every sort path.
    pub fn cycle_info_sort_column(&mut self, table: InfoTable) {
        let cycle = Self::info_cycle(table);
        let active = match table {
            InfoTable::Services => self.services_sort,
            InfoTable::Startup => self.startup_sort,
            InfoTable::Users => self.sessions_sort,
        };
        let next = match active {
            Some((column, _)) => {
                let index = cycle
                    .iter()
                    .position(|candidate| *candidate == column)
                    .unwrap_or(0);
                cycle[(index + 1) % cycle.len()]
            }
            None => cycle[0],
        };
        self.set_info_sort(table, next);
    }

    /// Flip one inventory table's sort direction without changing the column
    /// (`S` key on a non-Applications table page). Mirrors the process-table
    /// `toggle_sort_direction`; a table still in provider order (no column
    /// picked yet) starts from the first cycle column. The status line names
    /// the direction like the process table's.
    pub fn toggle_info_sort_direction(&mut self, table: InfoTable) {
        let cycle = Self::info_cycle(table);
        let slot = match table {
            InfoTable::Services => &mut self.services_sort,
            InfoTable::Startup => &mut self.startup_sort,
            InfoTable::Users => &mut self.sessions_sort,
        };
        match slot {
            Some((_, direction)) => *direction = direction.toggle(),
            None => *slot = Some((cycle[0], SortDir::Desc)),
        }
        self.selected = 0;
        let notice = slot.as_ref().map(|(column, direction)| {
            format!("Sorted by {} {}", column.label(), direction.label())
        });
        if let Some(notice) = notice {
            self.report_notice(
                FeedbackSource::Navigation,
                FeedbackSeverity::Info,
                FeedbackLifecycle::SHORT,
                notice,
            );
        }
    }

    /// The Services-table rows in the shared sort order (provider order when
    /// the user never picked a column). Every frontend projects from this
    /// accessor so selection indexes always map to the same visible order.
    #[must_use]
    pub fn sorted_services(&self) -> Vec<&ServiceItem> {
        let services = self.data.services.as_deref().unwrap_or_default();
        self.sorted_service_indices()
            .into_iter()
            .filter_map(|index| services.get(index))
            .collect()
    }

    /// Resolve one Services-table visual row — the order every renderer
    /// projects through [`Self::sorted_services`] — back to its provider row.
    /// This is the single "row N → target" translation for the page's menus
    /// and keyboard actions, so an active sort can never make an action hit a
    /// different row than the one the renderer highlighted. Mirrors the
    /// process table's [`Self::visible_process_at`].
    #[must_use]
    pub fn sorted_service_at(&self, index: usize) -> Option<&ServiceItem> {
        let services = self.data.services.as_deref()?;
        self.sorted_service_indices()
            .get(index)
            .and_then(|&source| services.get(source))
    }

    /// The provider-order indices for the Services table's canonical sort.
    /// Frontends that need owned row facts can project directly from these
    /// indices without allocating a parallel `Vec<&ServiceItem>` or recovering
    /// indices with an O(N²) pointer search.
    #[must_use]
    pub fn sorted_service_indices(&self) -> Vec<usize> {
        let services = self.data.services.as_deref().unwrap_or_default();
        let mut indices: Vec<usize> = (0..services.len()).collect();
        if let Some((column, direction)) = self.services_sort {
            indices.sort_by(|&left, &right| {
                let left = &services[left];
                let right = &services[right];
                let ordering = match column {
                    InfoSortCol::Name => left.name.cmp(&right.name),
                    InfoSortCol::Status => status_rank(left.status).cmp(&status_rank(right.status)),
                    InfoSortCol::Session | InfoSortCol::Seat => std::cmp::Ordering::Equal,
                };
                apply_direction(ordering, direction)
            });
        }
        indices
    }

    /// The Startup-table rows in the shared sort order; semantics mirror
    /// [`ShellApp::sorted_services`]. Enabled-first matches the gpui fixed
    /// sort's historic primary key.
    #[must_use]
    pub fn sorted_startup_entries(&self) -> Vec<&StartupEntry> {
        let entries = self.data.startup_entries.as_deref().unwrap_or_default();
        self.sorted_startup_indices()
            .into_iter()
            .filter_map(|index| entries.get(index))
            .collect()
    }

    /// Resolve one Startup-table visual row back to its provider row through
    /// the canonical sort; the single "row N → target" translation (see
    /// [`Self::sorted_service_at`]).
    #[must_use]
    pub fn sorted_startup_entry_at(&self, index: usize) -> Option<&StartupEntry> {
        let entries = self.data.startup_entries.as_deref()?;
        self.sorted_startup_indices()
            .get(index)
            .and_then(|&source| entries.get(source))
    }

    /// The provider-order indices for the Startup table's canonical sort.
    #[must_use]
    pub fn sorted_startup_indices(&self) -> Vec<usize> {
        let entries = self.data.startup_entries.as_deref().unwrap_or_default();
        let mut indices: Vec<usize> = (0..entries.len()).collect();
        if let Some((column, direction)) = self.startup_sort {
            indices.sort_by(|&left, &right| {
                let left = &entries[left];
                let right = &entries[right];
                let ordering = match column {
                    InfoSortCol::Name => left.name.cmp(&right.name),
                    InfoSortCol::Status => right.enabled.cmp(&left.enabled),
                    InfoSortCol::Session | InfoSortCol::Seat => std::cmp::Ordering::Equal,
                };
                apply_direction(ordering, direction)
            });
        }
        indices
    }

    /// The Users-table rows in the shared sort order; semantics mirror
    /// [`ShellApp::sorted_services`].
    #[must_use]
    pub fn sorted_sessions(&self) -> Vec<&SessionItem> {
        let sessions = self.data.sessions.as_deref().unwrap_or_default();
        self.sorted_session_indices()
            .into_iter()
            .filter_map(|index| sessions.get(index))
            .collect()
    }

    /// Resolve one Users-table visual row back to its provider row through
    /// the canonical sort; the single "row N → target" translation (see
    /// [`Self::sorted_service_at`]).
    #[must_use]
    pub fn sorted_session_at(&self, index: usize) -> Option<&SessionItem> {
        let sessions = self.data.sessions.as_deref()?;
        self.sorted_session_indices()
            .get(index)
            .and_then(|&source| sessions.get(source))
    }

    /// The provider-order indices for the Users table's canonical sort.
    #[must_use]
    pub fn sorted_session_indices(&self) -> Vec<usize> {
        let sessions = self.data.sessions.as_deref().unwrap_or_default();
        let mut indices: Vec<usize> = (0..sessions.len()).collect();
        if let Some((column, direction)) = self.sessions_sort {
            indices.sort_by(|&left, &right| {
                let left = &sessions[left];
                let right = &sessions[right];
                let ordering = match column {
                    InfoSortCol::Name => left.user.cmp(&right.user),
                    InfoSortCol::Session => left.id.cmp(&right.id),
                    InfoSortCol::Seat => left.seat.cmp(&right.seat),
                    InfoSortCol::Status => std::cmp::Ordering::Equal,
                };
                apply_direction(ordering, direction)
            });
        }
        indices
    }
}

/// Pairwise parity: the shell's `visible_processes` ordering vs the neutral
/// `taskmanager_application::process_sort` comparator on the shared fixture
/// (the shell applies `SortDir` + its stable pid tie-break around
/// [`SortCol::ascending`], which must compose to exactly the neutral
/// `compare_processes` ordering for every column and direction).
#[cfg(test)]
#[path = "../../tests/headless/shell_app_sorting_sort_parity_tests.rs"]
mod sort_parity_tests;
