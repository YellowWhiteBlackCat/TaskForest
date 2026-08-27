# ADR-023: Per-feature privilege-escalation framework (privileged-helper design)

- Status: Accepted
- Relates to: [ADR-022 (audited perf_event_open boundary crate)](022-audited-perf-boundary-crate.md)
  and [权限与信任边界](../docs/PERMISSION_MODEL.md).
- Operationalizes: Boundary 2 (default-unprivileged) and Boundary 3 (direct vs
  requires-escalation) of [`docs/PERMISSION_MODEL.md`](../docs/PERMISSION_MODEL.md)

## Context

`docs/PERMISSION_MODEL.md` sets three boundaries. Boundary 1 (safe-Rust) says
business crates are `#![forbid(unsafe_code)]` and native ABI access stays in the
four audited boundary crates. Boundary 2 is the product principle that **the app runs unprivileged by
default on every platform** — it never launches elevated and the binary never
carries a blanket capability (no `setcap` / `setuid`). Boundary 3 classifies
every telemetry/control domain as *direct* (pure safe Rust, no privilege) or
*requires escalation* (needs a capability/syscall an unprivileged user lacks).

The remaining escalation domains include Intel i915 per-engine utilization
(PMU), per-process network rates (`AF_PACKET` / `CAP_NET_RAW`), ATA/SATA SMART
(`smartctl`, often root), control of foreign-uid processes
(`CAP_KILL` / `CAP_SYS_PTRACE`), and system service start/stop (systemd polkit
/ OpenRC root). Each remains **honestly typed-degraded**
(`PermissionDenied` / `Unsupported`, never a fabricated number or zero) — but
there was no single place that named the escalation path or the contract a
feature must satisfy to use it.

The owner's direction (recorded in the permission model) is a **privileged-
helper architecture**: the main app NEVER holds privilege; a small helper is
invoked through the OS-native escalation prompt — polkit `.policy` + `pkexec`
on Linux, an elevated helper + UAC on Windows, an auth prompt on macOS — and
performs ONLY the privileged op, returning safe typed data. Privilege is
granted only when the user actively uses a specific feature, only for that
operation, and refused/missing data stays typed-degraded.

## Decision

The framework is a zero-dependency, pure-safe-Rust
seam crate `crates/taskmanager-escalation` that names the escalation-column
features and the contract for probing them, plus the honest default gate.

### The seam (`crates/taskmanager-escalation`)

`#![forbid(unsafe_code)]`, no dependencies beyond `std`/`core`:

- `enum EscalationFeature { IntelPmu, PerProcessNet, AtaSmart,
  ForeignProcessControl, SystemServiceControl }` — the escalation-column
  features from Boundary 3, one variant per domain so the UI can offer exactly
  that prompt. `Debug + Clone + Copy + Eq + Hash`.
- `enum EscalationDenialReason { Unsupported, PermissionDenied,
  HelperUnavailable }` — typed reason a feature is denied, so callers react
  without string parsing.
- `enum EscalationAvailability { Available, RequiresEscalation(EscalationFeature),
  Denied { reason: EscalationDenialReason } }` — the typed probe result. Honesty
  red line: missing data is a typed variant, never a fabricated value or zero.
- `trait PrivilegeGate { fn probe(&self, feature: EscalationFeature) ->
  EscalationAvailability; }` — the one-method seam a feature asks before
  touching a privileged path. Probing is side-effect-free: it never grants a
  capability and never launches an elevated process.
- `struct UnprivilegedGate; impl PrivilegeGate for UnprivilegedGate` — returns
  `RequiresEscalation(feature)` for EVERY variant. This is the honest default:
  a freshly started TaskForest has escalated nothing yet, so every escalation
  feature requires escalation — never `Available` (would fabricate access) and
  never `Denied` (would hide a real opportunity behind a hard refusal).

### The privileged-helper design

The runtime shape is:

1. **The main app stays unprivileged.** It constructs `UnprivilegedGate` (or a
   richer gate once the helper exists) and asks `probe(feature)` before each
   privileged path. On `RequiresEscalation` it offers the user the single
   OS-native prompt for that one feature.
2. **The helper holds the capability, not the app.** A small binary — `pkexec`
   target on Linux (with an installed polkit `.policy` that names the exact
   action and keeps it on the single feature), an elevated helper + UAC
   manifest on Windows, an authorization-required helper on macOS — performs
   ONLY the privileged op for the requested feature and returns safe typed
   data over a defined IPC boundary. The main app never receives a raw
   capability; it receives typed results.
3. **One feature at a time.** There is no blanket "run as root" mode. Each
   prompt authorizes one operation; nothing is escalated proactively at start.
4. **Typed degradation stays the default.** On `Denied { reason }` — restrictive
   host, helper missing, or user refusal — the feature stays
   `PermissionDenied` / `Unsupported` and the rest of the sample is
   unaffected. No fabricated data, ever.

### Relationship to the safe-Rust boundaries

This framework is orthogonal to Boundary 1. The privileged helper is the place
that needs the OS capability (and, where relevant, the audited `unsafe` seam
from ADR-022 for the PMU open) — but the seam crate itself is pure safe Rust,
`#![forbid(unsafe_code)]`, with zero dependencies, so the foundation carries no
new trust root. The reverse firewall (dependency_firewall architecture test)
already accepts it as a leaf crate with no internal deps.

## Consequences

- **Positive:** the escalation path is named and typed. Wiring
  `PrivilegeGate` into the PMU provider / CLI / per-process-net path has
  a single, documented contract to implement against, and the honest default
  (`UnprivilegedGate`) keeps every escalation feature correctly typed-degraded
  until then — no behaviour changes today.
- **Honest cost / remaining:** installed-package helper/policy deployment,
  successful live authorization receipts, Windows/macOS helpers, and a richer
  session gate that reports `Available` after a real helper result remain open.
  `UnprivilegedGate` is still the safe default for features without an
  operational helper; missing or refused helper results remain typed and never
  become fabricated data.
- **No fabrication:** the framework cannot emit `Available` for an escalation
  feature until a real gate is wired, and `UnprivilegedGate` provably returns
  `RequiresEscalation` for every variant (asserted by the crate's unit tests
  and the `escalation_framework` architecture test).
- **No privilege granted by this change:** nothing here is `setcap`/`setuid`;
  the binary stays unprivileged. The framework is the *boundary* the future
  helper will cross, not the crossing itself.

## Current boundary status

- **Built:** `crates/taskmanager-escalation` (seam types + `UnprivilegedGate`),
  this ADR, and the `escalation_framework` workspace architecture test that
  asserts the crate exists, carries `#![forbid(unsafe_code)]`, and that
  `UnprivilegedGate::probe` returns `RequiresEscalation` for every
  `EscalationFeature` variant.
- **Operational boundaries:** the Intel PMU helper and AF_PACKET launcher
  are operational seams, and `taskmanager-process-control-helper` now covers
  one identity-checked foreign-process operation per pkexec invocation. The
  Linux process provider invokes it only after a typed direct permission
  denial. The installed-package helper/policy and successful foreign-uid
  on-box receipt remain open; Windows/macOS equivalents and a richer session
  gate remain unavailable until their native helper paths are implemented and
  validated.

## Alternatives considered

- **Blanket `setcap`/`setuid` on the main binary:** rejected — violates
  Boundary 2. The app must run unprivileged by default; privilege is per-feature
  via an OS prompt, never a blanket capability baked into the binary.
- **Reach escalation features with `std::process::Command` + `sudo`/`pkexec`
  inline from providers:** rejected — there is no single contract, no typed
  probe, and providers would each re-derive the prompt UX. The seam + helper
  keep one audited path and one consistent typed result.
- **Stay permanently typed-degraded and never build the helper:** the default
  remains safe and honest, but the typed-degradation contract is retained so a
  future helper can be added without changing the application vocabulary.
