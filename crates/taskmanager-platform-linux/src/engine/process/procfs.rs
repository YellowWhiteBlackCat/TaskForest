//! Small typed process-specific procfs and POSIX clock readers.

use std::fs;

#[cfg(target_os = "linux")]
use nix::errno::Errno;
#[cfg(target_os = "linux")]
use nix::unistd::{SysconfVar, sysconf};
use taskmanager_core::{FailureKind, FrozenProcessIdentity};

use super::tree::io_failure;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcStatFields {
    pub(super) threads: u32,
    pub(super) start_ticks: u64,
    pub(super) user_ticks: u64,
    pub(super) system_ticks: u64,
    pub(super) nice: i32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcMemoryObservations {
    pub(super) pss_fields: Result<ProcStatusMemoryFields, FailureKind>,
    pub(super) swap_bytes: Result<u64, FailureKind>,
}

pub fn parse_proc_stat(text: &str) -> Option<ProcStatFields> {
    let _ = text.find('(')?;
    let rparen = text.rfind(')')?;
    // One allocation-free walk that materializes only the five fields the
    // observation reads (previously a full Vec<&str> per read per tick).
    let [user_ticks, system_ticks, nice, threads, start_ticks] =
        nth_fields(&text[rparen + 1..], [11, 12, 16, 17, 19])?;
    Some(ProcStatFields {
        user_ticks: user_ticks.parse().ok()?,
        system_ticks: system_ticks.parse().ok()?,
        nice: nice.parse().ok()?,
        threads: threads.parse().ok()?,
        start_ticks: start_ticks.parse().ok()?,
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
    let mut path = [0_u8; 32];
    let text = fs::read_to_string(write_proc_path(&mut path, pid, "stat")?)
        .map_err(|error| io_failure(&error))?;
    parse_proc_stat(&text).ok_or(FailureKind::ProviderFault)
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
) -> Result<ProcMemoryObservations, FailureKind> {
    let mut path = [0_u8; 32];
    let text = fs::read_to_string(write_proc_path(&mut path, pid, "status")?)
        .map_err(|error| io_failure(&error))?;
    Ok(ProcMemoryObservations {
        pss_fields: parse_proc_status_memory_fields(&text),
        swap_bytes: parse_unique_kib_field(&text, "VmSwap:"),
    })
}

pub fn parse_proc_io(text: &str) -> ProcIoFields {
    ProcIoFields {
        read_bytes: parse_unique_u64_field(text, "read_bytes:"),
        write_bytes: parse_unique_u64_field(text, "write_bytes:"),
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

fn retain_strongest(current: &mut Option<FailureKind>, candidate: FailureKind) {
    if current.is_none_or(|failure| failure_priority(candidate) > failure_priority(failure)) {
        *current = Some(candidate);
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::MissingDependency => 7,
        FailureKind::TimedOut => 6,
        FailureKind::ProviderFault => 5,
        FailureKind::TemporarilyUnavailable => 4,
        FailureKind::Unsupported => 3,
        FailureKind::IdentityChanged => 2,
        FailureKind::Rejected => 1,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_procfs_tests.rs"]
mod tests;
