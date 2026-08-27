# ADR-036: In-process opt-in history persistence

Status: accepted. This is the current writer-ownership decision for opt-in
history persistence; storage format, retention policy and typed history
vocabulary are defined by the implementation and current state contract.

## Context

The independent history collector kept a second runtime alive after every
frontend exited. That split lifecycle required OS services or login autostart,
made the user-facing setting indirect, duplicated composition and lock
ownership, and made packaging responsible for a process that the frontend could
not stop. It also meant the GUI was only a read-only client of its own history.

## Decision

1. `Config.history_persistence` remains opt-in and defaults to `false`.
2. An enabled frontend session owns one `HistoryFrontendSession` containing its
   read-only replay client and its `HistoryPersistenceWriter`. app-host creates
   the bounded persistence generation on a worker and returns it through the
   typed connector completion; the UI thread never performs filesystem I/O.
3. Shell/application ingestion supplies accepted system, sensor, power, and
   process-snapshot facts to the session's `HistoryRecordSink`. Frontends do not
   read platform sources or construct a second history fold.
4. Disabling history drops the session and releases its writer generation. A
   frontend process exit has the same effect. The unique history lock is
   therefore scoped to the active frontend session; a second writer fails
   closed while read-only replay remains request-local.
5. No standalone history executable, systemd unit, LaunchAgent, registry Run
   entry, or package/autostart installation is provided. Existing packages
   contain only the frontend and ordinary optional helpers.
6. Recording while every frontend is closed is intentionally not provided by
   this ADR. A future cross-frontend session owner or IPC design requires a
   separate trust-boundary ADR and must not reintroduce an implicit collector.

## Consequences

- The history setting now controls the writer in the process the user started;
  shutdown and lock release are observable and bounded.
- History does not continue while the frontend is closed. The UI copy and
  replay gap semantics must state this rather than promise background capture.
- Packaging and developer installation no longer create service/autostart
  artifacts, and the deleted collector crate cannot drift from the frontend
  runtime.
- A single writer lock still protects the history directory. Concurrent
  frontends can query persisted data, but only the session that owns the lock
  records new samples.

## Verification

- app-host tests prove disabled resources are absent, connector completion
  returns the writer and replay pair, and bounded shutdown releases the next
  generation's writer lock;
- shell/frontend tests prove accepted telemetry and process snapshots reach the
  sink only while the session is active;
- packaging and install-manager checks prove no collector binary, service,
  LaunchAgent, registry Run entry, or activation link is staged;
- workspace lockfile, formatting, targeted tests, and the repository quick gate
  pass before release.
