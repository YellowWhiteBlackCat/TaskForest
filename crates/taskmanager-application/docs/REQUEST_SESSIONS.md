# Request sessions, effects, and reviews

- `src/request_session.rs` owns affinity-read, process-batch, SMART-self-test,
  GPU-engine acquisition, process-network escalation and shell UI-action
  admission/terminal lifecycles. Attempts upgrade atomically to platform request
  ids; terminals additionally match their frozen domain identity, capability and
  payload kind. Replace, close, retry, late and duplicate behavior is defined by
  these states rather than frontend `Option + bool + error` or request maps.
- `src/service_lifecycle.rs` owns service dependency and log-stream sessions.
  Admission gets a typed attempt identity before platform submission, accepted
  attempts atomically upgrade to `RequestId`, and only matching target/query
  generations resolve. Close, retry, filter and cursor races retain last-good
  data only within the same target/filter generation.
- `src/snapshot_export.rs` owns immutable export requests and the correlated
  `Closed | Queued | Running | Ready | Failed` lifecycle. Its non-blocking port
  couples admission failure, late/duplicate terminal rejection and close; it
  contains no serializer, current-directory discovery or filesystem adapter.
- `src/window_capture.rs` owns the same typed one-shot lifecycle for a
  current-window PNG. It carries only the destination, receipt
  dimensions/backend and bounded native failure; capture and publication remain
  outside the application layer. The current native one-shot implementation is
  Linux-only; other platforms keep a typed unsupported/unavailable result rather
  than claiming a capture they cannot provide.
- `src/interaction.rs` owns the single shared dangerous-confirmation machine:
  one frozen EndTask/ProcessBatch/Service/Startup/Session/SMART-self-test
  payload, explicit arm/replace/confirm/dismiss transitions, and the sole
  conversion from confirmed intent to platform effect.
- `src/first_run.rs` owns quiet optional-setup discovery, the observed
  descriptor, typed action admission and correlated progress. Frontends own
  explicit review visibility; native completions never open a dismissed surface
  (ADR-040).
- `src/diagnostics.rs` prepares already-redacted diagnostic plans and owns the
  shared Preview/Writing/Complete/Failed review state and request-correlated
  publication port. The app-host owns its worker and file transaction; confirm
  submits the frozen plan once, and closing a frontend session makes a late
  completion inert.
