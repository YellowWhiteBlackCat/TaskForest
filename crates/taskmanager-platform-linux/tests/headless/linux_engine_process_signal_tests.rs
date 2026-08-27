//! Co-located round-trip + range checks for the `pub(crate)` affinity helpers
//! [`super::cpus_to_cpuset`] / [`super::cpuset_to_cpus`]. These live here (not in
//! `tests/`) because the functions are `pub(crate)`. The sibling `mod tests` in
/// `core/process.rs` already covers the basic round-trip, duplicates, and the
/// `MAX_CPU` boundary `is_ok/is_err`; this block adds the value-level boundary
/// round-trip (the last valid id must come back unchanged) and pins the typed
/// `Rejected` contract.
use super::{
    classify_nix_process_errno, classify_rustix_process_errno, cpus_to_cpuset, cpuset_to_cpus,
    validated_raw_pid,
};
use taskmanager_core::FailureKind;

/// Empty input round-trips to an empty Vec — the documented "no cpus" case.
#[test]
fn empty_cpuset_roundtrips_to_empty_vec() {
    let set = cpus_to_cpuset(&[]).unwrap();
    assert!(cpuset_to_cpus(&set).is_empty());
}

/// A single id round-trips unchanged, and a mixed set of ids comes back sorted
/// in ascending order regardless of input order (the bit-set has no ordering).
#[test]
fn cpuset_roundtrip_preserves_sorted_ids() {
    // Single id.
    let one = cpus_to_cpuset(&[0]).unwrap();
    assert_eq!(cpuset_to_cpus(&one), vec![0]);

    // Distinct ids, given out of order, come back sorted.
    let mixed = cpus_to_cpuset(&[7, 31, 15, 3]).unwrap();
    assert_eq!(cpuset_to_cpus(&mixed), vec![3, 7, 15, 31]);
}

/// `MAX_CPU - 1` is the last valid cpu id. Round-tripping it (not just `is_ok`)
/// confirms the highest bit is settable AND readable — the boundary the kernel
/// and `CPU_SET` macro both honor.
#[test]
fn last_valid_cpu_id_roundtrips() {
    let last =
        u32::try_from(rustix::thread::CpuSet::MAX_CPU - 1).expect("Linux CpuSet limit fits u32");
    let set = cpus_to_cpuset(&[last]).unwrap();
    assert_eq!(cpuset_to_cpus(&set), vec![last]);
}

/// An id `>= MAX_CPU` is rejected before any syscall. A valid id followed
/// by an out-of-range id also fails without partial mutation.
#[test]
fn out_of_range_cpu_is_typed_as_rejected() {
    let max = u32::try_from(rustix::thread::CpuSet::MAX_CPU).expect("Linux CpuSet limit fits u32");
    assert!(matches!(cpus_to_cpuset(&[max]), Err(FailureKind::Rejected)));
    let big = max + 100;
    assert!(matches!(cpus_to_cpuset(&[big]), Err(FailureKind::Rejected)));
    assert!(matches!(
        cpus_to_cpuset(&[0, max]),
        Err(FailureKind::Rejected)
    ));
}

#[test]
fn process_errno_classifiers_preserve_actionable_failure_kinds() {
    let nix_cases = [
        (nix::errno::Errno::EPERM, FailureKind::PermissionDenied),
        (nix::errno::Errno::ESRCH, FailureKind::IdentityChanged),
        (nix::errno::Errno::ENOSYS, FailureKind::Unsupported),
        (nix::errno::Errno::ETIMEDOUT, FailureKind::TimedOut),
        (
            nix::errno::Errno::EAGAIN,
            FailureKind::TemporarilyUnavailable,
        ),
        (nix::errno::Errno::EINVAL, FailureKind::Rejected),
        (nix::errno::Errno::EIO, FailureKind::ProviderFault),
    ];
    for (error, expected) in nix_cases {
        assert_eq!(classify_nix_process_errno(error), expected);
    }

    let rustix_cases = [
        (rustix::io::Errno::PERM, FailureKind::PermissionDenied),
        (rustix::io::Errno::SRCH, FailureKind::IdentityChanged),
        (rustix::io::Errno::NOSYS, FailureKind::Unsupported),
        (rustix::io::Errno::TIMEDOUT, FailureKind::TimedOut),
        (
            rustix::io::Errno::AGAIN,
            FailureKind::TemporarilyUnavailable,
        ),
        (rustix::io::Errno::INVAL, FailureKind::Rejected),
        (rustix::io::Errno::IO, FailureKind::ProviderFault),
    ];
    for (error, expected) in rustix_cases {
        assert_eq!(classify_rustix_process_errno(error), expected);
    }
}

#[test]
fn invalid_native_pids_are_rejected_before_syscall() {
    assert_eq!(validated_raw_pid(0), Err(FailureKind::Rejected));
    assert_eq!(validated_raw_pid(u32::MAX), Err(FailureKind::Rejected));
}
