//! Neutral process-table sort comparator shared by every frontend.
//!
//! The shell ([`crate::process_sort`] consumer in `taskmanager-shell`) and the
//! GPUI processes view each keep their own `SortCol` enum (their headers,
//! cycles, and persistence are frontend concerns), but the ORDERING SEMANTICS
//! live here exactly once: every frontend maps its column enum onto
//! [`crate::process_sort::ProcessSortAxis`] with a compiler-exhaustive match and delegates the
//! comparison. CPU uses [`ProcessItem::current_cpu_percentage`] with total
//! `f32` ordering; memory sorts resident RSS through
//! [`ProcessItem::current_memory_bytes`]; text uses the shared ASCII
//! case-insensitive comparator; every optional scalar sorts as `Option`; and
//! PID is the direction-independent tie-break.
//!
//! The adjudication rule throughout is honesty over fabrication: a value the
//! provider did not measure sorts as `None` (standard [`Option`] ordering —
//! `None` before `Some` ascending, therefore LAST in a descending sort, so a
//! provider failure never wins the top of a descending list), and independent
//! measurement kinds (RSS vs PSS vs swap) never stand in for each other.
//!
//! Everything here is toolkit-neutral, allocation-free, and panic-free; it
//! depends only on [`ProcessItem`] and the shared text comparator.

use std::cmp::Ordering;

use crate::ProcessItem;
use crate::text::cmp_ascii_ci;

/// One sortable projection of a [`ProcessItem`] — the neutral vocabulary every
/// frontend's column enum maps onto. Each variant documents the field it reads
/// and how a missing observation orders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProcessSortAxis {
    /// Process id (`ProcessItem::pid`, always available).
    Pid,
    /// Process name, compared ASCII case-insensitively (`"Alpha" == "alpha"`);
    /// no typed availability — the raw string always participates.
    Name,
    /// Instantaneous CPU percentage via
    /// [`ProcessItem::current_cpu_percentage`]. `None` (stale, unavailable, or
    /// an untrusted legacy sentinel) sorts before every measured value
    /// ascending; measured `f32`s compare with `total_cmp` (NaN ordered above
    /// every finite value, deterministically).
    Cpu,
    /// Resident memory via [`ProcessItem::current_memory_bytes`] (typed RSS).
    /// Deliberately NOT the PSS-preferred display fold: PSS is its own axis,
    /// so an unavailable PSS observation can never sort as RSS. `None` sorts
    /// first ascending.
    Memory,
    /// Hybrid proportional-set-size memory via
    /// [`ProcessItem::current_memory_pss_bytes`] — no RSS fallback. `None`
    /// sorts first ascending.
    Pss,
    /// Per-process swap via [`ProcessItem::current_swap_bytes`] — never folded
    /// into RSS or PSS. `None` sorts first ascending.
    Swap,
    /// Owner name, compared ASCII case-insensitively (`"Root" == "root"`).
    User,
    /// Process state token (`ProcessItem::status`) compared as raw bytes,
    /// case-sensitively — these are short provider tokens ("R"/"S"/"Running"),
    /// not user-facing display names; both frontends already compared them
    /// raw. (Shell's `State` column and gpui's `Status` column both map here.)
    Status,
    /// Thread count via [`ProcessItem::current_threads`]. `None` sorts first
    /// ascending.
    Threads,
    /// Cumulative CPU time in seconds via
    /// [`ProcessItem::current_cpu_time_secs`]. `None` sorts first ascending.
    CpuTime,
    /// Disk read rate via [`ProcessItem::current_disk_read_bytes_per_sec`].
    /// `None` sorts first ascending.
    DiskRead,
    /// Disk write rate via [`ProcessItem::current_disk_write_bytes_per_sec`].
    /// `None` sorts first ascending.
    DiskWrite,
    /// Wall-clock start time in seconds via
    /// [`ProcessItem::current_start_time_secs`]. `None` sorts first ascending.
    StartTime,
    /// Open file-descriptor count via [`ProcessItem::current_fds`]. `None`
    /// sorts first ascending.
    Fds,
    /// Scheduling nice value (-20..19) via [`ProcessItem::current_nice`].
    /// `None` sorts first ascending.
    Nice,
}

impl ProcessSortAxis {
    /// Every axis, in declaration order. The iteration source for consumers
    /// and tests — never duplicate the list elsewhere.
    pub const ALL: [ProcessSortAxis; 15] = [
        ProcessSortAxis::Pid,
        ProcessSortAxis::Name,
        ProcessSortAxis::Cpu,
        ProcessSortAxis::Memory,
        ProcessSortAxis::Pss,
        ProcessSortAxis::Swap,
        ProcessSortAxis::User,
        ProcessSortAxis::Status,
        ProcessSortAxis::Threads,
        ProcessSortAxis::CpuTime,
        ProcessSortAxis::DiskRead,
        ProcessSortAxis::DiskWrite,
        ProcessSortAxis::StartTime,
        ProcessSortAxis::Fds,
        ProcessSortAxis::Nice,
    ];
}

/// `Option<f32>` ordering with a total order on the inner value: `None`
/// before `Some` ascending, and measured values compared with `total_cmp` so
/// NaN is placed deterministically instead of comparing unordered.
fn option_f32_cmp(left: Option<f32>, right: Option<f32>) -> Ordering {
    match (left, right) {
        (Some(left_value), Some(right_value)) => left_value.total_cmp(&right_value),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Raw ascending comparison on one axis, WITHOUT direction and WITHOUT the
/// pid tie-break. This is the delegation point for pipelines that apply the
/// direction and tie-break themselves (the shell's
/// `visible_processes_indices` applies `SortDir` then its stable pid
/// tie-break around this value, which composes to exactly
/// [`compare_processes`]).
#[must_use]
pub fn compare_axis(left: &ProcessItem, right: &ProcessItem, axis: ProcessSortAxis) -> Ordering {
    match axis {
        ProcessSortAxis::Pid => left.pid.cmp(&right.pid),
        ProcessSortAxis::Name => cmp_ascii_ci(&left.name, &right.name),
        ProcessSortAxis::Cpu => option_f32_cmp(
            left.current_cpu_percentage(),
            right.current_cpu_percentage(),
        ),
        ProcessSortAxis::Memory => left
            .current_memory_bytes()
            .cmp(&right.current_memory_bytes()),
        ProcessSortAxis::Pss => left
            .current_memory_pss_bytes()
            .cmp(&right.current_memory_pss_bytes()),
        ProcessSortAxis::Swap => left.current_swap_bytes().cmp(&right.current_swap_bytes()),
        ProcessSortAxis::User => cmp_ascii_ci(
            left.current_user().as_deref().unwrap_or_default(),
            right.current_user().as_deref().unwrap_or_default(),
        ),
        ProcessSortAxis::Status => left.status.cmp(&right.status),
        ProcessSortAxis::Threads => left.current_threads().cmp(&right.current_threads()),
        ProcessSortAxis::CpuTime => left
            .current_cpu_time_secs()
            .cmp(&right.current_cpu_time_secs()),
        ProcessSortAxis::DiskRead => left
            .current_disk_read_bytes_per_sec()
            .cmp(&right.current_disk_read_bytes_per_sec()),
        ProcessSortAxis::DiskWrite => left
            .current_disk_write_bytes_per_sec()
            .cmp(&right.current_disk_write_bytes_per_sec()),
        ProcessSortAxis::StartTime => left
            .current_start_time_secs()
            .cmp(&right.current_start_time_secs()),
        ProcessSortAxis::Fds => left.current_fds().cmp(&right.current_fds()),
        ProcessSortAxis::Nice => left.current_nice().cmp(&right.current_nice()),
    }
}

/// The complete neutral ordering between two processes on one axis: the
/// axis comparison directed by `ascending`, then the DIRECTION-INDEPENDENT
/// pid-ascending tie-break (equal rows keep a deterministic order in both
/// directions and across refresh ticks). Frontends call this directly inside
/// `sort_by`; passing `ascending = false` is NOT the same as reversing the
/// result — the tie-break deliberately does not flip.
#[must_use]
pub fn compare_processes(
    left: &ProcessItem,
    right: &ProcessItem,
    axis: ProcessSortAxis,
    ascending: bool,
) -> Ordering {
    let primary = compare_axis(left, right, axis);
    let directed = if ascending {
        primary
    } else {
        primary.reverse()
    };
    directed.then_with(|| left.pid.cmp(&right.pid))
}

#[cfg(test)]
#[path = "../tests/headless/application_process_sort_tests.rs"]
mod tests;
