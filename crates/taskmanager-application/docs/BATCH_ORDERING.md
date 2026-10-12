# Batch ordering and scheduled telemetry

- `src/platform/client/scheduler.rs` owns the closed automatic
  cadence/dispatcher registry; runtime route construction consumes that same
  authority.
- `src/platform/event_batch.rs` owns the frontend batch ordering seam. Runtime
  `EventSequence` is canonical only inside each correlated domain after fair
  control/observation delivery; application projections use typed revision.
  The batch defines no cross-domain last-writer order.

Scheduled system telemetry submits only the domains actually due from the
runtime. Those domains share one application revision for an atomic projection,
while each retains independent request correlation and failure rollback. Pending
correlation is replaced per domain rather than globally cleared or accumulated.
Every drained batch is stable-sorted inside each domain before it leaves the
application, and the shell repeats that normalization defensively for alternate
typed producers. Cross-domain folds must commute unless the shell declares an
explicit precedence phase; struct field order is never execution order.
