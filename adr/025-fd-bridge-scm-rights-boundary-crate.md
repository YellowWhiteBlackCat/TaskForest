# ADR-025: Audited SCM_RIGHTS fd-passing boundary crate (launcher → unprivileged app)

- Status: Accepted
- Extends: [ADR-023 (per-feature privilege escalation)](023-per-feature-privilege-escalation-framework.md), [ADR-024 (AF_PACKET boundary crate)](024-afpacket-boundary-crate.md), [ADR-022 (the boundary-crate precedent)](022-audited-perf-boundary-crate.md)

## Context

ADR-024 made the `AF_PACKET` socket the per-process-network trust root, and
ADR-023 requires privilege to flow only through the OS-native prompt (polkit +
`pkexec` on Linux) — never a blanket `setcap`/`setuid` on the main binary. So a
dedicated privileged **launcher** (`taskmanager-net-launcher`) opens the
`AF_PACKET` socket with `CAP_NET_RAW` and must hand the fd to the unprivileged
app, which then runs the capture loop on the inherited fd without ever holding
the capability.

Two constraints shape the mechanism:

- **`pkexec` sanitizes the child's inherited fd table** (a security feature),
  so the fd cannot simply be inherited across the `exec`. It must be
  transferred over a channel the launcher connects back to.
- **`AF_PACKET`'s own contract (ADR-024) says its `unsafe` ends at
  `recvfrom`.** `SCM_RIGHTS` ancillary-data `sendmsg`/`recvmsg` with the
  `CMSG_*` macros is a *different* kernel surface — mixing it into
  `taskmanager-afpacket` would violate that crate's stated scope and re-create
  the "two unrelated ABI surfaces in one trust root" smell rejected by the
  safe-Rust boundary policy.

The refined principle (one minimal audited boundary crate per kernel surface)
therefore calls for a *third* boundary crate for the fd-pass.

## Decision

Introduce `crates/taskmanager-fd-bridge` — the workspace's **third Unix**
audited `unsafe` boundary crate (and fourth boundary overall) — as the seam for passing a file descriptor between
processes over a Unix domain socket via `SCM_RIGHTS`. The layered gate mirrors
ADR-022/024:

- **Boundary crate** (`crates/taskmanager-fd-bridge`): permitted to contain
  `unsafe`; its root carries `#![deny(unsafe_op_in_unsafe_fn)]`. The `unsafe`
  lives in `CMSG_SPACE`/`CMSG_LEN`/`CMSG_FIRSTHDR`/`CMSG_DATA`/`CMSG_NXTHDR`,
  the `mem::zeroed()` `msghdr` initialization, the `CMSG_DATA` payload
  copy, and the `sendmsg`/`recvmsg` calls. Pointer casts use `.cast()` only.
- **Safe seam:** the public API exposes only `send_fd(channel: &impl AsFd,
  fd: &impl AsFd) -> io::Result<()>` and `recv_fd(channel: &impl AsFd) ->
  io::Result<OwnedFd>`. `recv_fd` passes `MSG_CMSG_CLOEXEC` so the received fd
  is close-on-exec (no leak across a later `fork`+`exec` — a privilege-hygiene
  requirement), returns `UnexpectedEof` on a peer orderly-close (`n == 0`)
  distinct from a malformed message, and wraps the duplicated fd in `OwnedFd`
  exactly once (no double-close). No `RawFd`/`AsRawFd`/raw pointer crosses the
  public API.
- **Protocol (out-of-band):** the app binds a throwaway Unix socket, `pkexec`s
  the launcher with the socket path + interface index; the launcher opens the
  `AF_PACKET` socket (via `taskmanager-afpacket::open_packet_fd`), `send_sd`s
  it via `send_fd`, and blocks on a one-byte ACK so it does not exit (and drop
  its fd reference) before the kernel has duplicated the fd into the app's
  table — closing the close-before-transfer race. The app `recv_fd`s, ACKs, and
  runs the capture loop on the owned fd.

### Trust-root invariants (CI-enforced)

`tests/logic/workspace_architecture_test/dependency_firewall.rs` enforces:

1. `default_build_is_strict_safe_rust_with_zero_unsafe` allowlists all four
   boundary dirs (`perf-ioctl`, `afpacket`, `fd-bridge`, `windows-api`); every
   other production source stays `unsafe`-free.
2. `audited_boundary_crate_carries_its_own_unsafe_contract` checks all four:
   root carries `#![deny(unsafe_op_in_unsafe_fn)]`; every `unsafe` block/fn has
   a `// SAFETY:` comment; no forbidden cast/`impl AsRawFd`/raw pub handle.
3. `audited_fd_bridge_boundary_crate_is_depended_on_only_by_sanctioned_consumers`
   is a reverse firewall (SUBSET check): only `taskmanager-escalation` (the
   unprivileged `recv_fd` side, in `invoke_net_launcher`) and
   `taskmanager-net-launcher` (the privileged `send_fd` side) may depend on it;
   the boundary crate has zero workspace deps (only `libc`).

## Consequences

- **The fd-passing primitive is headless-verifiable:** `send_fd`/`recv_fd` over
  an in-process `socketpair` needs no privilege, so the boundary crate ships a
  round-trip unit test proving the duplicated fd is the same open file
  description. This is the one part of the per-process-network chain that IS
  runtime-verified in CI.
- **The orchestration is on-box-unverified:** the live `pkexec` + `CAP_NET_RAW`
  + `SCM_RIGHTS` handoff (`invoke_net_launcher` in `taskmanager-escalation`)
  cannot be exercised headless (no active polkit session, no capability); it is
  verified structurally + via the mocked process seam, and on-box by the
  integrator.
- **The consumer wiring is the on-box step:** `invoke_net_launcher →
  recv_fd → PacketSource::from_owned_fd → capture-worker restart` is NOT yet
  wired into the live provider; today an unprivileged host sees
  `RequiresEscalation(PerProcessNet)` and a `CAP_NET_RAW` host runs the worker.
  Closing this link is a focused boundary completion, not a re-design.
- **Four trust roots, each minimal:** the audited surface stays one OS/ABI
  family per crate; the fd-bridge owns the Unix fd-transfer and pidfd/peer-
  credential seams without exposing raw handles to its consumers.

## Alternatives considered

- **Put `SCM_RIGHTS` in `taskmanager-afpacket`:** rejected — it would
  technically still pass the architecture test (any `unsafe` with a `SAFETY:`
  comment is allowed), but it violates afpacket's own stated contract ("this
  crate's `unsafe` ends at `recvfrom`") and mixes two unrelated kernel ABI
  surfaces in one audited crate — exactly the smell the boundary policy rejects. A third
  minimal crate keeps each trust root single-surface.
- **Inherit the fd across the `pkexec` `exec`:** rejected — polkit/pkexec
  sanitize the child fd table; the fd does not survive. The address-passed
  Unix socket + `SCM_RIGHTS` is the portable mechanism.
- **A long-lived privileged capture daemon:** rejected (ADR-023) — it re-creates
  a continuously-privileged process (larger attack surface). The one-shot
  launcher opens + passes the fd + exits; only the kernel object survives,
  owned by the unprivileged app.
