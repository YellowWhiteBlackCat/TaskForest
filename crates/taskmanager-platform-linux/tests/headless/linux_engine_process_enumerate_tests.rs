//! Owned procfs enumeration: one `/proc` pass, one `stat` read per pid, and
//! zero descriptors retained across ticks.

use super::*;

#[test]
fn one_procfs_pass_yields_sorted_pids_that_include_the_running_process() {
    let listing = list_procfs().expect("procfs must be enumerable on Linux");
    assert!(
        listing.pids.len() > 1,
        "a live Linux host must expose more than one process"
    );
    assert!(
        listing.pids.contains(&std::process::id()),
        "the running process must enumerate itself"
    );
    // Deterministic order is a published behavior: the previous `HashMap` walk
    // published rows in arbitrary order.
    let mut sorted = listing.pids.clone();
    sorted.sort_unstable();
    assert_eq!(
        listing.pids, sorted,
        "pids must be published in ascending order"
    );
    let mut unique = listing.pids.clone();
    unique.dedup();
    assert_eq!(listing.pids, unique, "one pid must appear at most once");
    assert!(
        matches!(
            listing.probe,
            ProcfsProbe::Complete | ProcfsProbe::Partial(_)
        ),
        "a successful directory walk never reports an unavailable source"
    );
}

#[test]
fn an_unreadable_procfs_entry_degrades_the_probe_never_the_source() {
    // A walk that could not read one entry must not be published as Complete:
    // that would let a restricted procfs mount look like a healthy inventory.
    let mut strongest = None;
    retain_strongest(&mut strongest, FailureKind::PermissionDenied);
    retain_strongest(&mut strongest, FailureKind::TemporarilyUnavailable);
    assert_eq!(
        strongest.map_or(ProcfsProbe::Complete, ProcfsProbe::Partial),
        ProcfsProbe::Partial(FailureKind::PermissionDenied)
    );
    let clean: Option<FailureKind> = None;
    assert_eq!(
        clean.map_or(ProcfsProbe::Complete, ProcfsProbe::Partial),
        ProcfsProbe::Complete
    );
}

#[test]
fn reading_the_running_process_yields_identity_scalars_and_argv_together() {
    let pid = std::process::id();
    let entry = read_process_entry(pid).expect("the running process must be readable");
    assert!(
        !entry.identity.comm.is_empty(),
        "the command name must not be fabricated empty"
    );
    assert_eq!(
        entry.identity.parent_pid,
        Some(std::os::unix::process::parent_id()),
        "the parent must come from the same stat payload as the identity"
    );
    assert!(
        entry.stat.start_ticks > 0,
        "a live process carries a real start token"
    );
    assert!(
        !entry.argv.is_empty(),
        "the test binary is invoked with at least its own path"
    );
}

#[test]
fn a_pid_that_cannot_be_read_is_a_typed_failure_not_a_nameless_row() {
    // No Linux host can have pid u32::MAX, so this is the honest shape of an
    // unreadable entry: a typed failure the caller can publish as a source
    // outcome. It must never degrade into an empty name plus zeroed scalars.
    let failure = read_process_entry(u32::MAX).expect_err("an absent pid cannot be read");
    assert_eq!(failure, FailureKind::TemporarilyUnavailable);
}

#[test]
fn enumeration_holds_no_descriptor_per_observed_process() {
    // The failure this test owns: a collector that keeps one open
    // `/proc/<pid>/stat` handle per process makes the descriptor table track
    // the host's process population. Enumerate and read every pid, then assert
    // the retained handles do not scale with the number observed.
    let listing = list_procfs().expect("procfs must be enumerable on Linux");
    let mut read_ok = 0_usize;
    for pid in listing.pids {
        if read_process_entry(pid).is_ok() {
            read_ok += 1;
        }
    }
    assert!(
        read_ok > 20,
        "the sample must be large enough to make the bound meaningful"
    );
    let held = retained_procfs_handles();
    assert!(
        held <= 4,
        "reading {read_ok} processes left {held} descriptors pointing into another \
         process's /proc entry — a per-process handle is being retained"
    );
}

/// Count this process's open descriptors whose target is a `/proc/<pid>/…`
/// path belonging to some other process.
fn retained_procfs_handles() -> usize {
    let own = std::process::id();
    let mut held = 0_usize;
    let Ok(directory) = fs::read_dir("/proc/self/fd") else {
        return 0;
    };
    for entry in directory.flatten() {
        let Ok(target) = fs::read_link(entry.path()) else {
            continue;
        };
        let target = target.to_string_lossy();
        let Some(rest) = target.strip_prefix("/proc/") else {
            continue;
        };
        let pid = rest.split('/').next().unwrap_or_default();
        match pid.parse::<u32>() {
            Ok(pid) if pid != own => held += 1,
            _ => {}
        }
    }
    held
}
