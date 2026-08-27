//! Bounded `/proc/<pid>/cwd` + `/proc/<pid>/environ` collector.
//!
//! The environment is read only for an explicitly selected process (never for
//! the whole list) and is capped by the shared
//! `MAX_ENVIRONMENT_BYTES`/`MAX_ENVIRONMENT_ENTRIES` budgets. Truncation is
//! reported, never silent, and the same identity pinning as the other facets
//! protects against PID reuse.

use std::fs;
use std::io::Read;
use std::path::Path;

use taskmanager_core::{
    DeviceStatus, MAX_ENVIRONMENT_BYTES, MAX_ENVIRONMENT_ENTRIES, ProcessEnvironment,
    ProcessEnvironmentEntry,
};

use super::{state_for_status, status_from_io_error};

/// Collect the bounded environment facet from one `/proc/<pid>` directory.
/// Designed to be called from a collector that has already pinned the process
/// start-time token.
#[must_use]
pub fn collect_environment_from_proc_dir(proc_dir: &Path, now_ms: u64) -> ProcessEnvironment {
    let working_directory = fs::read_link(proc_dir.join("cwd")).ok();
    let raw = read_bounded(proc_dir.join("environ"));
    let (entries, truncated_count) = match raw {
        Ok((bytes, hit_byte_cap)) => {
            let (entries, dropped) = parse_entries(&bytes, hit_byte_cap);
            (entries, dropped)
        }
        Err(error) => {
            return ProcessEnvironment {
                state: state_for_status(status_from_io_error(&error), now_ms),
                working_directory,
                entries: Vec::new(),
                truncated_count: 0,
            };
        }
    };
    ProcessEnvironment {
        state: state_for_status(DeviceStatus::Healthy, now_ms),
        working_directory,
        entries,
        truncated_count,
    }
}

/// Read at most `MAX_ENVIRONMENT_BYTES + 1` bytes so a clean file smaller than
/// the budget is provably complete. `hit_byte_cap` is true when the cap was
/// reached (the trailing entry may be partial and is dropped by the parser).
fn read_bounded(path: std::path::PathBuf) -> std::io::Result<(Vec<u8>, bool)> {
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0_u8; MAX_ENVIRONMENT_BYTES + 1];
    let mut total = 0_usize;
    loop {
        let read = file.read(&mut buffer[total..])?;
        if read == 0 {
            break;
        }
        total += read;
        if total > MAX_ENVIRONMENT_BYTES {
            buffer.truncate(MAX_ENVIRONMENT_BYTES);
            return Ok((buffer, true));
        }
    }
    buffer.truncate(total);
    Ok((buffer, false))
}

/// Split a NUL-separated environ blob into ordered key/value entries.
///
/// Entries without a `=` separator are provider noise and skipped. When the
/// byte cap clipped the final entry, that partial entry counts as truncated.
/// Entries beyond the entry cap are counted, not retained.
fn parse_entries(bytes: &[u8], hit_byte_cap: bool) -> (Vec<ProcessEnvironmentEntry>, u32) {
    let mut entries = Vec::new();
    let mut truncated_count = 0_u32;
    let mut tail_is_partial = hit_byte_cap && !bytes.ends_with(&[0]);
    for raw in bytes.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        if tail_is_partial {
            tail_is_partial = false;
            truncated_count += 1;
            continue;
        }
        let Some(separator) = raw.iter().position(|byte| *byte == b'=') else {
            continue;
        };
        let key = String::from_utf8_lossy(&raw[..separator]).into_owned();
        let value = String::from_utf8_lossy(&raw[separator + 1..]).into_owned();
        if key.is_empty() {
            continue;
        }
        if entries.len() >= MAX_ENVIRONMENT_ENTRIES {
            truncated_count += 1;
            continue;
        }
        entries.push(ProcessEnvironmentEntry { key, value });
    }
    (entries, truncated_count)
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_process_telemetry_environment_tests.rs"]
mod tests;
