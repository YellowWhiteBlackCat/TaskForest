//! Small typed process-specific procfs and POSIX clock readers.

use std::fs;

#[cfg(target_os = "linux")]
use nix::errno::Errno;
#[cfg(target_os = "linux")]
use nix::unistd::{SysconfVar, sysconf};
use taskforest_core::{FailureKind, FrozenProcessIdentity};

use super::tree::io_failure;
use taskforest_core::ProcessSchedulingPolicy;

/// The scheduling state byte of one `/proc/<pid>/stat` read, typed at the
/// procfs boundary. The label vocabulary is the product's process-status
/// column; an unrecognized or absent byte is [`ProcessState::Other`], never a
/// guessed state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcessState {
    Running,
    Sleeping,
    Idle,
    UninterruptibleDiskSleep,
    Zombie,
    Stopped,
    Tracing,
    Dead,
    Wakekill,
    Waking,
    Parked,
    Other,
}

impl ProcessState {
    /// Decode the kernel state byte (`proc(5)` § /proc/pid/stat, field 3).
    pub(super) fn from_stat_field(field: Option<&str>) -> Self {
        match field.and_then(|field| field.chars().next()) {
            Some('R') => Self::Running,
            Some('S') => Self::Sleeping,
            Some('I') => Self::Idle,
            Some('D') => Self::UninterruptibleDiskSleep,
            Some('Z') => Self::Zombie,
            Some('T') => Self::Stopped,
            Some('t') => Self::Tracing,
            Some('X' | 'x') => Self::Dead,
            Some('K') => Self::Wakekill,
            Some('W') => Self::Waking,
            Some('P') => Self::Parked,
            _ => Self::Other,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Sleeping => "Sleeping",
            Self::Idle => "Idle",
            Self::UninterruptibleDiskSleep => "UninterruptibleDiskSleep",
            Self::Zombie => "Zombie",
            Self::Stopped => "Stopped",
            Self::Tracing => "Tracing",
            Self::Dead => "Dead",
            Self::Wakekill => "Wakekill",
            Self::Waking => "Waking",
            Self::Parked => "Parked",
            Self::Other => "Other",
        }
    }
}

/// The identity half of one `/proc/<pid>/stat` read: command name, scheduling
/// state and parent. Kept separate from [`ProcStatFields`] so the scalar
/// record stays `Copy` and the two halves can fail independently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProcIdentity {
    pub(super) comm: String,
    pub(super) state: ProcessState,
    /// `None` for pid 0's children (the kernel reports ppid 0 for init and for
    /// kernel threads whose parent is not a user-space process).
    pub(super) parent_pid: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcStatFields {
    pub(super) threads: u32,
    pub(super) start_ticks: u64,
    pub(super) user_ticks: u64,
    pub(super) system_ticks: u64,
    pub(super) nice: i32,
    pub(super) policy: Option<ProcessSchedulingPolicy>,
    pub(super) minflt: u64,
    pub(super) majflt: u64,
}

impl ProcStatFields {
    /// Total user + system CPU time in clock ticks, saturating. Exposed for
    /// measurement-side consumers (e.g. wall-clock vs process-CPU profiling
    /// gates); the collector keeps its overflow-checked `total_cpu_ticks`.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn cpu_ticks_total(self) -> u64 {
        self.user_ticks.saturating_add(self.system_ticks)
    }

    /// Raw kernel start-tick token. Exposed only under `test-support` so
    /// property tests can lock the start-token monotonicity contract without
    /// widening the production API.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn start_ticks(self) -> u64 {
        self.start_ticks
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FdCount {
    pub(super) value: u32,
    pub(super) partial_failure: Option<FailureKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcIoFields {
    pub read_bytes: Result<u64, FailureKind>,
    pub write_bytes: Result<u64, FailureKind>,
    pub cancelled_write_bytes: Result<u64, FailureKind>,
}

/// Resident-memory components needed by the platform-neutral hybrid PSS
/// projection. The fields retain bytes, not kernel `kB`, so unit conversion is
/// performed exactly once at this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcStatusMemoryFields {
    pub(super) rss_bytes: u64,
    pub(super) rss_anon_bytes: u64,
    pub(super) rss_file_bytes: u64,
    pub(super) rss_shmem_bytes: u64,
}

/// Independent memory counters exported by Linux `smaps_rollup`.
///
/// Each field is kept as its own result because older kernels, restricted
/// procfs mounts, and permission changes can hide one counter without making
/// an already-readable status snapshot dishonest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcSmapsRollupFields {
    pub(super) private_clean_bytes: Result<u64, FailureKind>,
    pub(super) private_dirty_bytes: Result<u64, FailureKind>,
    pub(super) anon_huge_pages_bytes: Result<u64, FailureKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcMemoryObservations {
    pub(super) pss_fields: Result<ProcStatusMemoryFields, FailureKind>,
    pub(super) swap_bytes: Result<u64, FailureKind>,
    pub(super) uss_bytes: Result<u64, FailureKind>,
    pub(super) anon_huge_pages_bytes: Result<u64, FailureKind>,
}

/// Which memory sources one observation samples.
///
/// `Status` reads only `/proc/<pid>/status`: RSS components and `VmSwap` stay
/// fresh every tick. `Full` additionally reads `smaps_rollup` for the USS and
/// transparent-huge-page counters, which the kernel computes by walking every
/// page table of the target (~1.2 ms per process against ~36 µs for status).
/// A deferred sample is typed `TemporarilyUnavailable`, never zero, so the
/// row's `retain_previous` pass bridges the prior value as `Stale`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemorySample {
    Status,
    Full,
}

pub fn parse_proc_stat(text: &str) -> Option<ProcStatFields> {
    let (_, tail) = split_stat_text(text)?;
    // Indices: 7=minflt, 9=majflt, 11=utime, 12=stime, 16=nice, 17=threads, 19=starttime
    let [
        minflt,
        majflt,
        user_ticks,
        system_ticks,
        nice,
        threads,
        start_ticks,
    ] = nth_fields(tail, [7, 9, 11, 12, 16, 17, 19])?;
    // Field 41 (policy, tail index 38) is present on Linux 2.6+ but optional on short fixtures.
    let policy = tail
        .split_whitespace()
        .nth(38)
        .and_then(|p| p.parse::<u32>().ok())
        .map(ProcessSchedulingPolicy::from_linux_policy);
    Some(ProcStatFields {
        user_ticks: user_ticks.parse().ok()?,
        system_ticks: system_ticks.parse().ok()?,
        nice: nice.parse().ok()?,
        threads: threads.parse().ok()?,
        start_ticks: start_ticks.parse().ok()?,
        policy,
        minflt: minflt.parse().ok()?,
        majflt: majflt.parse().ok()?,
    })
}

/// Split one `/proc/<pid>/stat` payload into `(comm, tail)`.
///
/// The kernel wraps field 2 in parentheses and the command name may itself
/// contain spaces and parentheses, so the only safe delimiters are the FIRST
/// '(' and the LAST ')' in the whole line. A payload whose ')' precedes its
/// '(' is malformed and yields `None` rather than an inverted slice.
fn split_stat_text(text: &str) -> Option<(&str, &str)> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close <= open {
        return None;
    }
    Some((&text[open + 1..close], &text[close + 1..]))
}

/// Parse the identity fields from the same payload [`parse_proc_stat`] reads.
/// Tail indices: 0=state, 1=ppid (`proc(5)` fields 3 and 4).
pub(super) fn parse_proc_identity(text: &str) -> Option<ProcIdentity> {
    let (comm, tail) = split_stat_text(text)?;
    let [state, parent] = nth_fields(tail, [0, 1])?;
    Some(ProcIdentity {
        comm: comm.to_owned(),
        state: ProcessState::from_stat_field(Some(state)),
        parent_pid: parent.parse::<u32>().ok().filter(|pid| *pid != 0),
    })
}

/// Extract the whitespace-split fields at `indices` in one pass without
/// materializing the whole field list. Fields are positional (0-based after
/// the caller's prefix skip); extraction stops once the largest index is
/// passed. `None` when any wanted field is absent.
fn nth_fields<const N: usize>(text: &str, indices: [usize; N]) -> Option<[&str; N]> {
    let max_wanted = indices.iter().copied().max()?;
    let mut found: [Option<&str>; N] = [None; N];
    for (position, field) in text.split_whitespace().enumerate() {
        for (slot, wanted) in indices.iter().enumerate() {
            if *wanted == position {
                found[slot] = Some(field);
            }
        }
        if position >= max_wanted {
            break;
        }
    }
    let mut filled = [""; N];
    for (slot, value) in found.into_iter().enumerate() {
        filled[slot] = value?;
    }
    Some(filled)
}

/// Write `/proc/<pid>/<leaf>` into the caller's stack buffer and return the
/// path as a borrow of it — zero allocation (the previous
/// `format!("/proc/{pid}/…")` allocated three Strings per process per tick).
/// The buffer is 32 bytes: "/proc/" + at most 10 digits + "/" + a leaf no
/// longer than "status" cannot overflow. A leaf that does not fit the
/// remaining space is a typed [`FailureKind::ProviderFault`] instead of a
/// slice-bounds panic (the runtime guard costs one length comparison).
fn write_proc_path<'a>(
    buffer: &'a mut [u8; 32],
    pid: u32,
    leaf: &str,
) -> Result<&'a str, FailureKind> {
    let mut len = 6_usize;
    buffer[..len].copy_from_slice(b"/proc/");
    let mut digits = [0_u8; 10];
    let mut digit_len = 0_usize;
    let mut value = pid;
    if value == 0 {
        digits[0] = b'0';
        digit_len = 1;
    } else {
        while value > 0 {
            digits[digit_len] = b'0' + (value % 10) as u8;
            digit_len += 1;
            value /= 10;
        }
    }
    for &digit in digits[..digit_len].iter().rev() {
        buffer[len] = digit;
        len += 1;
    }
    buffer[len] = b'/';
    len += 1;
    if len + leaf.len() > buffer.len() {
        return Err(FailureKind::ProviderFault);
    }
    buffer[len..len + leaf.len()].copy_from_slice(leaf.as_bytes());
    len += leaf.len();
    Ok(std::str::from_utf8(&buffer[..len]).unwrap_or("/"))
}

pub(super) fn read_proc_stat(pid: u32) -> Result<ProcStatFields, FailureKind> {
    let text = read_proc_stat_text(pid)?;
    parse_proc_stat(&text).ok_or(FailureKind::ProviderFault)
}

/// One shared read of `/proc/<pid>/stat` for callers that need both halves.
/// Enumeration parses identity and scalars from this single payload; nothing
/// re-opens the file to obtain the other half.
pub(super) fn read_proc_stat_text(pid: u32) -> Result<String, FailureKind> {
    let mut path = [0_u8; 32];
    fs::read_to_string(write_proc_path(&mut path, pid, "stat")?).map_err(|error| io_failure(&error))
}

/// Split a raw `/proc/<pid>/cmdline` payload into argv.
///
/// The kernel separates arguments with NUL and terminates the payload with
/// one trailing NUL. Only that terminator is dropped: interior empty
/// arguments and argument whitespace are preserved verbatim, because the
/// command line is displayed evidence and trimming it would silently rewrite
/// what the process actually published.
pub(super) fn split_cmdline(bytes: &[u8]) -> Vec<String> {
    let body = match bytes.last() {
        Some(0) => &bytes[..bytes.len() - 1],
        _ => bytes,
    };
    if body.is_empty() {
        return Vec::new();
    }
    body.split(|byte| *byte == 0)
        .map(|argument| String::from_utf8_lossy(argument).into_owned())
        .collect()
}

/// Read argv for one pid. An unreadable or empty cmdline (kernel threads,
/// ptrace-restricted processes, exit races) yields an empty vector; the caller
/// falls back to the command name. No failure is fabricated as an argument.
pub(super) fn read_proc_cmdline(pid: u32) -> Vec<String> {
    let mut path = [0_u8; 32];
    let Ok(file_path) = write_proc_path(&mut path, pid, "cmdline") else {
        return Vec::new();
    };
    match fs::read(file_path) {
        Ok(bytes) => split_cmdline(&bytes),
        Err(_) => Vec::new(),
    }
}

/// Re-read the provider-native identity immediately before one process read or
/// mutation. This narrows, but cannot eliminate, the kernel-level race without
/// a pidfd-capable operation for the requested action.
pub(crate) fn validate_exact_start_token(
    target: &FrozenProcessIdentity,
) -> Result<(), FailureKind> {
    let expected = target
        .authoritative_start_token()
        .ok_or(FailureKind::IdentityChanged)?;
    if read_proc_stat(target.pid)?.start_ticks == expected {
        Ok(())
    } else {
        Err(FailureKind::IdentityChanged)
    }
}

pub fn parse_proc_status_memory(text: &str) -> Result<u64, FailureKind> {
    parse_unique_kib_field(text, "VmRSS:")
}

pub(super) fn read_proc_status_memory(pid: u32) -> Result<u64, FailureKind> {
    let mut path = [0_u8; 32];
    let text = fs::read_to_string(write_proc_path(&mut path, pid, "status")?)
        .map_err(|error| io_failure(&error))?;
    parse_proc_status_memory(&text)
}

pub(super) fn read_proc_oom_score(pid: u32) -> Option<u32> {
    let mut path = [0_u8; 32];
    let file_path = write_proc_path(&mut path, pid, "oom_score").ok()?;
    fs::read_to_string(file_path)
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
}

/// Parse all fields required for an independent hybrid-PSS and per-process
/// swap observation. Missing fields are `Unsupported`, while malformed or
/// duplicated fields are provider faults; neither case is converted to zero.
pub(super) fn parse_proc_status_memory_fields(
    text: &str,
) -> Result<ProcStatusMemoryFields, FailureKind> {
    Ok(ProcStatusMemoryFields {
        rss_bytes: parse_unique_kib_field(text, "VmRSS:")?,
        rss_anon_bytes: parse_unique_kib_field(text, "RssAnon:")?,
        rss_file_bytes: parse_unique_kib_field(text, "RssFile:")?,
        rss_shmem_bytes: parse_unique_kib_field(text, "RssShmem:")?,
    })
}

pub(super) fn read_proc_memory_observations(
    pid: u32,
    sample: MemorySample,
) -> Result<ProcMemoryObservations, FailureKind> {
    let mut path = [0_u8; 32];
    let text = fs::read_to_string(write_proc_path(&mut path, pid, "status")?)
        .map_err(|error| io_failure(&error))?;
    let smaps = match sample {
        MemorySample::Full => read_proc_smaps_rollup(pid),
        MemorySample::Status => Err(FailureKind::TemporarilyUnavailable),
    };
    let (uss_bytes, anon_huge_pages_bytes) = match smaps {
        Ok(fields) => {
            let ProcSmapsRollupFields {
                private_clean_bytes,
                private_dirty_bytes,
                anon_huge_pages_bytes,
            } = fields;
            (
                sum_memory_bytes(private_clean_bytes, private_dirty_bytes),
                anon_huge_pages_bytes,
            )
        }
        Err(failure) => (Err(failure), Err(failure)),
    };
    Ok(ProcMemoryObservations {
        pss_fields: parse_proc_status_memory_fields(&text),
        swap_bytes: parse_unique_kib_field(&text, "VmSwap:"),
        uss_bytes,
        anon_huge_pages_bytes,
    })
}

/// Parse the private memory and transparent huge-page counters from one
/// `smaps_rollup` payload. Results remain independent so callers can expose a
/// valid USS value even if the kernel omits the optional THP counter.
pub(super) fn parse_proc_smaps_rollup(text: &str) -> ProcSmapsRollupFields {
    ProcSmapsRollupFields {
        private_clean_bytes: parse_unique_kib_field(text, "Private_Clean:"),
        private_dirty_bytes: parse_unique_kib_field(text, "Private_Dirty:"),
        anon_huge_pages_bytes: parse_unique_kib_field(text, "AnonHugePages:"),
    }
}

fn read_proc_smaps_rollup(pid: u32) -> Result<ProcSmapsRollupFields, FailureKind> {
    let mut path = [0_u8; 32];
    let text = fs::read_to_string(write_proc_path(&mut path, pid, "smaps_rollup")?)
        .map_err(|error| io_failure(&error))?;
    Ok(parse_proc_smaps_rollup(&text))
}

fn sum_memory_bytes(
    left: Result<u64, FailureKind>,
    right: Result<u64, FailureKind>,
) -> Result<u64, FailureKind> {
    match (left, right) {
        (Ok(left), Ok(right)) => left.checked_add(right).ok_or(FailureKind::ProviderFault),
        (Err(failure), _) | (_, Err(failure)) => Err(failure),
    }
}

pub fn parse_proc_io(text: &str) -> ProcIoFields {
    ProcIoFields {
        read_bytes: parse_unique_u64_field(text, "read_bytes:"),
        write_bytes: parse_unique_u64_field(text, "write_bytes:"),
        cancelled_write_bytes: parse_unique_u64_field(text, "cancelled_write_bytes:"),
    }
}

pub(super) fn read_proc_io(pid: u32) -> Result<ProcIoFields, FailureKind> {
    let mut path = [0_u8; 32];
    let text = fs::read_to_string(write_proc_path(&mut path, pid, "io")?)
        .map_err(|error| io_failure(&error))?;
    Ok(parse_proc_io(&text))
}

pub(super) fn read_fd_count(pid: u32) -> Result<FdCount, FailureKind> {
    let mut path = [0_u8; 32];
    let entries =
        fs::read_dir(write_proc_path(&mut path, pid, "fd")?).map_err(|error| io_failure(&error))?;
    let mut value = 0_u32;
    let mut partial_failure = None;
    for entry in entries {
        match entry {
            Ok(_) => {
                value = value.checked_add(1).ok_or(FailureKind::ProviderFault)?;
            }
            Err(error) => {
                retain_strongest(&mut partial_failure, io_failure(&error));
            }
        }
    }
    Ok(FdCount {
        value,
        partial_failure,
    })
}

/// Query the POSIX statistics-clock frequency through nix's safe wrapper.
///
/// A missing, zero, negative, or failed value is typed. No architecture is
/// assigned a guessed 100 Hz fallback.
pub(super) fn clock_ticks_per_second() -> Result<u64, FailureKind> {
    #[cfg(target_os = "linux")]
    {
        match sysconf(SysconfVar::CLK_TCK) {
            Ok(Some(value)) => normalize_clock_ticks(value),
            Ok(None) => Err(FailureKind::Unsupported),
            Err(Errno::EINTR) => Err(FailureKind::TemporarilyUnavailable),
            Err(_) => Err(FailureKind::ProviderFault),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(FailureKind::Unsupported)
    }
}

#[cfg(target_os = "linux")]
fn normalize_clock_ticks(value: i64) -> Result<u64, FailureKind> {
    let value = u64::try_from(value).map_err(|_| FailureKind::ProviderFault)?;
    if value == 0 {
        Err(FailureKind::ProviderFault)
    } else {
        Ok(value)
    }
}

fn parse_unique_u64_field(text: &str, key: &str) -> Result<u64, FailureKind> {
    let mut value = None;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        if value.is_some() {
            return Err(FailureKind::ProviderFault);
        }
        let mut fields = rest.split_whitespace();
        let parsed = fields
            .next()
            .ok_or(FailureKind::ProviderFault)?
            .parse::<u64>()
            .map_err(|_| FailureKind::ProviderFault)?;
        if fields.next().is_some() {
            return Err(FailureKind::ProviderFault);
        }
        value = Some(parsed);
    }
    value.ok_or(FailureKind::Unsupported)
}

fn parse_unique_kib_field(text: &str, key: &str) -> Result<u64, FailureKind> {
    let mut value = None;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        if value.is_some() {
            return Err(FailureKind::ProviderFault);
        }
        let mut fields = rest.split_whitespace();
        let kib = fields
            .next()
            .ok_or(FailureKind::ProviderFault)?
            .parse::<u64>()
            .map_err(|_| FailureKind::ProviderFault)?;
        if fields.next() != Some("kB") || fields.next().is_some() {
            return Err(FailureKind::ProviderFault);
        }
        value = Some(kib.checked_mul(1024).ok_or(FailureKind::ProviderFault)?);
    }
    value.ok_or(FailureKind::Unsupported)
}

pub(super) fn retain_strongest(current: &mut Option<FailureKind>, candidate: FailureKind) {
    *current = Some(FailureKind::strongest(*current, candidate));
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_procfs_tests.rs"]
mod tests;
