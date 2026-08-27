//! The single `SortCol → ProcessSortAxis` mapping every frontend consumes
//! (ADR-027 single source of ordering semantics).
//!
//! The tree/flat/category row sorts in GPUI, Iced, and the TUI all project
//! through the neutral
//! [`taskmanager_application::process_sort::compare_processes`] comparator;
//! this module is the only place a shell [`SortCol`] is translated into that
//! comparator's [`ProcessSortAxis`]. The match is compiler-exhaustive: adding
//! a [`SortCol`] variant without an axis arm is a build error, so a column
//! can never silently miss the neutral comparator (the same guarantee the
//! per-frontend bridges used to carry individually).
//!
//! Canonical fallback rule: every column maps to its REAL axis — the axes
//! exist for all fifteen columns (`User`, `State`→`Status`, `Threads`,
//! `StartTime`, `Fds`, `Nice` included), so no column degrades to `Name`,
//! `Pid`, or `CpuUsage` in tree/group modes anymore. The three frontends'
//! historic divergent fallbacks (gpui: real axes; iced: `Name` + local
//! comparators; tui: `Pid`/`CpuUsage`) are replaced by this one table.
//!
//! [`aggregate_sort_key`] carries the ONE documented fallback that remains:
//! group headers sort per [`taskmanager_application::AppGroup`] aggregates,
//! which genuinely lack per-axis data for some columns.

use super::sorting::SortCol;
use taskmanager_application::ProcessSortKey;
use taskmanager_application::process_sort::ProcessSortAxis;

/// Map a process-table column onto the neutral comparator axis. The single
/// exhaustive translation — consumers are shell `SortCol::ascending`, the
/// gpui processes view, the iced tree/category projection, and the TUI tree
/// projection.
#[must_use]
pub const fn sort_axis(column: SortCol) -> ProcessSortAxis {
    match column {
        SortCol::Pid => ProcessSortAxis::Pid,
        SortCol::Name => ProcessSortAxis::Name,
        SortCol::Cpu => ProcessSortAxis::Cpu,
        SortCol::Memory => ProcessSortAxis::Memory,
        SortCol::Pss => ProcessSortAxis::Pss,
        SortCol::Swap => ProcessSortAxis::Swap,
        SortCol::User => ProcessSortAxis::User,
        // The shell's `State` column and the historic gpui `Status` column
        // are the same provider status token; the neutral axis keeps the
        // `Status` spelling.
        SortCol::State => ProcessSortAxis::Status,
        SortCol::Threads => ProcessSortAxis::Threads,
        SortCol::CpuTime => ProcessSortAxis::CpuTime,
        SortCol::DiskRead => ProcessSortAxis::DiskRead,
        SortCol::DiskWrite => ProcessSortAxis::DiskWrite,
        SortCol::StartTime => ProcessSortAxis::StartTime,
        SortCol::Fds => ProcessSortAxis::Fds,
        SortCol::Nice => ProcessSortAxis::Nice,
    }
}

/// Map a column onto the legacy data-layer aggregate key used by
/// [`taskmanager_application::sort_apps`] (group headers / type headers).
/// This is the one place a column without aggregate meaning degrades, and
/// the rule is fixed and documented so no frontend re-decides it:
///
/// - `Pid` → `Pid` (the group's `main_pid`);
/// - `Name` → `Name` (the group identity);
/// - `Cpu`/`CpuTime` → `CpuUsage` — the only CPU aggregate `AppGroup`
///   carries; cumulative cpu-time rides the usage ranking as its proxy;
/// - `Memory`/`Pss`/`Swap` → `Memory` — the typed fallback only; frontends
///   with a metric-sum path intercept these columns before `sort_apps`;
/// - `DiskRead`/`DiskWrite` → their keys (legacy `sort_apps` folds both onto
///   `total_memory_bytes`);
/// - `User`/`State`/`Threads`/`StartTime`/`Fds`/`Nice` → `Name`: an
///   `AppGroup` has no per-user/per-state/per-thread aggregate, so the group
///   identity is the deterministic fallback.
#[must_use]
pub const fn aggregate_sort_key(column: SortCol) -> ProcessSortKey {
    match column {
        SortCol::Pid => ProcessSortKey::Pid,
        SortCol::Name => ProcessSortKey::Name,
        SortCol::Cpu | SortCol::CpuTime => ProcessSortKey::CpuUsage,
        SortCol::Memory | SortCol::Pss | SortCol::Swap => ProcessSortKey::Memory,
        SortCol::DiskRead => ProcessSortKey::DiskRead,
        SortCol::DiskWrite => ProcessSortKey::DiskWrite,
        SortCol::User
        | SortCol::State
        | SortCol::Threads
        | SortCol::StartTime
        | SortCol::Fds
        | SortCol::Nice => ProcessSortKey::Name,
    }
}
