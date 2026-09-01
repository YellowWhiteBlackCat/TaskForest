# taskmanager-app-host

## Role

Toolkit-neutral native composition root. It assembles configuration, history,
runtime services, platform clients and the selected product frontend.

## Boundary

This crate is the executable composition edge: it selects the native adapter and
owns the resulting paths and safe callbacks, but does not inspect OS files,
define domain rules, provider semantics, page state, or toolkit widgets.

`NativeAppHost` lazily owns one shared `ConfigCoordinator`. Cloned hosts and
additional windows receive independent `ConfigClient` cursors backed by that
same worker; no frontend receives a `ConfigStore` or performs config file I/O.
The last host/client handle shuts the worker down through an independent bounded
control seam.

Persistent history has one writer owner: the enabled frontend session. App-host
creates its bounded writer and read-only replay generation together on a worker
and returns both in `HistoryFrontendSession`. Disabled frontend state sends no
request, so it starts no query or writer worker and performs no history-path
access; the connector is an inert typed handle until an enable request starts
the history worker.

The app-host read-only worker owns `HistoryQuery`. Requests and immutable typed
completions cross fixed-capacity lanes; no frontend receives a path or storage
primitive. Concurrent reads may observe an in-progress frontend append only for that
request; they mutate no corruption state, and the next read sees the completed
line. Final-handle shutdown for either worker has an independent signal and
bounded wait; a filesystem operation that never returns is detached instead of
freezing window teardown.

Snapshot and diagnostic publication have one process-wide bounded worker each,
started lazily by the first submitted request. Snapshot serialization,
current-directory discovery and the atomic three-file transaction live entirely
here. Diagnostic plans cross the boundary only after application redaction; one
worker serves independent named client completion lanes for full bundles and
service-log exports. Frontends only submit immutable typed requests, drain
correlated completions and project shell feedback; they never construct a
filesystem adapter or spawn a per-window writer.

Current-window PNG capture follows the same host-owned worker boundary. Its Linux
adapter uses fixed-argument KDE Spectacle active-window capture on Wayland, the
worker validates the staged PNG and atomically renames it to the requested path.
This release exposes only that Linux one-shot path; unsupported platforms never
fall back to a display-wide or fabricated capture.
The receipt already records a backend enum so Portal Screenshot and continuous
ScreenCast/PipeWire can be added without moving native I/O into a frontend.

The host owns one `StartupLocalTimeCache` shared by every cloned host and
window. Native discovery runs exactly once when the production host is built;
frontend refreshes only clone the cached typed observation. Its explicit
`HostRestartOnly` invalidation policy means a process restart is the sole
refresh boundary. The host exposes neither `TZ`, a zoneinfo path nor a read
callback. Runtime time-zone watching would require a future host coordinator
with typed change publications, not a second frontend cache.

## Surface presentation contract

`WindowPresentation` is the toolkit-neutral request for one frontend-owned
surface. `Standalone` preserves the existing normal-window host. `LayerShell`
contains only owned values from `LayerShellSpec`: layer, anchor, compositor-
selected or explicit size, margins, exclusive zone, keyboard interaction,
output hint, namespace and fallback policy. It carries no Wayland object,
event queue, raw handle or renderer state.

The role is per surface. GPUI, Iced and Bevy may each expose a standalone host
and a layer-shell host while sharing the same application projection. A
layer-shell adapter must probe the compositor capability, validate the
configuration, and either use its own native path or return the typed fallback;
it must not claim normal-window operations such as maximize, minimize or move
when the selected surface role cannot provide them.

The current GPUI opt-in uses `LayerShellSpec::desktop_widget`: a bounded
`520×360` Top-layer surface anchored to the top-right with 16px margins and no
exclusive zone. This profile is separate from the standalone default and from
the generic top-panel constructor, so enabling the widget cannot change normal
desktop-window geometry.

## Contract and verification

Cross-crate cache and lifecycle ownership is defined by
[`docs/STATE_OWNERSHIP.md`](../../docs/STATE_OWNERSHIP.md); this README expands
only the app-host workers and startup-cache policy.

Composition must be deterministic, typed and portable across target OSes.
Keep platform selection here rather than in core or a renderer; verify all
target cfg paths and the native composition architecture tests.

## Module map

```text
src/lib.rs                       composition root: runtime + native client + surface roles
src/presentation.rs              toolkit-neutral WindowPresentation contract (ADR-037)
src/history_frontend.rs          HistoryFrontendSession (replay client + writer ownership)
src/history_persistence_runtime.rs (+ health.rs)   bounded persistence generation
src/history_replay_runtime.rs    read-only replay client
src/snapshot_export_runtime.rs  src/window_capture_runtime.rs
src/process_termination.rs     src/diagnostic_bundle_runtime.rs
src/worker_fault.rs              worker fault accounting
```
