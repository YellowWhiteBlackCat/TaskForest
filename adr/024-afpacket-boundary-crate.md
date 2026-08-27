# ADR-024: Audited AF_PACKET boundary crate (per-process network accounting)

- Status: Accepted
- Extends for this carve-out: [ADR-022 (audited perf_event_open boundary crate)](022-audited-perf-boundary-crate.md),
  [ADR-023 (per-feature privilege escalation)](023-per-feature-privilege-escalation-framework.md)
  and [权限与信任边界](../docs/PERMISSION_MODEL.md).

## Context

The workspace safe-Rust contract leaves per-process network byte rates typed-
`Unsupported` unless a separately audited native boundary is available.
Per-process network byte accounting is a user-visible capability, and the
mechanism question is settled: `/proc` socket tables prove connection
ownership and endpoints but **do not account bytes per PID** — that is exactly
why `nethogs` and friends use `AF_PACKET`. So per-process network rates need a
kernel seam, and pure safe Rust cannot reach one.

The refined safe-Rust principle from ADR-022 (business crates
`#![forbid(unsafe_code)]`; OS/ABI work in ONE minimal audited boundary crate per
kernel surface, exposing only safe APIs) already had one carve-out
(`perf_event_open`). The question was whether `AF_PACKET` qualifies as a second
minimal trust root. It does: it is `socket` + `bind` + `recvfrom` (plus one
`setsockopt` for a recv timeout) on a single raw socket whose fd the crate owns
behind a safe handle — directly analogous to the perf boundary, and far smaller
than an eBPF object/loader/ABI/program surface.

## Decision

Introduce `crates/taskmanager-afpacket` — the workspace's **second** audited
`unsafe` boundary crate — as the seam for a Linux `AF_PACKET` `SOCK_RAW`
socket, used to capture frames for per-process network byte attribution. The
refined principle is encoded as a layered gate, mirroring ADR-022:

- **Boundary crate** (`crates/taskmanager-afpacket`): permitted to contain
  `unsafe`; its root carries `#![deny(unsafe_op_in_unsafe_fn)]` (NOT `forbid`).
  It has exactly four `unsafe` sites: `socket` (open the raw socket +
  `OwnedFd::from_raw_fd`), `bind` (`sockaddr_ll`), `recvfrom` (one frame), and
  `setsockopt` (`SO_RCVTIMEO` so the capture worker can poll a shutdown flag).
  Pointer casts use the `.cast()` method only — never `as *const`/`as *mut`.
- **Packet parsing is PURE SAFE RUST** (`parse.rs`): bounds-checked slicing
  over the borrowed frame yields a typed `FiveTuple` (src/dst IP + protocol +
  ports). No `unsafe`, no panic on malformed input.
- **Safe seam:** the public API exposes only `PacketSource::open`,
  `open_packet_fd` (yield the owned fd for the privileged launcher),
  `from_owned_fd` (wrap a received fd — the app side), and `recv`. `CapturedPacket`
  carries the frame bytes + the `sll_pkttype` direction bit (sent vs received,
  for rx/tx split). No `RawFd`/`AsRawFd`/raw pointer crosses the public API;
  the only `unsafe` forms/reads the kernel fd the crate owns.

### Trust-root invariants (CI-enforced)

`tests/logic/workspace_architecture_test/dependency_firewall.rs` enforces:

1. `default_build_is_strict_safe_rust_with_zero_unsafe` now allowlists
   `crates/taskmanager-perf-ioctl/src` AND `crates/taskmanager-afpacket/src`
   (later also `taskmanager-fd-bridge/src`, ADR-025); every other production
   source stays `unsafe`-free and every non-boundary crate root keeps
   `#![forbid/deny(unsafe_code)]`.
2. `audited_boundary_crate_carries_its_own_unsafe_contract` checks BOTH
   boundary dirs: root carries `#![deny(unsafe_op_in_unsafe_fn)]`; every
   `unsafe {`/`unsafe fn` has a `// SAFETY:` comment on the same line or in the
   contiguous comment block above; no `as *const`/`as *mut`/`as RawFd`, no
   `impl AsRawFd`, and no raw handle/pointer in a `pub` item crosses the seam.
3. `audited_afpacket_boundary_crate_is_depended_on_only_by_sanctioned_consumers`
   is a reverse firewall (SUBSET check): only `taskmanager-platform-linux`
   (the unprivileged capture + attribution loop) and `taskmanager-net-launcher`
   (the privileged open side) may depend on it; the boundary crate has zero
   workspace deps (only `libc`).
4. The dependency DAG lists `taskmanager-afpacket` as a permitted dependency of
   `taskmanager-platform-linux` only (the launcher reaches it through its own
   manifest, sanctioned by the reverse firewall above).

## Consequences

- **True positive:** per-process network byte accounting is reachable when the
  process holds `CAP_NET_RAW`; the product's safe-Rust differentiator holds for
  every business crate. The crate is small (socket/bind/recvfrom/setsockopt +
  a pure-safe parser) — reviewable in one read.
- **Honest degrade:** without `CAP_NET_RAW` (the default unprivileged build),
  `open` returns `Err(PermissionDenied)`; the Linux adapter classifies it as
  `RequiresEscalation(PerProcessNet)` (ADR-023) and offers the OS-native prompt
  rather than fabricating zero bytes. The connection list and aggregate
  interface counters (pure `/proc/net`) are unaffected.
- **The live capture path is on-box-unverified:** headless CI has no
  `CAP_NET_RAW`, so only the failure/degrade path is exercised in tests. The
  privileged path (a `CAP_NET_RAW` host, or the launcher of ADR-023/025 handing
  the fd via `SCM_RIGHTS`) is verified on-box, not in CI.
- **eBPF is not part of this boundary:** its object/loader/ABI/program surface
  is outside the current trust root, while a single socket seam qualifies.

## Alternatives considered

- **Stay strictly zero-unsafe and forgo per-process network rates:** rejected —
  the capability is useful, while the refined principle (one minimal audited
  boundary crate per kernel surface) keeps business crates safe and the native
  seam reviewable.
- **Safe-procfs instead of AF_PACKET:** rejected — `/proc` socket tables carry
  no per-PID byte counters; that is precisely why `nethogs` uses `AF_PACKET`.
  Safe-procfs gives connection-level data, not bytes/s.
- **Restore an eBPF trust root:** rejected — its surface is too large to audit
  as one minimal trust root; `AF_PACKET` is one socket.
- **`setcap cap_net_raw+ep` on the main app binary:** rejected (ADR-023) —
  grants standing raw-socket power to any user with no per-use prompt;
  privilege must flow through the OS-native prompt via a dedicated launcher.
