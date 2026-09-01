# taskmanager-fd-bridge

## Role

Audited SCM_RIGHTS file-descriptor bridge between a privileged helper and the
unprivileged application (ADR-025), plus the audited `SO_PEERCRED` and Linux
pidfd seams the escalation chain consumes.

## Boundary

The crate owns ancillary-message layout, alignment, length checks, socket
ownership and close-on-exec behavior. Public APIs return owned descriptors and
typed errors; no raw pointer or OS handle crosses the boundary.

## Contract and verification

`send_fd`/`recv_fd` transfer exactly one descriptor: multi-fd cmsgs, several
rights cmsgs, and `MSG_CTRUNC` truncation are protocol violations — every fd
the kernel installed is closed and a typed `InvalidData` error is returned;
`EINTR` is retried, never reported as failure. `peer_credentials(stream)`
returns the kernel-filled `Ucred { pid, uid, gid }` of a connected Unix
stream's peer (`SO_PEERCRED`) — the receiver of a handed-off fd admits only a
uid-0 peer, so a guessed abstract address is disconnected instead of indulged.
`pidfd_open(pid)` / `pidfd_send_signal(pidfd, signal)` wrap the Linux ≥ 5.1
syscalls (close-on-exec owned fd; signal 0 probes); `is_pidfd_unsupported`
matches `ENOSYS` so callers fall back to their legacy typed path.

It does not open packet sockets, authorize users or attribute traffic. Keep
the handoff bounded and fail closed on malformed messages.

Fuzz: the pure cmsg-walk half of `recv_fd` is exposed behind the crate's
`test-support` feature as `find_scm_rights_in_control(bytes, controllen)`
(raw bytes into a product-shaped aligned slab; the slab type stays private)
and fuzzed by the standalone `fuzz/` workspace target `scm_rights_walk` —
arbitrary control bytes and reported lengths must never panic, read out of
bounds, or fabricate an fd.

### Verification

Socketpair round-trips, multi-fd/truncation rejection with fd-leak checks,
`SO_PEERCRED` cross-anchored against `getuid`/`process::id`, pidfd
probe/ESRCH/ENOSYS predicate tests, dependency firewall and Miri before
changing the crate.

## Module map

```text
src/lib.rs     SCM_RIGHTS fd transfer (launcher → unprivileged app)
src/pidfd.rs   pidfd identity verification
```
