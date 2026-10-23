# taskmanager-app-host

## Role

Toolkit-neutral native composition root. It assembles configuration, history,
runtime services, platform clients and the selected product frontend.

## Boundary

This crate is the executable composition edge: it selects the native adapter and
owns the resulting paths and safe callbacks, but does not inspect OS files,
define domain rules, provider semantics, page state, or toolkit widgets.

`native_capability_surface()` is the layer-B pass-through a frontend folds its
feature declaration against: it returns the selected adapter's static
`(capability -> source)` declaration, which the adapter co-locates with its real
route/provider registration. It is declaration data, never an OS read; runtime
availability stays with the capability catalog snapshot.

Each bounded worker (configuration, history writer/replay, snapshot, diagnostic,
window capture) has one process-wide owner here with typed, fixed-capacity
lanes; frontends only submit immutable typed requests and drain correlated
completions. Worker boundaries, the local-time cache and the current capture
adapters live in [`docs/HOST_WORKERS.md`](docs/HOST_WORKERS.md).

## Surface presentation contract

The `WindowPresentation` contract — definition, per-surface role rules, the
`LayerShellSpec` value set and the current GPUI desktop-widget profile — is
specified in [`docs/WINDOW_PRESENTATION.md`](docs/WINDOW_PRESENTATION.md).

## Contract and verification

Cross-crate cache and lifecycle ownership is defined by
[`docs/STATE_OWNERSHIP.md`](../../docs/STATE_OWNERSHIP.md); this crate expands
only the app-host workers and startup-cache policy.

Composition must be deterministic, typed and portable across target OSes.
Keep platform selection here rather than in core or a renderer; verify all
target cfg paths and the native composition architecture tests.

## Module map

```text
src/lib.rs                       composition root: runtime + native client + surface roles
src/mode.rs                      AppHostMode probe: daemon subscription vs standalone fallback (ADR-055)
src/presentation.rs              toolkit-neutral WindowPresentation contract (ADR-037)
src/history_frontend.rs          HistoryFrontendSession (replay client + writer ownership)
src/history_persistence_runtime.rs (+ health.rs)   bounded persistence generation
src/history_replay_runtime.rs    read-only replay client
src/snapshot_export_runtime.rs  src/window_capture_runtime.rs
src/process_termination.rs     src/diagnostic_bundle_runtime.rs
src/worker_fault.rs              worker fault accounting
```

## Docs

- [`docs/HOST_WORKERS.md`](docs/HOST_WORKERS.md) — per-worker ownership, lanes, shutdown and
  the startup local-time cache.
- [`docs/WINDOW_PRESENTATION.md`](docs/WINDOW_PRESENTATION.md) — the layer-shell surface
  contract and current desktop-widget profile.
- [`HOST_ARCHITECTURE`](../../docs/HOST_ARCHITECTURE.md) and
  [ADR-037](../../adr/037-parallel-frontend-hosts-and-layer-shell-contract.md) — host
  composition and the layer-shell contract.
