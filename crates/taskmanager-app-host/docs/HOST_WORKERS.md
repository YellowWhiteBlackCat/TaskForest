# Host workers and startup cache

## Composition constructors

`NativeAppHost::production` selects native user-data paths; `from_paths`
composes an isolated host from explicit locations and a cached time
observation. Neither constructor performs storage work.

## Configuration

`NativeAppHost` lazily owns one shared `ConfigCoordinator`. Cloned hosts and
additional windows receive independent `ConfigClient` cursors backed by that
same worker; no frontend receives a `ConfigStore` or performs config file
I/O. The last host/client handle shuts the worker down through an independent
bounded control seam.

## History

Persistent history has one writer owner: the enabled frontend session.
App-host creates its bounded writer and read-only replay generation together
on a worker and returns both in `HistoryFrontendSession`. Disabled frontend
state sends no request, so it starts no query or writer worker and performs no
history-path access; the connector is an inert typed handle until an enable
request starts the history worker.

The app-host read-only worker owns `HistoryQuery`. Requests and immutable
typed completions cross fixed-capacity lanes; no frontend receives a path or
storage primitive. Concurrent reads may observe an in-progress frontend
append only for that request; they mutate no corruption state, and the next
read sees the completed line. Final-handle shutdown for either worker has an
independent signal and bounded wait; a filesystem operation that never
returns is detached instead of freezing window teardown.

## Snapshot, diagnostics, and capture

Snapshot and diagnostic publication have one process-wide bounded worker
each, started lazily by the first submitted request. Snapshot serialization,
current-directory discovery and the atomic three-file transaction live
entirely here. Diagnostic plans cross the boundary only after application
redaction; one worker serves independent named client completion lanes for
full bundles and service-log exports. Frontends only submit immutable typed
requests, drain correlated completions and project shell feedback; they never
construct a filesystem adapter or spawn a per-window writer.

Current-window PNG capture follows the same host-owned worker boundary. The
host first invokes the registered in-process renderer hook, then the native
adapter selected by the
[platform-native selection matrix](../../taskmanager-platform-native/docs/SELECTION_MATRIX.md).
The worker validates the staged PNG and atomically renames it to the
requested path; it never falls back to a display-wide or fabricated capture.
The receipt records backend provenance in the vocabulary owned by
`taskmanager-platform-contract`.

## Local time cache

The host owns one `StartupLocalTimeCache` shared by every cloned host and
window. Native discovery runs exactly once when the production host is built;
frontend refreshes only clone the cached typed observation. Its explicit
`HostRestartOnly` invalidation policy means a process restart is the sole
refresh boundary. The host exposes neither `TZ`, a zoneinfo path nor a read
callback. Runtime time-zone watching would require a future host coordinator
with typed change publications, not a second frontend cache.
