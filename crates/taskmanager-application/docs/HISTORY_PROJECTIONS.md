# History projections

- `src/persistent_app_history.rs` projects each accepted process snapshot into
  three durable metrics for a deterministic verified-first set of at most 256
  identities; admission is recomputed per snapshot, while the history store
  alone owns cross-window series retention.
- `src/application_history_projection.rs` joins persistent replay rows into the
  one application-history read model consumed by GPUI, Iced, TUI, and Bevy,
  including explicit capability states and timestamp-aware chart gaps.
- `src/performance_history_projection.rs` filters the same accepted request into
  performance rows; `src/history_replay_samples.rs` preserves recording downtime
  without mistaking a downsampled time window for missing collection.
- `src/system_timeline.rs` owns System sections, 1m/5m/15m/60m windows,
  stable immutable curves and raw latest/peak readouts through an inward read
  port, without a telemetry-store dependency. Timestamp-spaced cells
  preserve downtime; unavailable tails do not reuse an older current value.
- `src/process_category_projection.rs` owns category bucket order and typed
  process aggregates. It selects the PSS-preferred display observation, then
  delegates availability, coverage, freshness, failure, and saturating-add
  semantics to core's `AggregateMetric` API; missing values never become a
  successful zero.
- `src/history_replay.rs` owns the persistent-replay request generation and
  `Closed | Loading | Ready | Failed` reducer. Only the current request may
  resolve; late/duplicate completions are inert, failed refreshes retain and
  label their last-good window, and a closed controller rejects refresh/window
  transitions. Published rows are immutable and curve cardinality is bounded.
- `src/boot_baseline.rs` owns boot-evidence request identity, duplicate
  suppression, late-completion rejection and last-good retention. A baseline
  is projected only when the successful evidence matches the current boot;
  failure of a newer boot cannot relabel an older comparison.
