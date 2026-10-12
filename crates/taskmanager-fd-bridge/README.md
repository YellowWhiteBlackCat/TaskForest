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

`send_fd`/`recv_fd` transfer exactly one descriptor; protocol violations are
typed, never leaked. `peer_credentials(stream)` exposes the kernel-filled
`SO_PEERCRED` identity, and the pidfd seams wrap the Linux ≥ 5.1 syscalls
with a typed `ENOSYS` fallback. The crate does not open packet sockets,
authorize users or attribute traffic; keep the handoff bounded and fail
closed on malformed messages. Transfer rules, peer identity, pidfd and the
fuzz surface are specified in
[`docs/PROTOCOL_RULES.md`](docs/PROTOCOL_RULES.md).

## Module map

```text
src/lib.rs     SCM_RIGHTS fd transfer (launcher → unprivileged app)
src/pidfd.rs   pidfd identity verification
```

## Docs

- [`docs/PROTOCOL_RULES.md`](docs/PROTOCOL_RULES.md) — one-fd transfer rules, peer identity,
  pidfd seams, fuzz surface and verification.
- [ADR-025](../../adr/025-fd-bridge-scm-rights-boundary-crate.md) — fd-bridge boundary crate.
