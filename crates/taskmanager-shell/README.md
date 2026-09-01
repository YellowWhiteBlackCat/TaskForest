# taskmanager-shell

## Role

Renderer-neutral state machine shared by GPUI, Iced, TUI, and Bevy (ADR-027): page,
selection, filtering, sorting, effects and `SystemProjectionStore`. Shell is the
composition owner that folds correlated telemetry into the single bounded
`taskmanager-telemetry-store` live graph authority.

## Boundary

Shell folds `PlatformEventBatch` into shared projections but owns no OS I/O,
GPUI Entity, Ratatui widget, Iced widget or native provider selection.

## Key modules

- `src/app/batch_fold.rs` is a typestate batch scheduler. Its compile-time
  phases are failure seed → independent domain systems → revision advancement
  → alert watermark → failure feedback; a caller cannot skip or reorder them.
  The domain-system registry owns disjoint data families and is order-independent.
- Batch activity is `Idle | Updated`, not an inferred combination of local
  booleans. Independent `BatchFoldChanges` remain ECS-style invalidation facts.
- `src/app/lifecycle.rs` owns one typed lifecycle reducer for the one-way quit
  transition and the sole `FeedbackState`. Background activity and point-of-action notices are separate;
  notices carry source, severity and an explicit replacement/platform-batch
  lifetime, so live updates cannot erase a control, settings or clipboard result.
- `src/app/platform_feedback.rs` is the only platform-fold side-effect reducer:
  it updates activity, publishes typed failure/control notices, ingests shared
  history and performs selection/service-log hygiene.
- Each frontend track owns exactly one application dependency lifecycle:
  `ShellApp` for Iced/TUI/Bevy and `DirectTrackState` for GPUI. `OpenServiceLog`
  owns only the feed plus its lifecycle; identity is projected from the
  lifecycle. Effect dispatch begins a typed admission attempt before submission
  and upgrades or rejects that exact attempt.
- Each track caches the read-only runtime capability snapshot through one named
  reducer. GPU-engine presentation combines that status with the application
  request session in one shared fold; the session is the sole accepted payload
  authority, and renderers retain no enable bool or failure-detail mirror.
- The same tracks privately own one `RequestSessions` component for affinity
  reads, process batches, SMART self-tests, GPU-engine acquisition, process-network
  escalation and command/reveal/URL actions. The shared fold emits correlated
  typed terminals once; this component filters wrong-target, wrong-capability,
  replaced, closed, late and duplicate terminals before renderer history,
  feedback, payload projection or invalidation can observe them.
- Each track also privately owns exactly one `SystemProjectionStore`. Renderers use
  `projection()` only; there is no `data_mut`, `projection_mut`, or public writable field.
  Production facts enter through `apply_platform_batch` and named control/alert reducers.
  `fixture::ProjectionSeedFact` and its domain-scoped editors are the deterministic
  demo/capture/test seam and never expose the store itself.
- The store's `AlertCenter` is the sole full managed-rule authority on each
  track. `ShellApp` and `DirectTrackState` expose only immutable managed rules
  and the same typed edit command; frontends cannot replace engine rules or
  persist enabled-state mirrors.
- The same `AlertCenter` also owns bounded activated/cleared event history;
  GPUI's event panel and Iced's event overlay consume its immutable slice,
  while filter/read state remains renderer-local.
- Effect submission folds through `NoSubmission | Rejected | Accepted`; partial
  multi-facet success remains accepted while every rejected facet is reported.
- Inline input ownership is one `ShellInputMode::{Content, Search, Help,
  Suggestions}` machine. Frontends cannot construct parallel open states.
- Each correlated domain is folded in ascending `EventSequence`; application
  projections use ascending typed revision with stable same-revision order.
  Cross-domain sequence is deliberately not a winner because runtime delivery
  is fair across control/observation classes rather than globally sequence-sorted.
- Inventory failures are explicitly seeded before independent domain snapshots,
  so a successful snapshot in the same batch is authoritative; final failure
  feedback runs only after data and revision phases.
- `src/history.rs` maps correlated application outcomes into the telemetry store;
  it does not own a second history representation.
- `src/app/selection.rs`, `sorting.rs`, `process_control.rs` and `effects.rs` own shared interaction.
- Shared dangerous gates consume application `InteractionState`; parallel `pending_*` mirrors are
  forbidden. The GPUI direct track owns that same machine inside `DirectTrackState`;
  its renderer-local window surface never stores shared semantic payloads.
- The direct track's inventory outcomes and runtime notices are fields of the
  same `FeedbackState`; there is no parallel feedback holder. Single-process
  completions are emitted once by the batch fold and reduced into the notice
  lifecycle rather than retained in a second projection slot.
- `ProcessRowId` distinguishes structural category rows, PID-less application
  aggregates and real process rows. An application root PID is a live tree
  lookup key; batch submission freezes its exact leaf-first descendant scope.
- `ProcessRowId`/`ProcessRowAnchor` are the row identity seam:
  process-backed rows use the shell-owned `ProcessRowIdentity` wrapper around
  the core `ProcessIdentity`, while
  `ProcessProjectionGeneration` rejects stale renderer geometry. There is no
  compatibility row-key API.
- `src/presentation.rs` and `src/viewmodel.rs` expose renderer-neutral projections, including the
  product-first GPU display identity that keeps hardware names separate from driver names.
- GPU history retains typed utilization/scalar/engine query windows. The shared chart-metric
  selection model in `presentation/gpu_chart_metric.rs` owns the
  telemetry-store
  series vocabulary, typed availability gating (unavailable series project explicit, never zero
  or hidden), default Utilization, fixed-order cycling, and generation-reset reconcile as one
  pure per-window session state. All supported frontends consume this projection, and the headline
  window read (`gpu_chart_metric_history`) is one generation-scoped typed dispatch over the
  store's `gpu_metric_point_series_for`: the same `GpuChartMetric::value` fold the gate uses,
  consumed by Iced/TUI through the shell history and by GPUI's direct track through its own
  `LiveGraphHistory` view of the same store — no frontend keeps a second sampling fold.

## Contract and verification

Cross-crate ownership, batch order and lifecycle matrices are authoritative in
[`docs/STATE_OWNERSHIP.md`](../../docs/STATE_OWNERSHIP.md); this crate only
implements the shell-owned rows of that ledger.

The same projection must drive rendering, keyboard paging, command feedback and
accessibility summaries. Keep frame commit, stale/current semantics and source
notices typed. Frontends request quit with an explicit reason and only read
`should_quit()`; they render feedback through the read-only projection API.
Verify shell behavior independently of a compositor.

## Module map

```text
src/app.rs                    SystemProjectionStore: one instance per frontend track
src/app/batch_fold/           event folding: failure seed → domain systems → revision
│                             → alert watermark → feedback
src/app/direct_track/         process inventory, sorting, selection
src/app/request_sessions.rs   per-track typed request-session instances
src/app/lifecycle.rs          ShellLifecycleState (quit and feedback)
src/app/effects.rs (+ effect_dispatch.rs)   effect generation and dispatch
src/app/confirmation_gates.rs  process_control.rs  process_requests.rs
src/app/frame.rs  input_mode.rs  selection.rs  search_input.rs
```
