//! Audited `perf_event_open` boundary crate — the workspace's FIRST `unsafe`
//! trust root (one of three: `perf-ioctl` / `afpacket` / `fd-bridge`).
//!
//! This is the first of three places in the product tree allowed to contain
//! `unsafe` (ADR-022; the others are `taskmanager-afpacket` ADR-024 and
//! `taskmanager-fd-bridge` ADR-025). It is the OS/driver ABI seam: a tiny, reviewed, fully
//! safe-public-API wrapper around the Linux `perf_event_open(2)` syscall and
//! its `ioctl` controls, used to read Intel i915 PMU per-engine busy counters.
//!
//! Refined safe-Rust principle (supersedes the strict zero-unsafe stance of
//! ADR-021 for this single carve-out):
//! * every business crate stays `#![forbid(unsafe_code)]`;
//! * OS/driver ABI work lives HERE, in ONE minimal, audited boundary crate; and
//! * the boundary crate exposes ONLY safe APIs.
//!
//! Trust-root invariants enforced by the workspace architecture test on every
//! change (`audited_boundary_crate_carries_its_own_unsafe_contract`):
//! * the crate root carries `#![deny(unsafe_op_in_unsafe_fn)]` (NOT `forbid` —
//!   forbid would disallow the audited opt-out);
//! * every `unsafe {` block and `unsafe fn` has a `// SAFETY:` comment on the
//!   same line or the line immediately before, citing the invariant;
//! * no raw pointer or `RawFd`/`AsRawFd` crosses the PUBLIC API — the only
//!   `unsafe` is forming the kernel fd into an `OwnedFd`/`File` we own, and the
//!   audited `ioctl` on a `File` we own; and
//! * eBPF was STILL removed (ADR-021) because it was too large to be a minimal
//!   trust root, while a single `perf_event_open` call qualifies.

// Linux-only kernel surface (perf_event_open); the crate is empty on other
// targets so the workspace still compiles there. Consumers reach it only
// through Linux-gated dependency edges (taskmanager-platform-linux,
// privilege-helper).
#![cfg(target_os = "linux")]
#![deny(unsafe_op_in_unsafe_fn)]

use core::mem::size_of;
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

/// `perf_event_attr` leading fields through `PERF_ATTR_SIZE_VER0` (64 bytes) —
/// the ONLY kernel ABI surface this crate touches. `#[repr(C)]` pins it to the
/// kernel's leading layout; `size` is set to `size_of::<Self>()` (64) so the
/// kernel accepts the struct: `perf_event_open(2)` returns `E2BIG` when
/// `attr.size < PERF_ATTR_SIZE_VER0` (64) — the bug that left every i915/xe PMU
/// counter open failing on a real Intel GPU host (the CI host has no GPU PMU,
/// so only the failure path was ever exercised). The trailing VER0 fields
/// (`wakeup_events`, `bp_type`, `bp_addr`/`config1`) stay zero for a
/// non-sampling uncore PMU counter read via [`GpuEngineCounter::read_counter`];
/// the full UAPI struct is far larger, but the kernel zero-fills the tail it
/// does not receive, so modeling through VER0 is both necessary and sufficient.
#[repr(C)]
#[derive(Clone, Copy)]
struct PerfEventAttr {
    type_: u32,
    size: u32,
    config: u64,
    sample_period_or_freq: u64,
    sample_type: u64,
    read_format: u64,
    /// First bitfield word of the kernel `perf_event_attr`. Bit 0 is
    /// `disabled` (`PERF_ATTR_DISABLED`); the remaining bits (`inherit`,
    /// `pinned`, `exclude_*`, …) stay zero so the counter is process/system
    /// unscoped and starts disabled until `PERF_EVENT_IOC_ENABLE`.
    bitfield_flags: u64,
    /// PERF_ATTR_SIZE_VER0 tail (offsets 48/52/56). Union aliases are kept as
    /// the `wakeup`/`bp`/`config1` names the kernel docs use; all zero for this
    /// crate's non-sampling uncore PMU reads.
    wakeup_events_or_watermark: u32,
    bp_type: u32,
    bp_addr_or_config1: u64,
}

/// `PERF_ATTR_DISABLED` — bit 0 of `perf_event_attr::disabled`.
const PERF_ATTR_DISABLED: u64 = 1;

/// `PERF_FORMAT_TOTAL_TIME_ENABLED` — read_format flag adding `time_enabled`.
const PERF_FORMAT_TOTAL_TIME_ENABLED: u64 = 1;
/// `PERF_FORMAT_TOTAL_TIME_RUNNING` — read_format flag adding `time_running`.
const PERF_FORMAT_TOTAL_TIME_RUNNING: u64 = 2;
/// Read layout = `{ value(u64), time_enabled(u64), time_running(u64) }` = 24
/// bytes, matching `intel_gpu_top`'s i915 PMU read. We only consume `value`
/// (cumulative busy ns); the time fields guard against counter-multiplexing
/// scaling and are kept for parity with the reference tool.
const READ_FORMAT: u64 = PERF_FORMAT_TOTAL_TIME_ENABLED | PERF_FORMAT_TOTAL_TIME_RUNNING;

/// `SYS_perf_event_open` — resolved through `libc` so the crate stays
/// architecture-portable rather than hard-coding the syscall number.
const SYS_PERF_EVENT_OPEN: libc::c_long = libc::SYS_perf_event_open as libc::c_long;

/// `PERF_EVENT_IOC_ENABLE` ioctl request (`_IO('$', 0)`).
const PERF_EVENT_IOC_ENABLE: u32 = 0x2400;
/// `PERF_EVENT_IOC_DISABLE` ioctl request (`_IO('$', 1)`).
const PERF_EVENT_IOC_DISABLE: u32 = 0x2401;
/// `PERF_EVENT_IOC_RESET` ioctl request (`_IO('$', 3)`).
const PERF_EVENT_IOC_RESET: u32 = 0x2403;

/// Issue `perf_event_open(2)` and return the owned kernel fd.
///
/// Private: the safe public API ([`GpuEngineCounter::open`]) is the only caller.
/// Combining the syscall and the `OwnedFd` ownership transfer in one audited
/// block keeps the crate to exactly two `unsafe` sites (this and `ioctl`).
fn perf_event_open(
    attr: &PerfEventAttr,
    pid: i32,
    cpu: i32,
    group_fd: i32,
    flags: u64,
) -> io::Result<OwnedFd> {
    // Reference→raw-pointer coercion is a safe op; do it OUTSIDE the block so
    // the audited site contains no `as *const`/`as *mut` cast literal.
    let attr_ptr: *const PerfEventAttr = attr;
    // SAFETY: `attr_ptr` is a valid pointer to a fully-initialized #[repr(C)]
    // PerfEventAttr borrowed for the call (outlives the syscall); pid, cpu,
    // group_fd and flags are by-value integers, matching the
    // perf_event_open(2) signature. libc::syscall returns a non-negative fd on
    // success or -1 with errno set otherwise; the error case becomes
    // io::Error::last_os_error() and never reaches from_raw_fd. On success the
    // integer is a freshly kernel-allocated file descriptor we exclusively own,
    // so OwnedFd::from_raw_fd is the one unavoidable unsafe — the OwnedFd
    // closes the descriptor on drop and no raw fd ever crosses the safe public
    // API.
    unsafe {
        let result = libc::syscall(SYS_PERF_EVENT_OPEN, attr_ptr, pid, cpu, group_fd, flags);
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(OwnedFd::from_raw_fd(result as i32))
    }
}

/// Issue a no-payload `PERF_EVENT_IOC_*` ioctl on an owned `File`.
///
/// Private: only the safe [`GpuEngineCounter`] control methods call this.
fn ioctl(file: &File, request: u32) -> io::Result<()> {
    // SAFETY: `file` is a valid File we own for the duration of the call
    // (borrowed by reference); `request` is one of
    // PERF_EVENT_IOC_{ENABLE,DISABLE,RESET} carrying no payload, so the third
    // argument is 0 and matches libc::ioctl's variadic contract for these
    // `_IO`-shaped requests. The raw fd obtained via as_raw_fd is read-only and
    // never escapes this function.
    let result = unsafe { libc::ioctl(file.as_raw_fd(), request as libc::c_ulong, 0) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// A safe handle to one Intel i915 PMU per-engine busy counter.
///
/// Owns the kernel file descriptor via `std::fs::File` (closes on drop, gives
/// [`Read`] for free). The public API never exposes the underlying `File`,
/// `RawFd`, or any raw pointer — callers can only `open`/`open_enabled`,
/// `read_counter`, and `enable`/`disable`/`reset`.
#[derive(Debug)]
pub struct GpuEngineCounter {
    file: File,
}

impl GpuEngineCounter {
    /// Open a perf counter for `(pmu_type, config)` on `cpu` in the disabled state.
    ///
    /// `pmu_type` is the value read from
    /// `/sys/bus/event_source/devices/<pmu>/type`; `config` is the per-event
    /// encoding (for xe, `(engine_class << 20) | (instance << 12) | event`; for
    /// i915, `(engine_class << 12) | (instance << 4) | I915_SAMPLE_BUSY`).
    /// `cpu` is a CPU from the PMU's `cpumask` sysfs file. An uncore PMU like
    /// i915/xe is pinned to one CPU, and `perf_event_open(2)` rejects
    /// `pid == -1 && cpu == -1` with `EINVAL` — so the caller MUST read cpumask
    /// (commonly `0`) and pass it here. The counter is opened system-wide
    /// (`pid = -1`, no group, no flags) and starts DISABLED — call
    /// [`GpuEngineCounter::open_enabled`] or [`GpuEngineCounter::enable`] to
    /// arm it.
    pub fn open(pmu_type: u32, config: u64, cpu: i32) -> io::Result<Self> {
        let attr = PerfEventAttr {
            type_: pmu_type,
            size: size_of::<PerfEventAttr>() as u32,
            config,
            sample_period_or_freq: 0,
            sample_type: 0,
            read_format: READ_FORMAT,
            bitfield_flags: PERF_ATTR_DISABLED,
            wakeup_events_or_watermark: 0,
            bp_type: 0,
            bp_addr_or_config1: 0,
        };
        let owned = perf_event_open(&attr, -1, cpu, -1, 0)?;
        Ok(Self {
            file: File::from(owned),
        })
    }

    /// Open, reset and enable in one call — the typical "ready to read" path.
    /// `cpu` is a CPU from the PMU's `cpumask` (see [`GpuEngineCounter::open`]).
    pub fn open_enabled(pmu_type: u32, config: u64, cpu: i32) -> io::Result<Self> {
        let counter = Self::open(pmu_type, config, cpu)?;
        ioctl(&counter.file, PERF_EVENT_IOC_RESET)?;
        ioctl(&counter.file, PERF_EVENT_IOC_ENABLE)?;
        Ok(counter)
    }

    /// Read the counter's cumulative `value` (i915 busy = nanoseconds busy).
    ///
    /// Reads the 24-byte `{ value, time_enabled, time_running }` layout selected
    /// by `READ_FORMAT` and returns the first `u64`. Two samples over a
    /// measured interval — same units as the sysfs `busy` node — become a 0–100%
    /// rate in the provider; this crate performs no rate math.
    pub fn read_counter(&mut self) -> io::Result<u64> {
        let mut buffer = [0u8; 24];
        self.file.read_exact(&mut buffer)?;
        let mut value_bytes = [0u8; 8];
        value_bytes.copy_from_slice(&buffer[..8]);
        Ok(u64::from_le_bytes(value_bytes))
    }

    /// Arm the counter (`PERF_EVENT_IOC_ENABLE`).
    pub fn enable(&self) -> io::Result<()> {
        ioctl(&self.file, PERF_EVENT_IOC_ENABLE)
    }

    /// Disarm the counter (`PERF_EVENT_IOC_DISABLE`).
    pub fn disable(&self) -> io::Result<()> {
        ioctl(&self.file, PERF_EVENT_IOC_DISABLE)
    }

    /// Reset the counter to zero (`PERF_EVENT_IOC_RESET`).
    pub fn reset(&self) -> io::Result<()> {
        ioctl(&self.file, PERF_EVENT_IOC_RESET)
    }
}

#[cfg(test)]
#[path = "../tests/headless/perf_contract.rs"]
mod tests;
