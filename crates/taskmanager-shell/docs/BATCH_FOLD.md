# Batch fold and feedback

- `src/app/batch_fold.rs` is a typestate batch scheduler. Its compile-time
  phases are failure seed → independent domain systems → revision advancement
  → alert watermark → failure feedback; a caller cannot skip or reorder them.
  The domain-system registry owns disjoint data families and is
  order-independent.
- Batch activity is `Idle | Updated`, not an inferred combination of local
  booleans. Independent `BatchFoldChanges` remain ECS-style invalidation facts.
- `src/app/lifecycle.rs` owns one typed lifecycle reducer for the one-way quit
  transition and the sole `FeedbackState`. Background activity and
  point-of-action notices are separate; notices carry source, severity and an
  explicit replacement/platform-batch lifetime, so live updates cannot erase a
  control, settings or clipboard result.
- `src/app/platform_feedback.rs` is the only platform-fold side-effect reducer:
  it updates activity, publishes typed failure/control notices, ingests shared
  history and performs selection/service-log hygiene.
- Each correlated domain is folded in ascending `EventSequence`; application
  projections use ascending typed revision with stable same-revision order.
  Cross-domain sequence is deliberately not a winner because runtime delivery
  is fair across control/observation classes rather than globally
  sequence-sorted.
- Inventory failures are explicitly seeded before independent domain snapshots,
  so a successful snapshot in the same batch is authoritative; final failure
  feedback runs only after data and revision phases.
- `src/history.rs` maps correlated application outcomes into the telemetry
  store; it does not own a second history representation.

The direct track's inventory outcomes and runtime notices are fields of the
same `FeedbackState`; there is no parallel feedback holder. Single-process
completions are emitted once by the batch fold and reduced into the notice
lifecycle rather than retained in a second projection slot.
