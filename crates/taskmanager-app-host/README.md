# taskmanager-app-host

## Role

Toolkit-neutral native composition root. It assembles configuration, history,
runtime services, platform clients and the selected product frontend.

## Boundary

This crate is the executable composition edge: it selects the native facade and
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
access; the inert connector control thread itself owns no platform or storage
capability until an enable request.

The app-host read-only worker owns `HistoryQuery`. Requests and immutable typed
completions cross fixed-capacity lanes; no frontend receives a path or storage
primitive. Concurrent reads may observe an in-progress frontend append only for that
request; they mutate no corruption state, and the next read sees the completed
line. Final-handle shutdown for either worker has an independent signal and
bounded wait; a filesystem operation that never returns is detached instead of
freezing window teardown.

Snapshot and diagnostic publication have one process-wide bounded worker each.
Snapshot serialization, current-directory discovery and the atomic three-file
transaction live entirely here. Diagnostic plans cross the boundary only after
application redaction; one worker serves independent named client completion
lanes for full bundles and service-log exports. Frontends only submit immutable
typed requests, drain correlated completions and project shell feedback; they
never construct a filesystem adapter or spawn a per-window writer.

The host owns one `StartupLocalTimeCache` shared by every cloned host and
window. Native discovery runs exactly once when the production host is built;
frontend refreshes only clone the cached typed observation. Its explicit
`HostRestartOnly` invalidation policy means a process restart is the sole
refresh boundary. The host exposes neither `TZ`, a zoneinfo path nor a read
callback. Runtime time-zone watching would require a future host coordinator
with typed change publications, not a second frontend cache.

## Contract and verification

Cross-crate cache and lifecycle ownership is defined by
[`docs/STATE_OWNERSHIP.md`](../../docs/STATE_OWNERSHIP.md); this README expands
only the app-host workers and startup-cache policy.

Composition must be deterministic, typed and portable across target OSes.
Keep platform selection here rather than in core or a renderer; verify all
target cfg paths and the native composition architecture tests.
