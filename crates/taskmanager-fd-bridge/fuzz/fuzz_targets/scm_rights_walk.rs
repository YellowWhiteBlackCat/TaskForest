#![no_main]
//! Fuzz target for the SCM_RIGHTS cmsg walk (`find_scm_rights`): a hostile
//! peer influences the bytes `recvmsg` leaves in the ancillary control
//! buffer, and the walk then performs CMSG layout arithmetic (header length,
//! alignment, truncation, chaining) over them. The contract is total — any
//! control bytes and any reported controllen must either return the fds
//! well-formed rights cmsgs carry or `None`, never panic, never read out of
//! bounds, never fabricate an fd.
//!
//! Input layout: `[control bytes][8-byte little-endian reported controllen]`.
//! The reported length is clamped to the bytes actually supplied so the
//! walked window is exactly attacker-supplied data; the seam's zeroed slab
//! tail stays outside the fuzzed region (which keeps the fd-provenance
//! oracle below sound).
use libfuzzer_sys::fuzz_target;

/// Slab capacity of `find_scm_rights_in_control` (96 bytes on x86-64: six
/// cmsghdrs). The harness must not claim bytes beyond the input it supplied.
const CONTROL_SLAB: usize = 96;
const FD_SIZE: usize = std::mem::size_of::<i32>();

fuzz_target!(|data: &[u8]| {
    if data.len() < std::mem::size_of::<usize>() {
        return;
    }
    let (control, tail) = data.split_at(data.len() - std::mem::size_of::<usize>());
    let control = &control[..control.len().min(CONTROL_SLAB)];
    let reported = usize::from_le_bytes(tail.try_into().expect("eight-byte length suffix"));
    let controllen = reported.min(control.len());
    let fds = taskmanager_fd_bridge::find_scm_rights_in_control(control, controllen);
    let Some(fds) = fds else { return };
    // Found-fd contract: the walk never returns an empty list (that outcome
    // is `None`), never more fds than the walked window could hold, and
    // every fd is a verbatim c_int copied from inside the fuzzed bytes
    // (little-endian target) — a value whose byte pattern is absent from
    // the input was fabricated, not received.
    assert!(!fds.is_empty());
    assert!(fds.len() <= control.len() / FD_SIZE);
    for fd in fds {
        let pattern = fd.to_le_bytes();
        assert!(
            control.windows(FD_SIZE).any(|window| window == pattern),
            "walk produced fd {fd} from bytes absent in the control buffer"
        );
    }
});
