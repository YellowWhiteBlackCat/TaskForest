//! Owned procfs process enumeration.
//!
//! The inventory used to delegate enumeration to `sysinfo::System`. That
//! dependency keeps one **open** `/proc/<pid>/stat` descriptor per observed
//! process for the lifetime of the handle (its `FileCounter` budget is half of
//! `RLIMIT_NOFILE`, and it raises the process's soft limit to the hard limit as
//! a side effect of initializing that budget). A 7×24 monitor therefore parked
//! one descriptor per host process forever, mutated a process-global resource
//! limit from inside a dependency, and re-read `statm`/`io`/CPU counters the
//! inventory never consumes — while this module already reads `stat` itself
//! for every scalar. One fact had two readers and the descriptor table was the
//! visible cost.
//!
//! This module is now the single enumeration authority: one `/proc` directory
//! pass yields both the availability probe and the live pid list, and one
//! `stat` read per pid yields identity and scalars together. Every reader
//! closes its own descriptors, so the retained descriptor count is independent
//! of the host's process population.

use std::fs;

use taskforest_core::FailureKind;

use super::procfs::{
    ProcIdentity, ProcStatFields, parse_proc_identity, parse_proc_stat, read_proc_cmdline,
    read_proc_stat_text, retain_strongest,
};
use super::tree::io_failure;

/// Procfs availability once a filesystem-level failure has been excluded.
/// The unavailable state is represented by the `Err` of [`list_procfs`], so
/// the refresh path handles it before any item work and the later outcome
/// match is exhaustive without a panic arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProcfsProbe {
    Complete,
    Partial(FailureKind),
}

/// One `/proc` pass: the sorted live pid list plus the strongest entry-level
/// failure seen while walking it. Sorting makes the published row order
/// deterministic (the previous `HashMap` walk published arbitrary order).
#[derive(Debug)]
pub(super) struct ProcfsListing {
    pub(super) pids: Vec<u32>,
    pub(super) probe: ProcfsProbe,
}

/// Enumerate `/proc` exactly once per tick.
///
/// Non-numeric entries are kernel pseudo-files, not processes. An entry that
/// cannot be read at all is a partial procfs failure, never a silently dropped
/// process; a directory that cannot be opened is a typed unavailable source.
pub(super) fn list_procfs() -> Result<ProcfsListing, FailureKind> {
    let entries = fs::read_dir("/proc").map_err(|error| io_failure(&error))?;
    let mut pids = Vec::new();
    let mut strongest = None;
    for entry in entries {
        match entry {
            Ok(entry) => {
                if let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.parse::<u32>().ok())
                {
                    pids.push(pid);
                }
            }
            Err(error) => retain_strongest(&mut strongest, io_failure(&error)),
        }
    }
    pids.sort_unstable();
    Ok(ProcfsListing {
        pids,
        probe: strongest.map_or(ProcfsProbe::Complete, ProcfsProbe::Partial),
    })
}

/// Everything one pid contributes to a row before the scalar observation pass:
/// the two halves of a single `stat` read plus a fresh argv.
#[derive(Debug)]
pub(super) struct ProcessEntry {
    pub(super) stat: ProcStatFields,
    pub(super) identity: ProcIdentity,
    pub(super) argv: Vec<String>,
}

/// Read one pid's identity, scalars and argv.
///
/// A pid that cannot be read is a typed failure, not a row with a fabricated
/// name: the caller records the failure against the stat source and publishes
/// no row for it. argv is re-read every tick because a process may rewrite its
/// own command line (`setproctitle`) without changing identity; caching it for
/// the process lifetime would display stale evidence.
pub(super) fn read_process_entry(pid: u32) -> Result<ProcessEntry, FailureKind> {
    let text = read_proc_stat_text(pid)?;
    let identity = parse_proc_identity(&text).ok_or(FailureKind::ProviderFault)?;
    let stat = parse_proc_stat(&text).ok_or(FailureKind::ProviderFault)?;
    Ok(ProcessEntry {
        stat,
        identity,
        argv: read_proc_cmdline(pid),
    })
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_enumerate_tests.rs"]
mod tests;
