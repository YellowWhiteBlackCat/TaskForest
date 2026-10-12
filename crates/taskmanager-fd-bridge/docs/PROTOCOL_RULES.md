# Protocol rules

## Descriptor transfer

`send_fd`/`recv_fd` transfer exactly one descriptor: multi-fd cmsgs, several
rights cmsgs, and `MSG_CTRUNC` truncation are protocol violations — every fd
the kernel installed is closed and a typed `InvalidData` error is returned;
`EINTR` is retried, never reported as failure.

## Peer identity

`peer_credentials(stream)` returns the kernel-filled
`Ucred { pid, uid, gid }` of a connected Unix stream's peer (`SO_PEERCRED`)
— the receiver of a handed-off fd admits only a uid-0 peer, so a guessed
abstract address is disconnected instead of indulged.

## pidfd seams

`pidfd_open(pid)` / `pidfd_send_signal(pidfd, signal)` wrap the Linux ≥ 5.1
syscalls (close-on-exec owned fd; signal 0 probes); `is_pidfd_unsupported`
matches `ENOSYS` so callers fall back to their legacy typed path.

## Fuzzing

The pure cmsg-walk half of `recv_fd` is exposed behind the crate's
`test-support` feature as `find_scm_rights_in_control(bytes, controllen)`
(raw bytes into a product-shaped aligned slab; the slab type stays private)
and fuzzed by the standalone `fuzz/` workspace target `scm_rights_walk` —
arbitrary control bytes and reported lengths must never panic, read out of
bounds, or fabricate an fd.

## Verification

Socketpair round-trips, multi-fd/truncation rejection with fd-leak checks,
`SO_PEERCRED` cross-anchored against `getuid`/`process::id`, pidfd
probe/ESRCH/ENOSYS predicate tests, dependency firewall and Miri before
changing the crate:

```bash
cargo nextest run -p taskmanager-fd-bridge -j 4
```
