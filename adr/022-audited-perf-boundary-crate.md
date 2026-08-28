# ADR-022: Audited perf_event_open boundary crate

- Status: Accepted
- Relates to: [权限与信任边界](../docs/PERMISSION_MODEL.md) 和
  [跨平台策略](../docs/CROSSPLATFORM_STRATEGY.md)。

## Context

The workspace safe-Rust contract removes `unsafe` from business crates, and the
workspace architecture test
(`default_build_is_strict_safe_rust_with_zero_unsafe`) enforces it as a CI
contract. On mainline i915 the per-engine `busy` sysfs node may be absent, so
per-engine utilization stays typed-empty — `intel_gpu_top` reads the i915
`perf_event_open` PMU instead, which is unreachable from pure safe Rust.

The owner refined the safe-Rust principle rather than abandoning it:

- **business crates stay `#![forbid(unsafe_code)]`** — the default build is
  still pure safe Rust for everything users and almost all telemetry touch; and
- **each OS/driver ABI surface lives in a minimal, audited boundary crate**
  that exposes ONLY safe APIs.

The question was whether `perf_event_open` qualifies as a minimal trust root.
It does: it is a single self-contained syscall plus two `ioctl` controls,
behind a safe handle that owns the kernel fd. eBPF, by contrast, was an entire
BPF object + loader + ABI + program-contract surface — too large to audit as
one trust root, and it stayed removed.

## Decision

Introduce `crates/taskmanager-perf-ioctl` — one of the workspace's four audited
`unsafe` trust roots — as the boundary for the Linux `perf_event_open(2)`
syscall and its `PERF_EVENT_IOC_*` ioctl controls, used to read Intel i915 PMU
per-engine busy counters. The refined principle is encoded as a layered gate:

- **Boundary crate** (`crates/taskmanager-perf-ioctl`): this crate is one of
  the four permitted production `unsafe` roots. Its crate root carries
  `#![deny(unsafe_op_in_unsafe_fn)]` (NOT `forbid` — forbid would disallow the
  audited opt-out). It has exactly two `unsafe` sites:
  1. `perf_event_open` — the `libc::syscall(SYS_perf_event_open, …)` call and
     the `OwnedFd::from_raw_fd` ownership transfer in one audited block; and
  2. `ioctl` — `libc::ioctl(file.as_raw_fd(), …)` on a `File` the crate owns.
- **Every business crate** remains `#![forbid(unsafe_code)]`. The i915 PMU
  DISCOVERY (`crates/taskmanager-platform-linux/.../gpu/intel/pmu.rs`) is pure
  safe Rust — it only builds the config values; the boundary crate does the
  open/read.
- **Safe seam:** the public API (`GpuEngineCounter`) exposes only
  `open`/`open_enabled`/`read_counter`/`enable`/`disable`/`reset`. No raw
  pointer, `RawFd`, or `AsRawFd` crosses the public API; the only `unsafe` is
  forming the kernel fd into an `OwnedFd`/`File` the crate owns, and the
  audited `ioctl` on that owned `File`.

### Trust-root invariants (CI-enforced)

`tests/logic/workspace_architecture_test/dependency_firewall.rs` enforces:

1. `default_build_is_strict_safe_rust_with_zero_unsafe` skips the explicit four
   root allowlist while still forbidding `unsafe {/impl/fn/trait` +
   `allow(unsafe_code)` and requiring `#![forbid/deny(unsafe_code)]` on every
   OTHER crate root — the incremental gate holds for all other crates.
2. `audited_boundary_crate_carries_its_own_unsafe_contract` checks the
   boundary crate specifically: its root carries
   `#![deny(unsafe_op_in_unsafe_fn)]`; every `unsafe {` block and `unsafe fn`
   has a `// SAFETY:` comment on the same line or in the contiguous comment
   block immediately above; and no raw-pointer cast (`as *const`, `as *mut`,
   `as RawFd`), `impl AsRawFd`, or raw handle/pointer in a `pub` item crosses
   the seam.
3. `audited_perf_boundary_crate_is_depended_on_only_by_the_linux_adapter_and_helper`
   enforces the reverse firewall: only the Linux adapter and the
   feature-specific privilege helper may depend on this boundary crate, and the
   boundary crate has zero workspace deps.
4. The dependency DAG test lists `taskmanager-perf-ioctl` only at those two
   sanctioned composition edges.

### cargo-geiger

CI runs `cargo geiger` as defense-in-depth (`.github/workflows/ci.yml`). The
authoritative gate is the architecture test above (ripgrep-based, fast,
deterministic, runs on every build via the nextest step); geiger is a
cross-check of the dependency tree and is kept non-blocking because geiger
releases can lag the repository's compatibility floor (1.97.1) and the current
stable developer channel. If geiger stabilises across both toolchain lanes it
can be promoted to blocking; the architecture test holds the line regardless.

## Consequences

- **True positive:** Intel i915 per-engine busy utilization is reachable on
  mainline i915 (where the sysfs `busy` node is absent) via the PMU, while the
  product's "pure safe Rust for everything users touch" differentiator is
  preserved and audited. This boundary crate is ~150 lines with exactly two
  `unsafe` sites — small enough to review in one read.
- **Honest limit:** the `xe` driver's two-counter ticks path (engine-busy plus
  a total-ticks counter needing a `Delta`-wrapped config and its own PMU event
  ids) is not implemented here. On-box validation of the i915 PMU read against
  `intel_gpu_top` is still required because CI has no Intel GPU.
- **No fabrication:** when the i915 PMU is absent or `perf_event_open` is
  denied (a restrictive `perf_event_paranoid`), the per-engine breakdown stays
  typed-empty / `FailureKind::PermissionDenied` and the rest of the GPU sample
  (frequency, RC6) is unaffected — the same honest-None convention as before.
- **eBPF is not part of this boundary:** its object, loader and ABI are outside
  the current production trust roots; the single `perf_event_open` call remains
  independently audited.

## Alternatives considered

- **Stay strictly zero-unsafe and forgo Intel engine breakdown:** rejected —
  the capability is useful, while the refined principle (one minimal audited
  boundary crate) keeps business crates safe and the native seam reviewable.
- **`nix` or `bindgen` for the syscall:** rejected — the boundary crate takes
  `libc` only (no nix, no bindgen), keeping the trust root minimal and the
  audited surface to two hand-reviewed `unsafe` sites.
- **Restore eBPF as the trust root instead of perf_event_open:** rejected —
  eBPF's surface (BPF object + loader + ABI + program contract) is too large to
  audit as one minimal trust root; perf_event_open is one syscall.
