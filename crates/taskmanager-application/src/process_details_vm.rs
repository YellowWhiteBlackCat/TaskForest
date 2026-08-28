//! Neutral process-details row ViewModel (ARCH.md §8.1 data layer).
//!
//! The single fold from a typed [`ProcessItem`] observation to the
//! label/value rows every frontend's process-details panel and
//! process-properties dialog renders. Before this module the TUI
//! (`ui/process_details.rs` + `ui/process_properties.rs`), the Iced
//! properties overlay (`ui/overlays/process_details.rs`), and the GPUI
//! Properties dialog (`root/chrome.rs`) each folded the same typed
//! observations independently, so the field sets and the missing-value
//! semantics could (and did) drift. Frontends now consume
//! [`crate::process_details_vm::process_details_rows`] and only supply per-end
//! labels and layout — the render structure stays owned by each toolkit.
//! Native local-time rules are an explicit fold input; the compatibility-
//! shaped helper supplies `Unsupported`, so no caller can accidentally read
//! or infer the host zone.
//!
//! # Field vocabulary and end coverage
//!
//! | field            | TUI panel | TUI modal | GPUI dialog | Iced overlay |
//! |------------------|-----------|-----------|-------------|--------------|
//! | `Name`           | yes       | overview  | overview + command | overview + command |
//! | `Pid`            | yes       | overview  | overview    | overview     |
//! | `ParentPid`      | —         | overview  | overview    | overview     |
//! | `User`           | yes       | overview  | overview    | overview     |
//! | `Status`         | yes       | overview  | overview    | overview     |
//! | `Cpu`            | combined  | performance current | performance current | overview + performance caption |
//! | `Memory`         | combined  | performance current | performance current | overview + performance caption |
//! | `Pss`            | combined  | —         | —           | —            |
//! | `Swap`           | combined  | —         | —           | —            |
//! | `Threads`        | combined  | overview  | overview    | overview     |
//! | `Fds`            | combined  | —         | —           | overview     |
//! | `Nice`           | yes       | —         | —           | overview     |
//! | `StartTime`      | yes       | overview  | overview    | overview     |
//! | `CpuTime`        | yes       | —         | —           | overview     |
//! | `DiskReadRate`   | yes       | performance current | performance current | performance caption |
//! | `DiskWriteRate`  | yes       | performance current | performance current | performance caption |
//! | `DiskReadTotal`  | —         | —         | —           | overview     |
//! | `DiskWriteTotal` | —         | —         | —           | overview     |
//! | `Exe`            | yes       | command   | command     | command      |
//! | `Cmdline`        | yes       | command   | command     | command      |
//!
//! Peaks are NOT folded here: they need a history window the row fold does
//! not take, so each end keeps its (shared `peak_of`-based) peak fold and
//! only the CURRENT values come from this VM.
//!
//! # Adjudicated formats
//!
//! * Byte quantities go through the neutral [`crate::units`] ladder with the
//!   caller's `core::units::UnitPreferences`: memory family for `Memory`/`Pss`/`Swap`,
//!   drive family for the four disk fields (rates append `/s`). With the
//!   Mission-Center-parity default (bytes, base-2) this matches the
//!   historical TUI/Iced spelling byte-for-byte and converges the GPUI
//!   dialog's previously hardcoded decimal MB readouts onto the shared
//!   ladder.
//! * `CpuTime` keeps the majority duration spelling (`01h 01m` /
//!   `1d 01h 00m`), pinned to the `taskmanager-shell` presentation
//!   contract.
//! * `Nice` keeps the majority signed spelling (`+10` / `0` / `-5`).
//! * `StartTime` renders the full UTC wall-clock `YYYY-MM-DD HH:MM:SS`
//!   (the GPUI dialog spelling, previously the only end showing a complete
//!   timestamp; the TUI showed raw epoch seconds and Iced a local
//!   time-of-day). The algorithm moved here from GPUI `root/chrome.rs`,
//!   which now consumes this VM — the implementation stays single-source.
//! * `Cpu` renders `{:.1}%`; per-end column padding stays a layout concern.
//! * A missing observation folds to
//!   [`crate::process_details_vm::DetailValue::Missing`] — never a fabricated
//!   `0`, `0.0%`, or empty string. A whitespace-only `Cmdline`
//!   is missing (the majority TUI/Iced dash-on-empty semantics).

use crate::model::ProcessItem;
use crate::units::{QuantityFamily, UnitPreferences, format_memory, format_quantity};

/// One pre-folded properties row: the vocabulary field plus its folded value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessDetailsRowVm {
    pub field: ProcessDetailsField,
    pub value: DetailValue,
}

/// The process-details field vocabulary — the single list every frontend's
/// label table keys on. Variant docs name the ends that render the field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessDetailsField {
    /// Process name. All ends.
    Name,
    /// Process id. All ends.
    Pid,
    /// Parent process id. TUI/GPUI/Iced overview.
    ParentPid,
    /// Owner label. All ends.
    User,
    /// Process state string. All ends.
    Status,
    /// CPU percentage, `12.5%`. TUI panel (combined), performance tabs, Iced overview.
    Cpu,
    /// Resident memory. TUI panel (combined), performance tabs, Iced overview.
    Memory,
    /// Hybrid proportional set size. TUI panel (combined) only.
    Pss,
    /// Swap charged to the process. TUI panel (combined) only.
    Swap,
    /// Thread count. TUI panel (combined), overviews.
    Threads,
    /// Open file-descriptor count. TUI panel (combined), Iced overview.
    Fds,
    /// Scheduling niceness, signed. TUI panel, Iced overview.
    Nice,
    /// Start time, UTC `YYYY-MM-DD HH:MM:SS`. All ends.
    StartTime,
    /// Cumulative CPU time, `01h 01m` / `1d 01h 00m`. TUI panel, Iced overview.
    CpuTime,
    /// Disk read rate (`/s` suffix). TUI panel, performance tabs.
    DiskReadRate,
    /// Disk write rate (`/s` suffix). TUI panel, performance tabs.
    DiskWriteRate,
    /// Cumulative disk read bytes. Iced overview.
    DiskReadTotal,
    /// Cumulative disk write bytes. Iced overview.
    DiskWriteTotal,
    /// Executable path. Command tabs, TUI panel.
    Exe,
    /// Full command line. Command tabs, TUI panel.
    Cmdline,
}

impl ProcessDetailsField {
    /// Every field in the canonical row order — the single variant list
    /// ([`process_details_rows`] emits exactly this sequence).
    pub const ALL: [Self; 20] = [
        Self::Name,
        Self::Pid,
        Self::ParentPid,
        Self::User,
        Self::Status,
        Self::Cpu,
        Self::Memory,
        Self::Pss,
        Self::Swap,
        Self::Threads,
        Self::Fds,
        Self::Nice,
        Self::StartTime,
        Self::CpuTime,
        Self::DiskReadRate,
        Self::DiskWriteRate,
        Self::DiskReadTotal,
        Self::DiskWriteTotal,
        Self::Exe,
        Self::Cmdline,
    ];

    /// The stable field id — the label-vocabulary key frontends map their
    /// own localized labels through.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Pid => "pid",
            Self::ParentPid => "parent_pid",
            Self::User => "user",
            Self::Status => "status",
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Pss => "pss",
            Self::Swap => "swap",
            Self::Threads => "threads",
            Self::Fds => "fds",
            Self::Nice => "nice",
            Self::StartTime => "start_time",
            Self::CpuTime => "cpu_time",
            Self::DiskReadRate => "disk_read_rate",
            Self::DiskWriteRate => "disk_write_rate",
            Self::DiskReadTotal => "disk_read_total",
            Self::DiskWriteTotal => "disk_write_total",
            Self::Exe => "exe",
            Self::Cmdline => "cmdline",
        }
    }
}

/// The folded value of one row: a display string, or the shared missing
/// marker (an unavailable observation never fabricates a zero).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DetailValue {
    Text(String),
    Missing,
}

impl DetailValue {
    /// The shared missing sentinel for lookups on absent rows.
    pub const MISSING: Self = Self::Missing;

    /// The folded text, or `None` when the observation is missing.
    #[must_use]
    pub const fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            Self::Missing => None,
        }
    }

    /// The folded text, or `dash` when the observation is missing — the
    /// single place a frontend's dash spelling plugs in.
    #[must_use]
    pub fn text_or<'a>(&'a self, dash: &'a str) -> &'a str {
        match self {
            Self::Text(text) => text.as_str(),
            Self::Missing => dash,
        }
    }

    /// Whether this row's observation is missing.
    #[must_use]
    pub const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

/// Fold one [`ProcessItem`] into its properties rows, exactly
/// [`ProcessDetailsField::ALL`] long and ordered. Every byte quantity goes
/// through the neutral unit ladder under `units`; every unavailable
/// observation folds to [`DetailValue::Missing`].
#[must_use]
pub fn process_details_rows(
    item: &ProcessItem,
    units: &UnitPreferences,
) -> Vec<ProcessDetailsRowVm> {
    process_details_rows_with_local_time(
        item,
        units,
        &crate::LocalTimeRulesObservation::unsupported(0),
    )
}

/// Fold process details with a composition-injected local-time snapshot.
#[must_use]
pub fn process_details_rows_with_local_time(
    item: &ProcessItem,
    units: &UnitPreferences,
    local_time_rules: &crate::LocalTimeRulesObservation,
) -> Vec<ProcessDetailsRowVm> {
    let text = |value: Option<String>| value.map_or(DetailValue::Missing, DetailValue::Text);
    vec![
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Name,
            value: DetailValue::Text(item.name.clone()),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Pid,
            value: DetailValue::Text(item.pid.to_string()),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::ParentPid,
            value: text(item.parent_pid.map(|pid| pid.to_string())),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::User,
            value: text(item.current_user()),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Status,
            value: DetailValue::Text(item.status.clone()),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Cpu,
            value: text(item.current_cpu_percentage().map(format_cpu_percent)),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Memory,
            value: text(
                item.current_memory_bytes()
                    .map(|bytes| format_memory(bytes, units)),
            ),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Pss,
            value: text(
                item.current_memory_pss_bytes()
                    .map(|bytes| format_memory(bytes, units)),
            ),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Swap,
            value: text(
                item.current_swap_bytes()
                    .map(|bytes| format_memory(bytes, units)),
            ),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Threads,
            value: text(item.current_threads().map(|threads| threads.to_string())),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Fds,
            value: text(item.current_fds().map(|fds| fds.to_string())),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Nice,
            value: text(item.current_nice().map(format_nice)),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::StartTime,
            value: text(
                item.current_start_time_secs()
                    .filter(|seconds| *seconds != 0)
                    .and_then(|seconds| format_local_timestamp_seconds(seconds, local_time_rules)),
            ),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::CpuTime,
            value: text(item.current_cpu_time_secs().map(format_duration_hm)),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::DiskReadRate,
            value: text(
                item.current_disk_read_bytes_per_sec()
                    .map(|bytes| format_quantity(bytes, QuantityFamily::Drive, true, units)),
            ),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::DiskWriteRate,
            value: text(
                item.current_disk_write_bytes_per_sec()
                    .map(|bytes| format_quantity(bytes, QuantityFamily::Drive, true, units)),
            ),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::DiskReadTotal,
            value: text(
                item.current_disk_read_bytes_total()
                    .map(|bytes| format_quantity(bytes, QuantityFamily::Drive, false, units)),
            ),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::DiskWriteTotal,
            value: text(
                item.current_disk_write_bytes_total()
                    .map(|bytes| format_quantity(bytes, QuantityFamily::Drive, false, units)),
            ),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Exe,
            value: text(
                item.current_exe_path()
                    .map(|path| path.display().to_string()),
            ),
        },
        ProcessDetailsRowVm {
            field: ProcessDetailsField::Cmdline,
            value: if item.cmdline.trim().is_empty() {
                DetailValue::Missing
            } else {
                DetailValue::Text(item.cmdline.clone())
            },
        },
    ]
}

/// Borrow one field's folded value from a rendered row set — [`DetailValue::MISSING`]
/// when the field is absent (callers passing [`process_details_rows`] output
/// always find every [`ProcessDetailsField::ALL`] entry).
#[must_use]
pub fn detail_value(rows: &[ProcessDetailsRowVm], field: ProcessDetailsField) -> &DetailValue {
    rows.iter()
        .find(|row| row.field == field)
        .map_or(&DetailValue::MISSING, |row| &row.value)
}

/// `{:.1}%` — the majority CPU spelling (alignment padding stays a layout concern).
fn format_cpu_percent(cpu: f32) -> String {
    format!("{cpu:.1}%")
}

/// `+10` / `0` / `-5` — the majority niceness spelling, pinned to the
/// `taskmanager-shell::presentation::optional_nice` string contract.
fn format_nice(nice: i32) -> String {
    if nice > 0 {
        format!("+{nice}")
    } else {
        nice.to_string()
    }
}

/// `00h 01m` / `1d 01h 00m` — the majority CPU-time duration spelling,
/// pinned to the `taskmanager-shell::presentation::duration` string contract.
fn format_duration_hm(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours:02}h {minutes:02}m")
    } else {
        format!("{hours:02}h {minutes:02}m")
    }
}

/// Format a non-zero epoch second using only injected local-time rules.
/// Unsupported, expired, or otherwise unavailable rules return `None`.
#[must_use]
pub fn format_local_timestamp_seconds(
    seconds: u64,
    rules: &crate::LocalTimeRulesObservation,
) -> Option<String> {
    let seconds = i64::try_from(seconds).ok()?;
    let local = rules.date_time_at(seconds)?;
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        local.year(),
        local.month(),
        local.day(),
        local.hour(),
        local.minute(),
        local.second()
    ))
}

#[cfg(test)]
#[path = "../tests/headless/application_process_details_vm_tests.rs"]
mod tests;
